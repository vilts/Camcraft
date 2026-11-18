//! A picked Slot's selection shapes and seeds (#1504 review): every pick
//! that bounds no slot (one edge, three edges, a face with edges, a
//! vertex or wall patch, a model that is gone) is refused by name, a
//! Slot created straight on picks takes the slot that fills them and
//! one undo removes it, and a tessellated round-ended (stadium) floor
//! face seeds the slot inscribed in its chords.
//!
//! Gate map (#1504): covers app::operation (slot_pick, generate),
//! app::commands (CreateOperation, SetOperationGeometry),
//! motion::machining2d (slot_room).

mod pocket_support;

use camcraft_app::commands::{AddTool, CreateOperation, History, ImportModel, SetOperationGeometry, SetStock};
use camcraft_app::geo2d::Point2;
use camcraft_app::geometry::{Mesh, Point3, feature};
use camcraft_app::links::Links;
use camcraft_app::machining3d::Entry3D;
use camcraft_app::model;
use camcraft_app::operation::{self, FeatureSel, GeoSel, Height, OpParams, SrcRef, slot_bound_of};
use camcraft_app::schema::Location;
use camcraft_app::stock::StockShape;
use camcraft_app::tools::catalog;
use camcraft_app::world::new_world;
use flecs_ecs::prelude::World;
use pocket_support::{block_with_pocket, rounded_rect};

const TOOL: &str = "EM-1000-4FL";
const EPS: f64 = 1e-6;

fn job(mesh: Mesh) -> (World, History) {
    let world = new_world();
    let mut h = History::default();
    h.execute(&world, ImportModel::from_mesh("part", mesh));
    let loc = Location { pos: Point3::new(0.0, 0.0, 0.0), ..Default::default() };
    h.execute(&world, SetStock::new(&world, StockShape::Box { dx: 50.0, dy: 50.0, dz: 20.0 }, loc).expect("stock"));
    let tool = catalog().into_iter().find(|(n, _)| *n == TOOL).map(|(_, t)| t).expect("catalog");
    h.execute(&world, AddTool::new(&world, TOOL, tool).expect("tool"));
    (world, h)
}

/// A 50 x 50 x [0, 20] block with a 30 x 16 square-cornered pocket to z 10.
fn square_pocket() -> (World, History) {
    job(block_with_pocket(25.0, 25.0, &rounded_rect(15.0, 8.0, 0.0, 1), 10.0, 20.0))
}

fn slot(width: f64) -> OpParams {
    OpParams::Slot {
        links: Links::PLANAR,
        a: Point2::new(-20.0, 0.0),
        b: Point2::new(20.0, 0.0),
        width,
        stock_to_leave: 0.0,
        clearance_z: Height::fixed(40.0),
        retract_z: None,
        top_z: Height::fixed(20.0),
        bottom_z: Height::fixed(10.0),
        stepdown: 5.0,
        stepover: 3.0,
        feed: 1200.0,
        plunge_feed: 300.0,
        climb: true,
        entry: Entry3D::Plunge,
        helix_radius: 1.0,
        tilt: None,
        extend_start: 0.0,
        extend_end: 0.0,
    }
}

/// The open floor chains of the pocket (z 10), the two long ones first.
fn floor_edges(world: &World) -> Vec<GeoSel> {
    let e = model::find_by_name(world, "part").expect("model");
    let f = model::features_of(world, &e).expect("features");
    let mut chains: Vec<(f64, GeoSel)> = feature::chains(&f)
        .into_iter()
        .filter_map(|c| {
            let p = feature::chain_points(&f, &c);
            let on_floor = !c.closed && p.len() >= 2 && p.iter().all(|q| (q[2] - 10.0).abs() < EPS);
            let y0 = p.first().map(|q| q[1]).unwrap_or(0.0);
            let long = (y0.abs() - 8.0).abs() < EPS && p.iter().all(|q| (q[1] - y0).abs() < EPS);
            on_floor.then(|| (if long { 0.0 } else { 1.0 }, GeoSel { entity: "part".into(), feature: FeatureSel::Chain(c.id) }))
        })
        .collect();
    chains.sort_by(|a, b| a.0.total_cmp(&b.0));
    assert!(chains.iter().filter(|c| c.0 == 0.0).count() == 2, "two long floor edges");
    chains.into_iter().map(|(_, g)| g).collect()
}

fn floor_face(world: &World) -> GeoSel {
    let e = model::find_by_name(world, "part").expect("model");
    let f = model::features_of(world, &e).expect("features");
    let face = f
        .faces
        .iter()
        .find(|fa| {
            fa.normal[2] > 0.99
                && fa.loops.iter().any(|l| l.verts.iter().filter_map(|&v| f.verts.get(v as usize)).all(|p| (p[2] - 10.0).abs() < EPS))
        })
        .expect("a floor face");
    GeoSel { entity: "part".into(), feature: FeatureSel::Face(face.id) }
}

fn refusal(world: &World, picks: &[GeoSel]) -> String {
    slot_bound_of(world, None, picks).expect_err("refused").to_string()
}

fn ends(world: &World, name: &str) -> (Point2, Point2, f64) {
    match model::find_by_name(world, name).and_then(|e| model::operation_of(world, &e)).map(|o| o.params) {
        Some(OpParams::Slot { a, b, width, .. }) => (a, b, width),
        other => panic!("no slot {name}: {other:?}"),
    }
}

#[test]
fn picks_that_bound_no_slot_are_refused_by_name() {
    let (world, _) = square_pocket();
    let edges = floor_edges(&world);
    assert!(edges.len() >= 3, "the long floor edges and an end edge: {edges:?}");
    slot_bound_of(&world, None, &edges[..2]).expect("the two long edges bound the slot");
    let e = refusal(&world, &edges[..1]);
    assert!(e.contains("this selection has 1 open edge and 0 faces or rings"), "{e}");
    let e = refusal(&world, &edges[..3]);
    assert!(e.contains("this selection has 3 open edges and 0 faces or rings"), "{e}");
    let mut face_and_edges = vec![floor_face(&world)];
    face_and_edges.extend_from_slice(&edges[..2]);
    let e = refusal(&world, &face_and_edges);
    assert!(e.contains("this selection has 2 open edges and 1 face or ring"), "{e}");
    for odd in [FeatureSel::Vertex(0), FeatureSel::Patch(0)] {
        let mut picks = edges[..2].to_vec();
        picks.push(GeoSel { entity: "part".into(), feature: odd });
        let e = refusal(&world, &picks);
        assert!(e.contains("is not an edge or a face - a Slot is picked by two opposite straight edges or one floor face"), "{e}");
    }
    let gone = vec![GeoSel { entity: "no-such-part".into(), feature: FeatureSel::Face(0) }];
    let e = refusal(&world, &gone);
    assert!(e.contains("the selection names 'no-such-part', which is gone"), "{e}");
}

#[test]
fn a_slot_created_on_picks_takes_the_slot_between_them_and_undo_removes_it() {
    let (world, mut h) = square_pocket();
    let edges = floor_edges(&world)[..2].to_vec();
    let cmd = CreateOperation::new(&world, "S1", TOOL, SrcRef::Selection(edges), slot(12.0)).expect("create");
    h.execute(&world, cmd);
    let (a, b, w) = ends(&world, "S1");
    let (lo, hi) = if a.x < b.x { (a, b) } else { (b, a) };
    assert!((w - 16.0).abs() < EPS && (lo.x + 7.0).abs() < EPS && (hi.x - 7.0).abs() < EPS, "seeded at creation: {a:?} {b:?} {w}");
    assert!(lo.y.abs() < EPS && hi.y.abs() < EPS);
    assert!(h.undo(&world));
    assert!(model::find_by_name(&world, "S1").is_none(), "one undo removes the created, seeded slot");
    assert!(h.redo(&world));
    assert!((ends(&world, "S1").2 - 16.0).abs() < EPS, "redo restores it seeded");
}

#[test]
fn a_tessellated_stadium_floor_face_seeds_the_inscribed_slot() {
    // 30 x 16 stadium pocket, 8 chords per quarter: the chords sag
    // 0.039 mm inside the round ends. The slot is drawn in by the sag,
    // never refused for it, and never the tolerance loosened.
    let (world, mut h) = job(block_with_pocket(25.0, 25.0, &rounded_rect(15.0, 8.0, 8.0, 8), 10.0, 20.0));
    h.execute(&world, CreateOperation::new(&world, "S1", TOOL, SrcRef::Entity("part".into()), slot(12.0)).expect("create"));
    h.execute(&world, SetOperationGeometry::new(&world, "S1", vec![floor_face(&world)]).expect("pick"));
    let (a, b, w) = ends(&world, "S1");
    let sag = 8.0 * (1.0 - (std::f64::consts::PI / 32.0).cos());
    assert!((w - (16.0 - 2.0 * sag)).abs() < 1e-6, "inscribed in the chords: {w}");
    let (lo, hi) = if a.x < b.x { (a, b) } else { (b, a) };
    assert!((lo.x + 7.0).abs() < 1e-6 && (hi.x - 7.0).abs() < 1e-6 && lo.y.abs() < 1e-6, "{a:?} {b:?}");
    let op = model::find_by_name(&world, "S1").and_then(|e| model::operation_of(&world, &e)).expect("op");
    let (_, warns) = operation::generate(&world, &op, "S1").expect("the seeded slot generates");
    assert!(warns.iter().all(|w| !w.blocks_post()), "{warns:?}");
    assert!(operation::slot_fit_of(&world, &op).is_some_and(|r| r.is_ok()), "the page echoes the room");
}
