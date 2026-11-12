//! #641: a grooving/parting blade is touched off on a CORNER of its tip
//! edge (LinuxCNC Lathe User Information 4.2), and the lathe posts
//! write that corner - the blade kernels' CL (the middle of the tip
//! edge) moved half a width along Z. A 3 mm groove centred at Z-10
//! posts Z-11.5 on the default left corner, Z-8.5 on the right one, and
//! Z-10 only on a blade touched off at its width centre; a part-off
//! leaves the part the verified kerf leaves; a code that is no OD blade
//! is refused by name; the corner edit is one undoable command; the
//! cached toolpath the verifier and the stock sim judge stays the
//! middle of the edge.
//!
//! Gate map (M4.4): covers app::posting (op_streams, the nose module's
//! blade branch), tools::orientation (blade corners), app::commands
//! (ChangeTool on a blade).
//! Lane shape: seconds.

use flecs_ecs::prelude::World;

use camcraft_app::commands::{
    AddTool, ChangeTool, CreateOperation, GenerateOperation, History, ImportModel, LoadMachine, SetStock,
};
use camcraft_app::cl::MoveKind;
use camcraft_app::geometry::{Mesh, Point3, Triangle};
use camcraft_app::machine::{Coolant, Machine};
use camcraft_app::model;
use camcraft_app::operation::{self, Height, OpParams, OpStatus, SrcRef};
use camcraft_app::post::{cnc, engine};
use camcraft_app::posting;
use camcraft_app::schema::Location;
use camcraft_app::tools::catalog;
use camcraft_app::world::new_world;

const TAU: f64 = std::f64::consts::TAU;
const LATHE_CPS: &str = include_str!("../../../assets/posts/fanuc-lathe.cps");
/// A 3 mm grooving/parting blade.
const GT: &str = "GT-0300";
const W: f64 = 3.0;
/// The bar: Ø12, Z-40..0, a 3 mm groove to r4 centred at Z-10.
const BAR: [(f64, f64); 8] =
    [(0.0, -40.0), (6.0, -40.0), (6.0, -11.5), (4.0, -11.5), (4.0, -8.5), (6.0, -8.5), (6.0, 0.0), (0.0, 0.0)];

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
    let at = Location { pos: Point3::new(0.0, 0.0, -40.0), ..Location::default() };
    let cmd = SetStock::new(&world, camcraft_app::stock::StockShape::Cylinder { diameter: 12.0, height: 40.0 }, at);
    history.execute(&world, cmd.expect("bar stock"));
    let lathe = Machine {
        model: "TestBench lathe".into(),
        kind: Some("lathe".into()),
        spindle_max: Some(12000.0),
        coolants: vec![Coolant { name: "Flood".into(), m_code: "M8".into() }],
        ..Default::default()
    };
    history.execute(&world, LoadMachine::new(&world, "lathe.mch".into(), Some(lathe)).expect("lathe"));
    let blade = catalog().into_iter().find(|(n, _)| *n == GT).map(|(_, t)| t).expect("catalog blade");
    assert!((blade.width - W).abs() < 1e-12, "the fixture's blade is 3 mm wide");
    history.execute(&world, AddTool::new(&world, GT, blade).expect("blade"));
    history.execute(&world, ImportModel::from_mesh("bar", revolve(&BAR, 32)));
    (world, history)
}

fn create(world: &World, history: &mut History, name: &str, params: OpParams) {
    let cmd = CreateOperation::new(world, name, GT, SrcRef::Entity("bar".into()), params).expect("create");
    history.execute(world, cmd);
    history.execute(world, GenerateOperation::new(world, name).expect("generate"));
    let e = model::find_by_name(world, name).expect("op");
    assert_eq!(operation::status_of(world, &e), OpStatus::Ok, "{name} generates");
}

fn groove() -> OpParams {
    OpParams::GrooveFinish {
        tolerance: 0.05,
        clearance_z: Height::fixed(10.0),
        top_z: Height::fixed(-8.5),
        bottom_z: Height::fixed(-11.5),
        feed: 120.0,
        plunge_feed: 60.0,
    }
}

fn part() -> OpParams {
    OpParams::Part {
        tolerance: 0.05,
        clearance_z: Height::fixed(10.0),
        top_z: Height::fixed(0.0),
        bottom_z: Height::fixed(-20.0),
        stepdown: 1.0,
        feed: 120.0,
        plunge_feed: 60.0,
        bar_load: false,
        bar_advance: 0.0,
        bar_unload: false,
        bar_transfer: false,
        inspect_tol: 0.0,
    }
}

fn set_corner(world: &World, history: &mut History, q: Option<u8>) {
    let e = model::find_by_name(world, GT).expect("tool");
    let mut t = model::tool_of(world, &e).expect("a tool");
    t.orientation = q;
    history.execute(world, ChangeTool::new(world, GT, t).expect("a sane corner"));
}

/// The posted feed moves' Z (the plunges), in stream order.
fn posted_plunge_z(world: &World) -> Vec<f64> {
    posting::stream_records(world)
        .expect("posts")
        .into_iter()
        .filter_map(|r| match r {
            cnc::Record::Move(m) if m.kind != MoveKind::Rapid => Some(m.z),
            _ => None,
        })
        .collect()
}

fn cached_z(world: &World, op: &str) -> Vec<f64> {
    let e = model::find_by_name(world, op).expect("op");
    model::toolpath_of(world, &e).expect("cache").path.moves.iter().map(|m| m.to.z).collect()
}

fn program(world: &World) -> String {
    let records = posting::stream_records(world).expect("posts");
    engine::run(LATHE_CPS, &posting::machine_value(world), &cnc::serialize(&records))
        .expect("the lathe script runs")
        .program
}

fn all_at(zs: &[f64], z: f64, what: &str) {
    assert!(!zs.is_empty(), "{what}: no plunges posted");
    assert!(zs.iter().all(|v| (v - z).abs() < 1e-6), "{what}: plunges at {zs:?}, wanted Z{z}");
}

#[test]
fn a_3mm_groove_at_z_minus_10_posts_the_touched_off_corner() {
    let (world, mut history) = bar_world();
    create(&world, &mut history, "G1", groove());
    all_at(&posted_plunge_z(&world), -11.5, "the default left corner");
    let text = program(&world);
    assert!(text.contains("X8 Z-11.5 "), "the r4 floor at the left corner: {text}");
    assert!(!text.contains("Z-10 "), "never the middle of the blade: {text}");
    assert!(text.contains("ORIENTATION 2)"), "the tool note names the corner: {text}");
    set_corner(&world, &mut history, Some(1));
    all_at(&posted_plunge_z(&world), -8.5, "the right corner");
    set_corner(&world, &mut history, Some(6));
    all_at(&posted_plunge_z(&world), -10.0, "the width centre");
    // The verifier's and the stock sim's truth is untouched: the
    // cache is still the middle of the tip edge.
    let e = model::find_by_name(&world, "G1").expect("op");
    assert_eq!(operation::status_of(&world, &e), OpStatus::Ok, "a corner edit never stales the path");
    let cz = cached_z(&world, "G1");
    assert!(cz.iter().any(|z| (z + 10.0).abs() < 1e-6), "the cache plunges at the centre: {cz:?}");
}

#[test]
fn a_part_off_leaves_the_part_the_verified_kerf_leaves() {
    // Parting plane Z-20, a 3 mm kerf centred on it: the part keeps
    // Z-18.5..0. Whichever corner the blade was touched off on, the
    // posted plunge puts the kerf's free-end face at Z-18.5 - before
    // #641 the left-corner blade posted Z-20 and left 17 mm.
    for (q, side) in [(None, -1.0), (Some(2), -1.0), (Some(1), 1.0), (Some(6), 0.0)] {
        let (world, mut history) = bar_world();
        create(&world, &mut history, "P1", part());
        set_corner(&world, &mut history, q);
        let zs = posted_plunge_z(&world);
        all_at(&zs, -20.0 + side * W * 0.5, &format!("corner {q:?}"));
        let face = zs[0] - side * W * 0.5 + W * 0.5;
        assert!((face + 18.5).abs() < 1e-6, "corner {q:?}: the part's back face at {face}, wanted -18.5");
    }
}

#[test]
fn a_code_that_is_no_od_blade_is_refused_by_name() {
    let (world, mut history) = bar_world();
    create(&world, &mut history, "G1", groove());
    for q in [3u8, 4, 5, 7, 8] {
        set_corner(&world, &mut history, Some(q));
        let e = posting::stream_records(&world).expect_err("no OD blade");
        assert!(e.contains("orientation [1, 2, 6]") && e.contains(&format!("is orientation {q}")), "{e}");
        assert!(e.contains("G1") && e.contains(GT), "the refusal names the operation and the blade: {e}");
    }
}

#[test]
fn the_corner_edit_is_one_undoable_command() {
    let (world, mut history) = bar_world();
    create(&world, &mut history, "G1", groove());
    let e = model::find_by_name(&world, GT).expect("tool");
    assert_eq!(model::tool_of(&world, &e).and_then(|t| t.lathe_orientation()), Some(2), "the left-corner default");
    set_corner(&world, &mut history, Some(1));
    all_at(&posted_plunge_z(&world), -8.5, "after the edit");
    assert!(history.undo(&world));
    assert_eq!(model::tool_of(&world, &e).and_then(|t| t.orientation), None, "undo restores the unstated default");
    all_at(&posted_plunge_z(&world), -11.5, "after undo");
    assert!(history.redo(&world));
    assert_eq!(model::tool_of(&world, &e).and_then(|t| t.orientation), Some(1));
    all_at(&posted_plunge_z(&world), -8.5, "after redo");
}
