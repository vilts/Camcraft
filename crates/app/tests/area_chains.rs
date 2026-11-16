//! #1727: two separate edges as one open boundary. A face selection
//! bounded by two of its edges is faced between them - never across
//! either - and runs on past the face at the open ends, with the
//! depth-aware verify silent; a Contour cannot ride two chains and an
//! include made of two chains is refused at Apply, each by name.
//! Provenance: our own tests, specced from the issue's "Done means".
//!
//! Gate map (#1727): covers app::operation (boundary_chains,
//! boundary_picks, boundary, contour_open), app::commands
//! (EditOperationParams::with_area).

mod area_support;

use area_support::*;

fn north_south(world: &World) -> Vec<GeoSel> {
    [
        rim_edge(world, |q| (q[1] - 20.0).abs() < 1e-9),
        rim_edge(world, |q| (q[1] + 20.0).abs() < 1e-9),
    ]
    .into_iter()
    .map(|f| GeoSel { entity: "part".into(), feature: f })
    .collect()
}

fn between(world: &World) -> AreaControls {
    AreaControls { open: Some(OpenBoundary { picks: north_south(world), side: ChainSide::Inside }), ..Default::default() }
}

#[test]
fn a_face_bounded_by_two_of_its_edges_is_faced_between_them() {
    let world = block_world();
    let top = top_face(&world, "part", 10.0);
    let shape = operation::boundary_pick_shape(&world, None, &north_south(&world), &face(10.0));
    assert_eq!(shape.summary(), "2 edges, 2 open chains");
    let (t, warns) = run(&world, &op(sel("part", top), face(10.0), between(&world))).expect("faced between the edges");
    assert_eq!(warns, 0, "the verify is silent: {:?}", t.notes);
    let f = feeds(&t);
    assert!(!f.is_empty(), "a path was cut");
    assert!(f.iter().all(|p| p.y + R <= 20.0 + EPS), "never across the north edge");
    assert!(f.iter().all(|p| p.y - R >= -20.0 - EPS), "never across the south edge");
    assert!((max_by(&f, |p| p.y) + R - 20.0).abs() < EPS, "works up to the north edge");
    assert!((max_by(&f, |p| -p.y) + R - 20.0).abs() < EPS, "works down to the south edge");
    // The ends are open: the rows run on past the face both ways.
    assert!(max_by(&f, |p| p.x) > 20.0 + R && max_by(&f, |p| -p.x) > 20.0 + R, "the open ends run on");
}

#[test]
fn a_contour_cannot_ride_two_separate_chains() {
    let world = block_world();
    let top = top_face(&world, "part", 10.0);
    let a = AreaControls { open: Some(OpenBoundary { picks: north_south(&world), side: ChainSide::Outside }), ..Default::default() };
    let err = run(&world, &op(sel("part", top), contour(10.0, 5.0, Side::Auto), a)).expect_err("refused");
    assert!(err.contains("a Contour rides one open chain") && err.contains("2 separate chains"), "{err}");
}

#[test]
fn an_include_of_two_separate_chains_is_refused_at_apply() {
    let world = block_world();
    let mut h = History::default();
    h.execute(&world, CreateOperation::new(&world, "F", TOOL, SrcRef::Stock, face(10.0)).expect("create"));
    let e = model::find_by_name(&world, "F").expect("op");
    let params = model::operation_of(&world, &e).expect("op").params;
    let bad = AreaControls {
        extra: vec![ExtraBoundary { mode: BoundaryMode::Include, shape: BoundaryShape::Picks(north_south(&world)) }],
        ..Default::default()
    };
    let err = EditOperationParams::with_area(&world, "F", params.clone(), bad).err().expect("refused at Apply");
    assert!(err.to_string().contains("boundary 1 (Include) is an open chain (2 edges, 2 open chains)"), "{err}");
    // The same picks as the open boundary ride one undoable edit.
    h.execute(&world, EditOperationParams::with_area(&world, "F", params, between(&world)).expect("open is fine"));
    assert_eq!(model::operation_of(&world, &e).expect("op").area, between(&world));
    assert!(h.undo(&world));
    assert_eq!(model::operation_of(&world, &e).expect("op").area, AreaControls::default());
}

#[test]
fn chains_that_cross_are_refused_by_name() {
    // A second block whose west rim edge (x = -5, y 15..25) crosses the
    // first block's north rim edge (y = 20) at (-5, 20).
    let world = block_world();
    let mut h = History::default();
    h.execute(&world, ImportModel::from_mesh("other", cube(Point3::new(-5.0, 15.0, 0.0), Point3::new(5.0, 25.0, 10.0))));
    let e = model::find_by_name(&world, "other").expect("model");
    let f = model::features_of(&world, &e).expect("features");
    let west = feature::chains(&f)
        .into_iter()
        .find(|c| {
            let p = feature::chain_points(&f, c);
            !c.closed && p.len() >= 2 && p.iter().all(|q| (q[2] - 10.0).abs() < 1e-9 && (q[0] + 5.0).abs() < 1e-9)
        })
        .map(|c| FeatureSel::Chain(c.id))
        .expect("the other block's west rim edge");
    let picks = vec![
        GeoSel { entity: "part".into(), feature: rim_edge(&world, |q| (q[1] - 20.0).abs() < 1e-9) },
        GeoSel { entity: "other".into(), feature: west },
    ];
    let shape = operation::boundary_pick_shape(&world, None, &picks, &face(10.0));
    let operation::PickShape::Invalid(why) = shape else { panic!("crossing chains are refused: {shape:?}") };
    assert!(why.contains("chains 1 and 2 touch or cross"), "{why}");
}
