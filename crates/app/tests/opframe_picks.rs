//! #763 Phase 2: picked features (faces, closed chains, stitched open runs)
//! read through OpFrame in an aligned setup for the four verified kinds
//! (Face, 2D Contour, 2D Pocket, 2D Adaptive), while Tilt x picks and
//! unverified kinds stay refused by name (area controls pass, #2210).
//!
//! Provenance: our own fixtures over plain re-clamp and two-sided step
//! machining practice; no vendor material consulted.
//!
//! Gate map (#763): covers app::operation::frame, app::operation::selection,
//! app::operation::stitch, app::operation::picks, app::operation::stamp,
//! app::operation::heights_sel, app::operation::area, app::setup_align.
//! Lane shape (M4.4): all tests here run in seconds.

use flecs_ecs::prelude::World;

use camcraft_app::commands::{
    AddTool, AlignPartToSetup, CreateOperation, CreateSetup, GenerateOperation, History,
    ImportModel, SetOperationGeometry, SetStock,
};
use camcraft_app::geometry::{Mesh, Point3, Triangle, cube, feature};
use camcraft_app::machining2d::{AdaptiveMethod, Lead, PocketStrategy, Side};
use camcraft_app::machining3d::Entry3D;
use camcraft_app::operation::{
    self, AreaControls, Carried, FeatureSel, GeoSel, Height, HeightReference, KindInfo, OpParams,
    OpStatus, OpType, SrcRef, Tilt, generate, region_of,
};
use camcraft_app::schema::Location;
use camcraft_app::setup_align::AlignPreset;
use camcraft_app::setup_turn::TurnPivot;
use camcraft_app::stock::StockShape;
use camcraft_app::tools::catalog;
use camcraft_app::world::new_world;
use camcraft_app::{model, posting};

const TIGHT: f64 = 1e-6;
const TOOL: &str = "EM-0400-2FL";

fn flip_tri(t: &Triangle) -> Triangle {
    Triangle::new(t.a, t.c, t.b)
}

/// 40 x 40 x [0, 20] block with a 20 x 20 square blind pocket opening
/// DOWNWARD in document space (solid Z in [10, 20], floor at Z = 10,
/// normal -Z). Flipped over X, the pocket opens upward at Z in [10, 20].
fn downward_pocket_block() -> Mesh {
    let quad = |a: Point3, b: Point3, c: Point3, d: Point3, f: &mut Vec<Triangle>| {
        f.push(Triangle::new(a, b, c));
        f.push(Triangle::new(a, c, d));
    };
    let mut f = Vec::new();
    let outer = cube(Point3::new(-20.0, -20.0, 0.0), Point3::new(20.0, 20.0, 20.0));
    f.extend(outer.facets[4..12].iter().cloned());
    f.extend(outer.facets[2..4].iter().cloned());
    let o = [(-20.0, -20.0), (-20.0, 20.0), (20.0, 20.0), (20.0, -20.0)];
    let i = [(-10.0, -10.0), (-10.0, 10.0), (10.0, 10.0), (10.0, -10.0)];
    for k in 0..4 {
        let n = (k + 1) % 4;
        let p = |(x, y): (f64, f64)| Point3::new(x, y, 0.0);
        quad(p(o[k]), p(o[n]), p(i[n]), p(i[k]), &mut f);
    }
    let inner = cube(Point3::new(-10.0, -10.0, 0.0), Point3::new(10.0, 10.0, 10.0));
    f.extend(inner.facets[4..12].iter().map(flip_tri));
    f.extend(inner.facets[2..4].iter().map(flip_tri));
    Mesh::from_facets(f)
}

fn setup_world(name: &str, mesh: Mesh, stock_dx: f64, stock_dy: f64, stock_dz: f64, stock_z: f64) -> (World, History) {
    let world = new_world();
    let mut h = History::default();
    h.execute(&world, ImportModel::from_mesh(name, mesh));
    h.execute(&world, CreateSetup::new(&world, None, None).expect("Setup1"));
    let at = Location { pos: Point3::new(0.0, 0.0, stock_z), ..Default::default() };
    h.execute(
        &world,
        SetStock::new(&world, StockShape::Box { dx: stock_dx, dy: stock_dy, dz: stock_dz }, at).expect("stock"),
    );
    let tool = catalog().into_iter().find(|(n, _)| *n == TOOL).map(|(_, t)| t).expect("tool");
    h.execute(&world, AddTool::new(&world, TOOL, tool).expect("add tool"));
    (world, h)
}

fn op_of(world: &World, name: &str) -> camcraft_app::schema::Operation {
    model::operation_of(world, &model::find_by_name(world, name).expect("op exists")).expect("operation")
}

fn find_face(
    f: &feature::Features,
    want_n: [f64; 3],
    want_off: f64,
) -> &feature::Face {
    f.faces
        .iter()
        .find(|fa| {
            let d = fa.normal[0] * want_n[0] + fa.normal[1] * want_n[1] + fa.normal[2] * want_n[2];
            d.abs() > 1.0 - 1e-6 && (fa.offset * d - want_off).abs() < 1e-6
        })
        .expect("matching face")
}

fn face_by_normal_and_offset(world: &World, entity: &str, want_n: [f64; 3], want_off: f64) -> GeoSel {
    let e = model::find_by_name(world, entity).expect("entity");
    let f = model::features_of(world, &e).expect("features");
    GeoSel { entity: entity.into(), feature: FeatureSel::Face(find_face(&f, want_n, want_off).id) }
}

fn face_edge_chains(world: &World, entity: &str, want_n: [f64; 3], want_off: f64) -> Vec<GeoSel> {
    let e = model::find_by_name(world, entity).expect("entity");
    let f = model::features_of(world, &e).expect("features");
    let face = find_face(&f, want_n, want_off);
    let loop_verts: Vec<[f64; 3]> =
        face.loops[0].verts.iter().filter_map(|&vi| f.verts.get(vi as usize).copied()).collect();
    let on_face = |pt: [f64; 3]| {
        loop_verts
            .iter()
            .any(|v| (v[0] - pt[0]).abs() < 1e-6 && (v[1] - pt[1]).abs() < 1e-6 && (v[2] - pt[2]).abs() < 1e-6)
    };
    feature::chains(&f)
        .into_iter()
        .filter(|c| {
            let pts = feature::chain_points(&f, c);
            pts.len() >= 2 && pts.iter().all(|p| on_face(*p))
        })
        .map(|c| GeoSel { entity: entity.into(), feature: FeatureSel::Chain(c.id) })
        .collect()
}

fn face_params(cut: Height, clearance: f64) -> OpParams {
    OpParams::Face {
        holes: Default::default(),
        blend_turns: false,
        tilt: None,
        stepover: 3.0,
        clearance_z: Height::fixed(clearance),
        cut_z: cut,
        top_z: None,
        stepdown: None,
        feed: 1200.0,
        plunge_feed: 300.0,
        climb: true,
    }
}

fn pocket_params(top: Height, bottom: Height, stepdown: f64) -> OpParams {
    OpParams::Pocket { rest: None,
        blend_radius: None,
        links: camcraft_app::links::Links::PLANAR,
        tilt: None,
        stepover: 2.5,
        stock_to_leave: 0.0,
        clearance_z: Height::fixed(40.0),
        retract_z: Some(Height::fixed(30.0)),
        top_z: top,
        bottom_z: bottom,
        stepdown,
        feed: 1000.0,
        plunge_feed: 300.0,
        climb: true,
        strategy: PocketStrategy::Zigzag,
        entry: Entry3D::Plunge,
        helix_radius: 1.0,
        optimal_load: camcraft_app::load::DEFAULT_OPTIMAL_LOAD,
        blend_turns: false,
    }
}

fn adaptive_params(top: Height, bottom: Height, stepdown: f64) -> OpParams {
    OpParams::Adaptive2D {
        blend_radius: None,
        blend_turns: false,
        links: camcraft_app::links::Links::PLANAR,
        tilt: None,
        stepover: 3.5,
        stock_to_leave: 0.0,
        clearance_z: Height::fixed(40.0),
        retract_z: Some(Height::fixed(30.0)),
        top_z: top,
        bottom_z: bottom,
        stepdown,
        feed: 1000.0,
        plunge_feed: 300.0,
        climb: true,
        entry: Entry3D::Plunge,
        helix_radius: 1.0,
        optimal_load: camcraft_app::load::DEFAULT_OPTIMAL_LOAD,
        method: AdaptiveMethod::Spiral,
        troch: Default::default(),
    }
}

fn contour_params(top: Height, bottom: Height, side: Side) -> OpParams {
    OpParams::Contour {
        stock_to_leave: 0.0,
        clearance_z: Height::fixed(40.0),
        retract_z: Height::fixed(30.0),
        top_z: top,
        bottom_z: bottom,
        stepdown: 5.0,
        feed: 1000.0,
        plunge_feed: 300.0,
        climb: true,
        side,
        lead: Lead::None,
        tolerance: 0.01,
        tab_count: 0,
        tab_width: 5.0,
        tab_height: 1.0,
        entry: Entry3D::Plunge,
        helix_radius: 1.0,
        blend_turns: false,
        tilt: None,
        links: camcraft_app::links::Links::PLANAR,
    }
}

#[test]
fn aligned_face_pick_machines_the_turned_side_and_refuses_the_vertical_top() {
    // 20 x 20 x 10 block: X=[0,20], Y=[0,20], Z=[0,10].
    let (world, mut h) = setup_world("block", cube(Point3::ZERO, Point3::new(20.0, 20.0, 10.0)), 40.0, 40.0, 30.0, -5.0);
    let doc_front = face_by_normal_and_offset(&world, "block", [0.0, -1.0, 0.0], 0.0);
    let doc_top = face_by_normal_and_offset(&world, "block", [0.0, 0.0, 1.0], 10.0);

    // Turn -90 deg about X around part centre (10, 10, 5):
    // doc_front becomes horizontal TOP (z = 15, normal +Z), X=[0,20], Y=[5,15].
    // doc_top becomes vertical (normal +Y).
    h.execute(
        &world,
        AlignPartToSetup::turn(&world, "Setup1", [-90.0, 0.0, 0.0], TurnPivot::PartCentre).expect("turn -90 X"),
    );

    let fp = face_params(Height::of(HeightReference::TopOfSelection, -1.0), 30.0);
    let (_, top_refusal) = operation::selection_account(&world, Some("Setup1"), std::slice::from_ref(&doc_top), &fp);
    assert!(
        top_refusal.as_ref().is_some_and(|m| m.contains("not horizontal")),
        "turned-vertical doc top must refuse: {top_refusal:?}"
    );
    let (_, front_refusal) = operation::selection_account(&world, Some("Setup1"), std::slice::from_ref(&doc_front), &fp);
    assert!(front_refusal.is_none(), "turned-horizontal front wall must pass: {front_refusal:?}");

    h.execute(
        &world,
        CreateOperation::new(&world, "FaceFront", TOOL, SrcRef::Selection(vec![doc_front]), fp)
            .expect("create face on turned front wall"),
    );
    let Err(err_set) = SetOperationGeometry::new(&world, "FaceFront", vec![doc_top]) else {
        panic!("vertical face must be refused at SetOperationGeometry");
    };
    assert!(err_set.to_string().contains("not horizontal"), "{err_set}");

    h.execute(&world, GenerateOperation::new(&world, "FaceFront").expect("generate aligned face pick"));
    let op = op_of(&world, "FaceFront");
    let reg = region_of(&world, &op).expect("aligned preview region");
    let (rlo, rhi) = reg.bounds().expect("region bounds");
    assert!((rlo.x - 0.0).abs() < TIGHT && (rhi.x - 20.0).abs() < TIGHT, "X bounds: {rlo:?}..{rhi:?}");
    assert!((rlo.y - 5.0).abs() < TIGHT && (rhi.y - 15.0).abs() < TIGHT, "turned Y bounds [5,15]: {rlo:?}..{rhi:?}");

    let (path, _) = generate(&world, &op, "FaceFront").expect("aligned face pick generates");
    let feed_pts: Vec<_> = path.moves.iter().filter(|m| m.feed.is_some()).collect();
    assert!(!feed_pts.is_empty());
    for m in &feed_pts {
        assert!((m.to.z - 14.0).abs() < TIGHT, "cut plane at turned top 15 - 1 = 14: {:?}", m.to);
        assert!(m.to.y >= 5.0 - 2.1 && m.to.y <= 15.0 + 2.1, "Y inside turned [5,15]+r: {:?}", m.to);
    }
}

#[test]
fn aligned_flipped_pocket_adaptive_and_contour_picks_cut_the_flipped_pocket() {
    let (world, mut h) = setup_world("part", downward_pocket_block(), 60.0, 60.0, 24.0, -2.0);
    let floor = face_by_normal_and_offset(&world, "part", [0.0, 0.0, -1.0], -10.0);

    h.execute(&world, AlignPartToSetup::preset(&world, "Setup1", AlignPreset::FlipX).expect("flip over X"));

    let sel = SrcRef::Selection(vec![floor]);
    let pocket = pocket_params(Height::of(HeightReference::TopOfModel, 0.0), Height::of(HeightReference::BottomOfSelection, 0.0), 10.0);
    let adaptive = adaptive_params(Height::of(HeightReference::TopOfModel, 0.0), Height::of(HeightReference::BottomOfSelection, 0.0), 10.0);
    let contour = contour_params(Height::of(HeightReference::TopOfModel, 0.0), Height::of(HeightReference::BottomOfSelection, 0.0), Side::Inside);

    h.execute(&world, CreateOperation::new(&world, "PocketPick", TOOL, sel.clone(), pocket).expect("pocket"));
    h.execute(&world, CreateOperation::new(&world, "AdaptPick", TOOL, sel.clone(), adaptive).expect("adaptive"));
    h.execute(&world, CreateOperation::new(&world, "ContourPick", TOOL, sel, contour).expect("contour"));

    for name in ["PocketPick", "AdaptPick", "ContourPick"] {
        h.execute(&world, GenerateOperation::new(&world, name).unwrap_or_else(|e| panic!("{name} generates: {e}")));
        let path = model::toolpath_of(&world, &model::find_by_name(&world, name).expect("op")).expect("path");
        let cut_moves: Vec<_> = path.path.moves.iter().filter(|m| m.feed.is_some() && m.to.z < 19.9).collect();
        assert!(!cut_moves.is_empty(), "{name} cuts inside the flipped pocket");
        for m in &cut_moves {
            assert!(m.to.z >= 10.0 - TIGHT, "{name} does not cut below floor Z=10: {:?}", m.to);
            assert!(
                m.to.x.abs() <= 10.0 - 2.0 + 0.02 && m.to.y.abs() <= 10.0 - 2.0 + 0.02,
                "{name} stays inside pocket [-10,10]^2 by tool radius: {:?}",
                m.to
            );
        }
    }
    let out_path = std::env::temp_dir().join(format!("camcraft-opframe-picks-{}.nc", std::process::id()));
    let posted = posting::post_setup(&world, &out_path);
    let _ = std::fs::remove_file(&out_path);
    let nc = posted.expect("flipped picked program posts").program;
    assert!(nc.contains("G54"), "{nc}");
}

#[test]
fn aligned_stitched_open_runs_and_stamp_staleness() {
    let (world, mut h) = setup_world("block", cube(Point3::ZERO, Point3::new(20.0, 20.0, 10.0)), 40.0, 40.0, 30.0, -5.0);
    let edges = face_edge_chains(&world, "block", [0.0, -1.0, 0.0], 0.0);
    assert_eq!(edges.len(), 4, "four open edges around the front face");

    h.execute(
        &world,
        AlignPartToSetup::turn(&world, "Setup1", [-90.0, 0.0, 0.0], TurnPivot::PartCentre).expect("turn -90 X"),
    );

    assert_eq!(operation::stitched_rings(&world, &edges), vec![Some(0); 4], "aligned open runs stitch into ring 0");
    let contour = contour_params(
        Height::of(HeightReference::TopOfSelection, 0.0),
        Height::of(HeightReference::BottomOfSelection, -2.0),
        Side::Outside,
    );
    let notes = operation::selection_notes(&world, Some("Setup1"), &edges, &contour);
    assert!(notes.iter().any(|n| n.contains("4 open runs stitched into 1 ring")), "{notes:?}");

    h.execute(
        &world,
        CreateOperation::new(&world, "StitchedContour", TOOL, SrcRef::Selection(edges.clone()), contour).expect("create contour"),
    );
    h.execute(&world, GenerateOperation::new(&world, "StitchedContour").expect("generate"));
    let op_e = model::find_by_name(&world, "StitchedContour").expect("op");
    assert_eq!(operation::status_of(&world, &op_e), OpStatus::Ok);
    let path = model::toolpath_of(&world, &op_e).expect("path");
    let cut_ys: Vec<f64> = path.path.moves.iter().filter(|m| m.feed.is_some() && m.to.z < 14.9).map(|m| m.to.y).collect();
    assert!(!cut_ys.is_empty());
    assert!(cut_ys.iter().all(|&y| (2.9..=17.1).contains(&y)), "cut Y in aligned frame: {cut_ys:?}");

    h.execute(&world, AlignPartToSetup::preset(&world, "Setup1", AlignPreset::TurnZ90).expect("re-align"));
    assert_eq!(operation::status_of(&world, &op_e), OpStatus::Stale);
    assert!(h.undo(&world));
    assert_eq!(operation::status_of(&world, &op_e), OpStatus::Ok);

    // Fixed-height picked op must still go Stale under a pure in-plane Z rotation (stamp reads pose_in).
    let top_face = face_by_normal_and_offset(&world, "block", [0.0, 0.0, 1.0], 10.0);
    h.execute(&world, AlignPartToSetup::preset(&world, "Setup1", AlignPreset::TurnZ90).expect("turn 90 Z"));
    h.execute(
        &world,
        CreateOperation::new(&world, "FixedFace", TOOL, SrcRef::Selection(vec![top_face]), face_params(Height::fixed(9.0), 25.0))
            .expect("create fixed face"),
    );
    h.execute(&world, GenerateOperation::new(&world, "FixedFace").expect("generate fixed face"));
    let fixed_e = model::find_by_name(&world, "FixedFace").expect("fixed op");
    assert_eq!(operation::status_of(&world, &fixed_e), OpStatus::Ok);
    h.execute(&world, AlignPartToSetup::preset(&world, "Setup1", AlignPreset::TurnZ270).expect("turn 270 Z"));
    assert_eq!(operation::status_of(&world, &fixed_e), OpStatus::Stale);

    // Multi-setup: Setup1 is AsModelled while active Setup2 is turned -90 X; stitched_rings follows active Setup2.
    h.execute(&world, AlignPartToSetup::preset(&world, "Setup1", AlignPreset::AsModelled).expect("reset Setup1"));
    h.execute(&world, CreateSetup::new(&world, None, Some("Setup1")).expect("Setup2"));
    h.execute(
        &world,
        AlignPartToSetup::turn(&world, "Setup2", [-90.0, 0.0, 0.0], TurnPivot::PartCentre).expect("turn Setup2 -90 X"),
    );
    assert_eq!(operation::stitched_rings(&world, &edges), vec![Some(0); 4], "stitched_rings reads active Setup2");
}

#[test]
fn tilt_times_picks_and_unverified_stay_refused_and_area_passes_in_phase_two() {
    let (world, mut h) = setup_world("block", cube(Point3::ZERO, Point3::new(20.0, 20.0, 10.0)), 40.0, 40.0, 30.0, -5.0);
    let top = face_by_normal_and_offset(&world, "block", [0.0, 0.0, 1.0], 10.0);
    let sel = SrcRef::Selection(vec![top]);

    let mut tilted_face = face_params(Height::fixed(9.0), 25.0);
    if let OpParams::Face { tilt, .. } = &mut tilted_face {
        *tilt = Some(Tilt { basis_version: 1, a_deg: 30.0, b_deg: 0.0, origin: [0.0; 3] });
    }
    h.execute(&world, CreateOperation::new(&world, "TiltedPick", TOOL, sel.clone(), tilted_face).expect("create"));
    let err = generate(&world, &op_of(&world, "TiltedPick"), "TiltedPick").expect_err("tilt x picks refuses");
    assert!(err.to_string().contains("tilted plane cannot yet index a picked selection"), "{err}");

    h.execute(&world, AlignPartToSetup::preset(&world, "Setup1", AlignPreset::FlipX).expect("flip"));
    let err = generate(&world, &op_of(&world, "TiltedPick"), "TiltedPick").expect_err("aligned tilt x picks refuses");
    assert!(err.to_string().contains("tilted plane is not supported yet in an aligned setup"), "{err}");

    // #2276 / #2283: 2D Offset, Circular and 2D Chamfer are verified
    // now; Deburr still waits for proof (the op keeps its old name).
    let circular = (KindInfo::ALL[OpType::Deburr.ordinal()].seed)(&Carried::of(&face_params(Height::fixed(9.0), 25.0)));
    let cm = catalog().into_iter().find(|(n, _)| *n == "CM-0600-90").map(|(_, t)| t).expect("chamfer mill");
    h.execute(&world, AddTool::new(&world, "CM-0600-90", cm).expect("add chamfer mill"));
    h.execute(&world, CreateOperation::new(&world, "CircularPick", "CM-0600-90", sel, circular).expect("create the Deburr"));
    let err = generate(&world, &op_of(&world, "CircularPick"), "CircularPick").expect_err("unverified kind refuses");
    assert!(err.to_string().contains("not supported yet in an aligned setup"), "{err}");

    let mut area_op = op_of(&world, "TiltedPick");
    area_op.params = face_params(Height::fixed(5.0), 25.0);
    area_op.area = AreaControls { offset: 1.0, ..Default::default() };
    // #2210: the 2D kinds' area controls resolve in the setup frame now;
    // the aligned door passes them (`setup_align_2d_area` pins the cut).
    assert_eq!(camcraft_app::setup_align::generation_problem(&world, &area_op), None);
    if let Err(err) = generate(&world, &area_op, "AreaPick") {
        assert!(!err.to_string().contains("aligned setup"), "{err}");
    }
}
