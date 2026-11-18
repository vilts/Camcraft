//! Boundary picks (M11.5 UX, the user's hand-test): what an entry's picks
//! spell (open chain / closed rings) is read before Generate; an open
//! chain in an include / exclude is refused at Apply by name (the
//! generate-time refusal is the backstop, unchanged); a picked run is
//! one edge, several runs join into one open chain, Ctrl-toggling a run
//! out of a face loop opens it and Extend to loop closes it again; and
//! every pick made before runs existed resolves exactly as it did.
//! Provenance: our own tests, specced from the page's contract.
//!
//! Gate map (M11.5): covers app::operation (boundary_picks, boundary_ctx,
//! stitch, selection), app::commands (EditOperationParams::with_area,
//! SetOperationGeometry), geometry::feature (runs).

mod area_support;

use area_support::*;
use camcraft_app::operation::PickShape;

fn north(world: &World) -> FeatureSel {
    rim_edge(world, |q| (q[1] - 20.0).abs() < 1e-9)
}

fn rim(world: &World) -> Vec<GeoSel> {
    [
        rim_edge(world, |q| (q[1] - 20.0).abs() < 1e-9),
        rim_edge(world, |q| (q[0] - 20.0).abs() < 1e-9),
        rim_edge(world, |q| (q[1] + 20.0).abs() < 1e-9),
        rim_edge(world, |q| (q[0] + 20.0).abs() < 1e-9),
    ]
    .into_iter()
    .map(|f| GeoSel { entity: "part".into(), feature: f })
    .collect()
}

fn shape(world: &World, picks: &[GeoSel]) -> PickShape {
    operation::boundary_pick_shape(world, None, picks, &face(10.0))
}

#[test]
fn the_picks_spell_an_open_chain_or_closed_rings_before_generate() {
    let world = block_world();
    let all = rim(&world);
    assert!(matches!(shape(&world, &all[..1]), PickShape::Open { picks: 1, .. }));
    // Three sides of the rim join end to end into one open chain.
    match shape(&world, &all[..3]) {
        PickShape::Open { picks, chain } => {
            assert_eq!(picks, 3);
            assert_eq!(chain.len(), 4, "three sides, four corners: {chain:?}");
        }
        other => panic!("three sides spell an open chain: {other:?}"),
    }
    assert_eq!(shape(&world, &all[..3]).summary(), "3 edges, open chain");
    assert!(shape(&world, &all).is_closed(), "the four sides close");
    let top = pick("part", top_face(&world, "part", 10.0));
    assert_eq!(shape(&world, &top).summary(), "1 face loop, closed");
    // #1727: two opposite sides do not touch - two open chains, the
    // area between them (touching chains are refused, boundary_chains).
    let apart = vec![all[0].clone(), all[2].clone()];
    assert!(matches!(shape(&world, &apart), PickShape::Chains { picks: 2, ref chains } if chains.len() == 2));
    assert!(matches!(shape(&world, &[]), PickShape::Invalid(_)));
}

#[test]
fn an_open_chain_in_an_include_is_refused_at_apply_and_the_open_role_takes_it() {
    let world = block_world();
    let mut h = History::default();
    h.execute(&world, CreateOperation::new(&world, "F", TOOL, SrcRef::Stock, face(10.0)).expect("create"));
    let e = model::find_by_name(&world, "F").expect("op");
    let params = model::operation_of(&world, &e).expect("op").params;
    let edge = pick("part", north(&world));
    let bad = AreaControls {
        extra: vec![ExtraBoundary { mode: BoundaryMode::Include, shape: BoundaryShape::Picks(edge.clone()) }],
        ..Default::default()
    };
    let err = EditOperationParams::with_area(&world, "F", params.clone(), bad.clone()).err().expect("refused at Apply");
    let msg = err.to_string();
    assert!(msg.contains("boundary 1 (Include) is an open chain (1 edge, open chain)"), "{msg}");
    assert!(msg.contains("Extend to loop") && msg.contains("role to Open"), "both ways out are named: {msg}");
    assert_eq!(model::operation_of(&world, &e).expect("op").area, AreaControls::default(), "nothing moved");
    // The generate-time refusal is still the backstop.
    let backstop = run(&world, &op(SrcRef::Stock, face(10.0), bad)).expect_err("backstop");
    assert!(backstop.contains("must close"), "{backstop}");
    // The same picks as the open boundary are accepted and undo.
    let good = AreaControls { open: Some(OpenBoundary { picks: edge, side: ChainSide::Inside }), ..Default::default() };
    h.execute(&world, EditOperationParams::with_area(&world, "F", params.clone(), good.clone()).expect("open is fine"));
    assert_eq!(model::operation_of(&world, &e).expect("op").area, good);
    assert!(h.undo(&world));
    assert_eq!(model::operation_of(&world, &e).expect("op").area, AreaControls::default());
    // A closed include passes the door.
    let closed = AreaControls {
        extra: vec![ExtraBoundary { mode: BoundaryMode::Include, shape: BoundaryShape::Picks(rim(&world)) }],
        ..Default::default()
    };
    assert!(EditOperationParams::with_area(&world, "F", params, closed).is_ok());
}

#[test]
fn a_toggled_run_opens_a_face_loop_and_extend_to_loop_closes_it() {
    let world = block_world();
    let top = pick("part", top_face(&world, "part", 10.0));
    let edge = GeoSel { entity: "part".into(), feature: north(&world) };
    let (opened, removed) = operation::toggle_run(&world, &top, &edge);
    assert!(removed, "the clicked side came out of the face loop");
    assert_eq!(opened.len(), 3, "the loop broke into its other three runs: {opened:?}");
    assert!(!opened.contains(&edge));
    assert!(matches!(shape(&world, &opened), PickShape::Open { picks: 3, .. }));
    // Toggling it back in closes the ring again; toggling once more opens it.
    let (back, removed) = operation::toggle_run(&world, &opened, &edge);
    assert!(!removed);
    assert!(shape(&world, &back).is_closed());
    let (again, removed) = operation::toggle_run(&world, &back, &edge);
    assert!(removed && shape(&world, &again).is_open());
    // Extend to loop from the one edge: the top face's whole rim, as runs.
    let closed = operation::extend_to_loop(&world, std::slice::from_ref(&edge)).expect("a loop");
    assert_eq!(closed.len(), 4);
    assert!(closed.contains(&edge));
    assert!(shape(&world, &closed).is_closed());
    assert!(operation::extend_to_loop(&world, &top).is_err(), "no edge picked - named, nothing guessed");
    // A run pick from the viewport is the very chain the old click made
    // on a box side (the run law never re-keys an unsplit chain).
    let e = model::find_by_name(&world, "part").expect("model");
    let f = model::features_of(&world, &e).expect("features");
    let FeatureSel::Chain(id) = edge.feature else { panic!("a chain") };
    let c = feature::resolve_chain(&f, id).expect("resolves");
    assert_eq!(operation::run_pick(&world, "part", c.edges[0]), Some(edge));
}

#[test]
fn every_old_chain_pick_resolves_exactly_as_before() {
    let world = block_world();
    let e = model::find_by_name(&world, "part").expect("model");
    let f = model::features_of(&world, &e).expect("features");
    for c in feature::chains(&f) {
        let r = feature::resolve_chain(&f, c.id).expect("an old id resolves");
        assert_eq!((r.edges, r.closed), (c.edges, c.closed));
    }
    // A face's open rim, picked as runs and as the old open boundary,
    // generates the same path.
    let edge = north(&world);
    let a = AreaControls { open: Some(OpenBoundary { picks: pick("part", edge), side: ChainSide::Inside }), ..Default::default() };
    let top = top_face(&world, "part", 10.0);
    let (t, _) = run(&world, &op(sel("part", top), face(10.0), a)).expect("open boundary on one run");
    assert!(!t.moves.is_empty());
}

#[test]
fn an_open_boundary_of_three_runs_clears_inside_the_u_it_draws() {
    let world = block_world();
    let all = rim(&world);
    // North, east and south sides: a U opening west, run on straight to
    // the stock. The U's mouth side holds less of the stock than the
    // rest, so it is the Outside side of the law.
    let a = AreaControls { open: Some(OpenBoundary { picks: all[..3].to_vec(), side: ChainSide::Outside }), ..Default::default() };
    let (t, _) = run(&world, &op(SrcRef::Stock, pocket(10.0, 8.0), a)).expect("a three-edge open chain");
    let f = feeds(&t);
    assert!(!f.is_empty());
    assert!(f.iter().all(|p| p.x + R <= 20.0 + EPS && p.y.abs() + R <= 20.0 + EPS), "never across the chain");
    assert!(max_by(&f, |p| -p.x) > 20.0 + R, "runs on out of the U's mouth");
}

fn loop_segments_in_setup(world: &World, setup: Option<&str>, picks: &[GeoSel]) -> Vec<([i64; 3], [i64; 3])> {
    let quant = |p: Point3| -> [i64; 3] {
        [(p.x * 1e4).round() as i64, (p.y * 1e4).round() as i64, (p.z * 1e4).round() as i64]
    };
    let e = model::find_by_name(world, "part").expect("part");
    let f = model::features_of(world, &e).expect("features");
    let loc = camcraft_app::setup_align::pose_in(world, &e, setup).unwrap_or_default();
    let mut segs: Vec<([i64; 3], [i64; 3])> = picks
        .iter()
        .map(|g| {
            let FeatureSel::Chain(id) = g.feature else { panic!("expected chain pick, got {g:?}") };
            let c = feature::resolve_chain(&f, id).expect("chain resolves");
            let pts = feature::chain_points(&f, &c);
            let a = quant(camcraft_app::geometry::transform_point(
                Point3::new(pts[0][0], pts[0][1], pts[0][2]),
                loc.pos,
                loc.rot,
            ));
            let b = quant(camcraft_app::geometry::transform_point(
                Point3::new(pts[pts.len() - 1][0], pts[pts.len() - 1][1], pts[pts.len() - 1][2]),
                loc.pos,
                loc.rot,
            ));
            if a <= b { (a, b) } else { (b, a) }
        })
        .collect();
    segs.sort();
    segs
}

fn edge_at_setup_midpoint(world: &World, setup: Option<&str>, target: Point3) -> GeoSel {
    let e = model::find_by_name(world, "part").expect("part");
    let f = model::features_of(world, &e).expect("features");
    let loc = camcraft_app::setup_align::pose_in(world, &e, setup).unwrap_or_default();
    let c = feature::chains(&f)
        .into_iter()
        .find(|c| {
            if c.closed {
                return false;
            }
            let pts = feature::chain_points(&f, c);
            let a = camcraft_app::geometry::transform_point(
                Point3::new(pts[0][0], pts[0][1], pts[0][2]),
                loc.pos,
                loc.rot,
            );
            let b = camcraft_app::geometry::transform_point(
                Point3::new(pts[pts.len() - 1][0], pts[pts.len() - 1][1], pts[pts.len() - 1][2]),
                loc.pos,
                loc.rot,
            );
            let mid = Point3::new(0.5 * (a.x + b.x), 0.5 * (a.y + b.y), 0.5 * (a.z + b.z));
            (mid.x - target.x).abs() < 1e-6 && (mid.y - target.y).abs() < 1e-6 && (mid.z - target.z).abs() < 1e-6
        })
        .expect("matching edge chain");
    GeoSel { entity: "part".into(), feature: FeatureSel::Chain(c.id) }
}

fn rotated_block_world(deg: [f64; 3], aligned: bool) -> (World, History, Point3) {
    let raw = cube(Point3::new(-20.0, -10.0, 0.0), Point3::new(20.0, 10.0, 10.0));
    let rot = camcraft_app::setup_align::rotation_from_angles(deg).expect("rot");
    let loc = camcraft_app::setup_align::about_centre(rot, Point3::new(0.0, 0.0, 5.0)).unwrap_or_default();
    let mesh = if aligned {
        raw
    } else {
        camcraft_app::geometry::Mesh::from_facets(
            raw.facets
                .iter()
                .map(|t| {
                    camcraft_app::geometry::Triangle::new(
                        camcraft_app::geometry::transform_point(t.a, loc.pos, loc.rot),
                        camcraft_app::geometry::transform_point(t.b, loc.pos, loc.rot),
                        camcraft_app::geometry::transform_point(t.c, loc.pos, loc.rot),
                    )
                })
                .collect(),
        )
    };
    let world = world_with(Some(ImportModel::from_mesh("part", mesh)), -20.0, 50.0);
    let mut h = History::default();
    h.execute(&world, camcraft_app::commands::CreateSetup::new(&world, None, None).expect("Setup1"));
    if aligned {
        h.execute(
            &world,
            camcraft_app::commands::AlignPartToSetup::angles(&world, "Setup1", deg).expect("align"),
        );
    }
    let mid = camcraft_app::geometry::transform_point(Point3::new(0.0, 10.0, 10.0), loc.pos, loc.rot);
    (world, h, mid)
}

#[test]
fn extend_to_loop_reads_face_normals_in_the_setup_frame() {
    for deg in [
        [180.0, 0.0, 0.0],
        [0.0, 0.0, 90.0],
        [90.0, 0.0, 0.0],
        [55.0, 0.0, 0.0],
    ] {
        let (aligned_world, _ha, mid_a) = rotated_block_world(deg, true);
        let (baked_world, _hb, mid_b) = rotated_block_world(deg, false);
        let edge_a = edge_at_setup_midpoint(&aligned_world, Some("Setup1"), mid_a);
        let edge_b = edge_at_setup_midpoint(&baked_world, Some("Setup1"), mid_b);
        let got = operation::extend_to_loop(&aligned_world, std::slice::from_ref(&edge_a)).expect("aligned loop");
        let want = operation::extend_to_loop(&baked_world, std::slice::from_ref(&edge_b)).expect("baked loop");
        assert_eq!(
            loop_segments_in_setup(&aligned_world, Some("Setup1"), &got),
            loop_segments_in_setup(&baked_world, Some("Setup1"), &want),
            "extend_to_loop mismatch for setup alignment {deg:?}"
        );
    }

    // Active Setup2 alignment is honoured even when Setup1 is unaligned.
    let (world, mut h, _) = rotated_block_world([0.0, 0.0, 0.0], false);
    h.execute(&world, camcraft_app::commands::CreateSetup::new(&world, None, None).expect("Setup2"));
    h.execute(
        &world,
        camcraft_app::commands::AlignPartToSetup::angles(&world, "Setup2", [90.0, 0.0, 0.0]).expect("align Setup2"),
    );
    h.execute(
        &world,
        camcraft_app::commands::ActivateSetup::new(&world, Some("Setup2")).expect("activate Setup2"),
    );
    let (baked_world, _hb, mid_b) = rotated_block_world([90.0, 0.0, 0.0], false);
    let edge_s2 = edge_at_setup_midpoint(&world, Some("Setup2"), mid_b);
    let edge_b = edge_at_setup_midpoint(&baked_world, Some("Setup1"), mid_b);
    let got = operation::extend_to_loop(&world, std::slice::from_ref(&edge_s2)).expect("Setup2 loop");
    let want = operation::extend_to_loop(&baked_world, std::slice::from_ref(&edge_b)).expect("baked loop");
    assert_eq!(
        loop_segments_in_setup(&world, Some("Setup2"), &got),
        loop_segments_in_setup(&baked_world, Some("Setup1"), &want),
        "extend_to_loop should use active Setup2's alignment"
    );

    // Explicit setup reads Setup2's alignment even when Setup1 is active.
    h.execute(
        &world,
        camcraft_app::commands::ActivateSetup::new(&world, Some("Setup1")).expect("activate Setup1"),
    );
    let got_in = operation::extend_to_loop_in(&world, Some("Setup2"), std::slice::from_ref(&edge_s2))
        .expect("explicit Setup2 loop");
    assert_eq!(
        loop_segments_in_setup(&world, Some("Setup2"), &got_in),
        loop_segments_in_setup(&baked_world, Some("Setup1"), &want),
        "extend_to_loop_in should use the named Setup2 alignment even when Setup1 is active"
    );
}
