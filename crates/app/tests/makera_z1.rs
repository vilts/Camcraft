//! #868: the Makera Z1, a desktop mill with a manual quick-change and an
//! air blast (M7), posted through the Carvera post. Pins the file's
//! published figures, its place in the built-in list, an op's M7 / M9
//! through carvera.cps, and the travel / spindle refusals by name.
//! Provenance: our own tests over our own machine file; figures from
//! Makera's Z1 pages (200 x 200 x 100 mm, 150 W, quick-change, AeroDust
//! blower) and 0-13000 rpm / 1200 mm/min from the cnx-software Z1 review;
//! see docs/decisions/868-makera-z1.md.
//! Gate map (M4.4): covers app::posting (MACHINES), app::machine (parse,
//! limit checks over a shipped file), assets/posts/carvera.cps.

use flecs_ecs::prelude::World;

use camcraft_app::commands::{AddTool, CreateOperation, GenerateOperation, History, LoadMachine, SetStock};
use camcraft_app::machine::{parse_mch, Machine};
use camcraft_app::operation::{CoolantMode, Height, OpParams, SrcRef};
use camcraft_app::post::{self, cnc};
use camcraft_app::posting;
use camcraft_app::stock::StockShape;
use camcraft_app::tools::{catalog, Tool};
use camcraft_app::world::new_world;

const Z1: &str = "Makera Z1";

fn z1() -> Machine {
    let text = posting::MACHINES.iter().find(|(n, _)| *n == Z1).map(|(_, t)| *t).expect("Z1 is shipped");
    parse_mch(text).expect("the Z1 parses")
}

fn face(cut_z: f64) -> OpParams {
    OpParams::Face {
        holes: Default::default(),
        blend_turns: false,
        tilt: None,
        stepover: 3.0,
        clearance_z: Height::fixed(25.0),
        cut_z: Height::fixed(cut_z),
        top_z: None,
        stepdown: None,
        feed: 1000.0,
        plunge_feed: 300.0,
        climb: true,
    }
}

/// A world on the Z1 with one generated face op over a `dx` x 60 stock.
fn world_on_z1(dx: f64, tool: Option<Tool>) -> World {
    let world = new_world();
    let mut h = History::default();
    h.execute(&world, LoadMachine::new(&world, "z1.mch".into(), Some(z1())).expect("sane"));
    h.execute(&world, SetStock::new(&world, StockShape::Box { dx, dy: 60.0, dz: 20.0 }, Default::default()).expect("stock"));
    let name = "EM-0600-4FL";
    let tool = tool.unwrap_or_else(|| catalog().into_iter().find(|(n, _)| *n == name).map(|(_, t)| t).expect("catalog"));
    h.execute(&world, AddTool::new(&world, name, tool).expect("tool"));
    h.execute(&world, CreateOperation::new(&world, "Face1", name, SrcRef::Stock, face(19.5)).expect("create"));
    h.execute(&world, GenerateOperation::new(&world, "Face1").expect("generate"));
    world
}

#[test]
fn the_z1_is_listed_and_states_makeras_figures() {
    assert!(posting::MACHINES.iter().any(|(n, _)| *n == Z1), "listed as a built-in");
    let m = z1();
    assert_eq!(m.model, Z1);
    assert_eq!(m.kind.as_deref(), Some("mill"));
    assert_eq!(m.units, None, "mm");
    let want = [200.0, 200.0, 100.0];
    assert_eq!(m.axes.len(), 3);
    for (a, w) in m.axes.iter().zip(want) {
        let (lo, hi) = a.travel.expect("travel");
        assert!((hi - w).abs() < 1e-6 && (lo + w).abs() < 1e-6, "{} {lo}..{hi}", a.name);
        assert_eq!((a.feed, a.rapid), (Some(1200.0), Some(1200.0)), "{}", a.name);
    }
    assert_eq!((m.spindle_min, m.spindle_max, m.spindle_power), (None, Some(13000.0), Some(0.15)));
    assert_eq!(m.tool_count, None, "manual quick-change: no changer stated");
    let codes: Vec<_> = m.coolants.iter().map(|c| (c.name.as_str(), c.m_code.as_str())).collect();
    assert_eq!(codes, [("Air", "M7")]);
}

#[test]
fn a_default_op_on_the_z1_posts_m7_m9_through_the_carvera_post() {
    let world = world_on_z1(60.0, None);
    let op = camcraft_app::model::find_by_name(&world, "Face1")
        .and_then(|e| camcraft_app::model::operation_of(&world, &e))
        .expect("op");
    assert_eq!(op.coolant, CoolantMode::Air);
    assert!(posting::verification_gate(&world).is_empty(), "{:?}", posting::verification_gate(&world));
    let script = posting::BUILTINS.iter().find(|(n, _)| *n == "Makera Carvera").map(|(_, s)| *s).expect("post");
    let records = posting::stream_records(&world).expect("streams");
    let out = post::post(script, &posting::machine_value(&world), &cnc::serialize(&records)).expect("runs");
    assert!(out.warnings.is_empty(), "{:?}\n{}", out.warnings, out.program);
    let count = |c: &str| out.program.lines().filter(|l| l.split_whitespace().any(|w| w == c)).count();
    assert_eq!((count("M7"), count("M9"), count("M8")), (1, 1, 0), "{}", out.program);
}

#[test]
fn travel_past_the_z1_refuses_by_name() {
    // A 500 mm stock faced end to end runs X past the 200 mm half-stroke.
    let world = world_on_z1(500.0, None);
    let gate = posting::verification_gate(&world).join("\n");
    assert!(gate.contains("machine Makera Z1") && gate.contains("in X, the stroke is -200.000..200.000"), "{gate}");
}

#[test]
fn a_tool_past_the_z1s_spindle_ceiling_refuses_by_name() {
    let (_, mut fast) = catalog().into_iter().find(|(n, _)| *n == "EM-0600-4FL").expect("catalog");
    fast.rpm = 18000.0;
    let world = world_on_z1(60.0, Some(fast));
    let gate = posting::verification_gate(&world).join("\n");
    assert!(gate.contains("machine Makera Z1") && gate.contains("above the machine's 13000"), "{gate}");
}
