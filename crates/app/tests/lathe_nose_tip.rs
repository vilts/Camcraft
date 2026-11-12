//! #630: the lathe posts write the tool's CONTROL POINT for its
//! orientation, not the insert's nose centre. A Ø19.05 finish posts
//! X19.05, a shoulder at Z0 posts Z0, every orientation that fits
//! posts the cached nose path moved by r x its tip offset, an
//! orientation that cannot cut the operation is refused by name, the
//! orientation edit is one undoable command, and the cached toolpath
//! the verifier and the stock sim judge stays the nose centre.
//!
//! Gate map (M4.4): covers app::posting (op_streams, the nose module),
//! tools::orientation, app::commands (ChangeTool on a lathe tool).
//! Lane shape: seconds.

use flecs_ecs::prelude::World;

use camcraft_app::commands::{
    AddTool, ChangeTool, CreateOperation, GenerateOperation, History, ImportModel, LoadMachine, SetStock,
};
use camcraft_app::geometry::{Mesh, Point3, Triangle};
use camcraft_app::machine::{Coolant, Machine};
use camcraft_app::model;
use camcraft_app::operation::{self, Height, OpParams, OpStatus, SrcRef};
use camcraft_app::orientation;
use camcraft_app::post::{cnc, engine};
use camcraft_app::posting;
use camcraft_app::schema::Location;
use camcraft_app::tools::{catalog, Tool};
use camcraft_app::world::new_world;

const TAU: f64 = std::f64::consts::TAU;
const LATHE_CPS: &str = include_str!("../../../assets/posts/fanuc-lathe.cps");
const TN: &str = "TN-1600-0R8";
const R: f64 = 0.8;
/// The finished neck: Ø19.05 (3/4") above a Ø24 flange, shoulder at Z0.
const NECK: f64 = 19.05;
const BAR: [(f64, f64); 6] = [(0.0, -16.0), (12.0, -16.0), (12.0, 0.0), (NECK / 2.0, 0.0), (NECK / 2.0, 16.0), (0.0, 16.0)];

fn tool(name: &str) -> Tool {
    catalog().into_iter().find(|(n, _)| *n == name).map(|(_, t)| t).expect("catalog entry")
}

fn revolve(meridian: &[(f64, f64)], segs: usize) -> Mesh {
    let v = |j: usize, r: f64, z: f64| {
        let th = TAU * j as f64 / segs as f64;
        Point3::new(r * th.cos(), r * th.sin(), z)
    };
    let mut facets = Vec::new();
    for seg in meridian.windows(2) {
        let ((ra, za), (rb, zb)) = (seg[0], seg[1]);
        for j in 0..segs {
            let (a1, a2, b1, b2) = (v(j, ra, za), v(j + 1, ra, za), v(j, rb, zb), v(j + 1, rb, zb));
            if ra <= 1e-12 {
                facets.push(Triangle::new(a1, b2, b1));
            } else if rb <= 1e-12 {
                facets.push(Triangle::new(a1, a2, b2));
            } else {
                facets.push(Triangle::new(a1, a2, b2));
                facets.push(Triangle::new(a1, b2, b1));
            }
        }
    }
    Mesh::from_facets(facets)
}

fn bar_world() -> (World, History) {
    let world = new_world();
    let mut history = History::default();
    let at = Location { pos: Point3::new(0.0, 0.0, -16.0), ..Location::default() };
    let cmd = SetStock::new(&world, camcraft_app::stock::StockShape::Cylinder { diameter: 24.0, height: 32.0 }, at);
    history.execute(&world, cmd.expect("bar stock"));
    let lathe = Machine {
        model: "TestBench lathe".into(),
        kind: Some("lathe".into()),
        spindle_max: Some(12000.0),
        coolants: vec![Coolant { name: "Flood".into(), m_code: "M8".into() }],
        ..Default::default()
    };
    history.execute(&world, LoadMachine::new(&world, "lathe.mch".into(), Some(lathe)).expect("lathe"));
    history.execute(&world, AddTool::new(&world, TN, tool(TN)).expect("insert"));
    history.execute(&world, ImportModel::from_mesh("bar", revolve(&BAR, 32)));
    (world, history)
}

fn create(world: &World, history: &mut History, name: &str, params: OpParams) {
    let cmd = CreateOperation::new(world, name, TN, SrcRef::Entity("bar".into()), params).expect("create");
    history.execute(world, cmd);
    history.execute(world, GenerateOperation::new(world, name).expect("generate"));
    let e = model::find_by_name(world, name).expect("op");
    assert_eq!(operation::status_of(world, &e), OpStatus::Ok, "{name} generates");
}

fn finish() -> OpParams {
    OpParams::ProfileFinish {
        tolerance: 0.05,
        clearance_z: Height::fixed(25.0),
        top_z: Height::fixed(16.0),
        bottom_z: Height::fixed(-2.0),
        feed: 120.0,
        plunge_feed: 60.0,
    }
}

fn facing() -> OpParams {
    OpParams::Facing {
        tolerance: 0.05,
        clearance_z: Height::fixed(25.0),
        top_z: Height::fixed(16.0),
        bottom_z: Height::fixed(15.0),
        stepdown: 0.5,
        feed: 150.0,
        plunge_feed: 80.0,
    }
}

fn set_orientation(world: &World, history: &mut History, q: Option<u8>) {
    let e = model::find_by_name(world, TN).expect("tool");
    let mut t = model::tool_of(world, &e).expect("a tool");
    t.orientation = q;
    history.execute(world, ChangeTool::new(world, TN, t).expect("a sane orientation"));
}

/// The posted moves (stream order) of the one operation in the document.
fn posted_moves(world: &World) -> Vec<[f64; 3]> {
    posting::stream_records(world)
        .expect("posts")
        .into_iter()
        .filter_map(|r| match r {
            cnc::Record::Move(m) => Some([m.x, m.y, m.z]),
            _ => None,
        })
        .collect()
}

fn cached_moves(world: &World, op: &str) -> Vec<Point3> {
    let e = model::find_by_name(world, op).expect("op");
    model::toolpath_of(world, &e).expect("cache").path.moves.iter().map(|m| m.to).collect()
}

fn program(world: &World) -> String {
    let records = posting::stream_records(world).expect("posts");
    engine::run(LATHE_CPS, &posting::machine_value(world), &cnc::serialize(&records))
        .expect("the lathe script runs")
        .program
}

#[test]
fn an_od_finish_posts_the_finished_diameter_and_the_shoulder_posts_z0() {
    let (world, mut history) = bar_world();
    create(&world, &mut history, "S1", finish());
    let text = program(&world);
    assert!(text.contains("X19.05 "), "the Ø19.05 neck posts X19.05: {text}");
    assert!(!text.contains("X20.65"), "never the nose centre's 19.05 + 2r: {text}");
    // The shoulder face runs at Z0 into the neck's corner - never at
    // the nose centre's Z r.
    assert!(text.contains("X19.05 Z0 "), "the shoulder at Z0 posts Z0 into the neck: {text}");
    assert!(!text.contains("X20.65 Z0.8 "), "never the nose centre's corner: {text}");
    // The verifier's and the stock sim's truth is untouched: the cache
    // still rides the nose centre at 19.05/2 + r.
    let neck = cached_moves(&world, "S1").iter().map(|p| p.y).fold(f64::NEG_INFINITY, f64::max);
    assert!(neck >= NECK / 2.0 + R - 1e-6, "the cache is the nose centre: max radius {neck}");
}

#[test]
fn a_face_posts_its_plane_and_reaches_the_axis_with_the_nose() {
    let (world, mut history) = bar_world();
    create(&world, &mut history, "F1", facing());
    let text = program(&world);
    assert!(text.contains("X-1.6 Z15 "), "the last pass faces Z15 with the nose centre on the axis: {text}");
    assert!(!text.contains("Z15.8"), "never the nose centre's Z: {text}");
}

#[test]
fn every_fitting_orientation_posts_the_nose_path_moved_by_its_tip() {
    let (world, mut history) = bar_world();
    create(&world, &mut history, "S1", finish());
    let cl = cached_moves(&world, "S1");
    for q in orientation::fitting(&[orientation::Contact::MinusX]).into_iter().chain([orientation::CENTRE]) {
        set_orientation(&world, &mut history, Some(q));
        let e = model::find_by_name(&world, "S1").expect("op");
        assert_eq!(operation::status_of(&world, &e), OpStatus::Ok, "an orientation edit never stales the path");
        let (ox, oz) = orientation::tip_offset(q).expect("a chart code");
        let posted = posted_moves(&world);
        assert_eq!(posted.len(), cl.len(), "orientation {q}: one posted move per CL move");
        for (p, c) in posted.iter().zip(&cl) {
            assert!((p[0] - c.x).abs() < 1e-3, "orientation {q} left the meridian");
            assert!((p[1] - (c.y + R * ox)).abs() < 1e-3, "orientation {q}: radius {} for CL {}", p[1], c.y);
            assert!((p[2] - (c.z + R * oz)).abs() < 1e-3, "orientation {q}: Z {} for CL {}", p[2], c.z);
        }
        assert!(program(&world).contains(&format!("ORIENTATION {q})")), "the tool note names {q}");
    }
}

#[test]
fn an_orientation_that_cannot_cut_the_operation_is_refused_by_name() {
    let (world, mut history) = bar_world();
    create(&world, &mut history, "S1", finish());
    // A boring-bar orientation on an outside turn.
    set_orientation(&world, &mut history, Some(3));
    let e = posting::stream_records(&world).expect_err("an ID tip on an OD cut");
    assert!(e.contains("orientation [1, 2, 5, 6, 7]") && e.contains("is orientation 3"), "{e}");
    assert!(e.contains("S1"), "the refusal names the operation: {e}");
    // A tool cutting away from the chuck cannot face inward.
    let (world, mut history) = bar_world();
    create(&world, &mut history, "F1", facing());
    set_orientation(&world, &mut history, Some(1));
    let e = posting::stream_records(&world).expect_err("a +Z tip on a face");
    assert!(e.contains("orientation [2, 6, 7]") && e.contains("-Z and -X"), "{e}");
}

#[test]
fn the_orientation_edit_is_one_undoable_command() {
    let (world, mut history) = bar_world();
    let e = model::find_by_name(&world, TN).expect("tool");
    assert_eq!(model::tool_of(&world, &e).and_then(|t| t.lathe_orientation()), Some(2), "the insert's default");
    set_orientation(&world, &mut history, Some(6));
    assert_eq!(model::tool_of(&world, &e).and_then(|t| t.orientation), Some(6));
    assert!(history.undo(&world));
    assert_eq!(model::tool_of(&world, &e).and_then(|t| t.orientation), None, "undo restores the unstated default");
    assert!(history.redo(&world));
    assert_eq!(model::tool_of(&world, &e).and_then(|t| t.orientation), Some(6));
    // An off-chart code never enters the history.
    let mut t = model::tool_of(&world, &e).expect("a tool");
    t.orientation = Some(10);
    assert!(ChangeTool::new(&world, TN, t).is_err(), "orientation 10 is refused at the command");
}

#[test]
fn a_boring_bar_defaults_to_the_id_tip_and_refuses_an_od_one() {
    let bore = OpParams::BoringTurning {
        tolerance: 0.05,
        clearance_z: Height::fixed(25.0),
        top_z: Height::fixed(16.0),
        bottom_z: Height::fixed(0.0),
        feed: 100.0,
        plunge_feed: 50.0,
    };
    let mut bar = tool("BR-1000");
    assert_eq!(posting::nose::checked_orientation(&bore, "BR-1000", &bar), Ok(Some(3)), "a bar cuts toward the chuck on +X");
    bar.orientation = Some(2);
    let e = posting::nose::checked_orientation(&bore, "BR-1000", &bar).expect_err("an OD tip in a bore");
    assert!(e.contains("orientation [3, 4, 5, 7, 8]") && e.contains("+X"), "{e}");
    // A drill or a mill posts as it always did: no nose centre, no
    // orientation (a blade posts its corner - #641, lathe_blade_corner).
    assert_eq!(posting::nose::checked_orientation(&facing(), "EM", &tool("EM-0400-2FL")), Ok(None));
}
