//! #869: the air blast. A machine whose only coolant is `Air` M7 (the
//! Makera Carvera) starts new operations on Air and posts M7 / M9 for
//! them; an op asks for Air by name and every post that reads the ask
//! resolves it from the machine's list (M7 from `Air`, never M8 from a
//! seeded `Flood`), or refuses it by name when the machine states none -
//! never a wrong M-code, never silently dry.
//!
//! Provenance: our own tests over our own fixtures; M7 / M9 "airflow"
//! from the Carvera firmware's supported-codes list, RS-274 M8 / M9.
//!
//! Gate map (M4.4): covers post::post::coolant (CoolantMode::Air,
//! machine_default), post::machine (COOLANT_KINDS),
//! app::commands::operation::create (the new op's coolant),
//! assets/posts/carvera.cps, assets/posts/tormach.cps and the generic
//! posts' air ask.

use flecs_ecs::prelude::World;

use camcraft_app::commands::{
    AddTool, CreateOperation, EditOperationParams, GenerateOperation, History, LoadMachine, SetStock,
};
use camcraft_app::machine::{parse_mch, Coolant, Machine};
use camcraft_app::operation::{CoolantMode, Height, OpParams, SrcRef};
use camcraft_app::post::{self, cnc};
use camcraft_app::posting;
use camcraft_app::stock::StockShape;
use camcraft_app::tools::catalog;
use camcraft_app::world::new_world;

const GENERIC: [&str; 6] = ["Fanuc", "Haas", "Siemens 840D", "Heidenhain", "Mazak", "Doosan DNM"];
const CARVERA: &str = "Makera Carvera";

fn face_params(cut_z: f64) -> OpParams {
    OpParams::Face {
        holes: Default::default(),
        blend_turns: false,
        tilt: None,
        stepover: 3.0,
        clearance_z: Height::fixed(25.0),
        cut_z: Height::fixed(cut_z),
        top_z: None,
        stepdown: None,
        feed: 1200.0,
        plunge_feed: 300.0,
        climb: true,
    }
}

fn coolant(name: &str, code: &str) -> Coolant {
    Coolant { name: name.into(), m_code: code.into() }
}

fn shipped_carvera() -> Machine {
    let text = posting::MACHINES.iter().find(|(n, _)| *n == CARVERA).map(|(_, t)| *t).expect("shipped");
    parse_mch(text).expect("the Carvera parses")
}

fn bench(coolants: Vec<Coolant>) -> Machine {
    // States Z0 as its safe retract: the Siemens post refuses a machine
    // that leaves "up" unstated, which is not this suite's law.
    Machine { model: "AirBench".into(), coolants, safe_retract_z: Some(0.0), ..Default::default() }
}

/// A world with `machine` mounted FIRST, then one generated face op
/// (`Face1`), its coolant left as the machine's default unless `ask`.
fn one_face(machine: Machine, ask: Option<CoolantMode>) -> World {
    let world = new_world();
    let mut history = History::default();
    history.execute(&world, LoadMachine::new(&world, "air.mch".into(), Some(machine)).expect("sane"));
    let stock = SetStock::new(&world, StockShape::Box { dx: 60.0, dy: 60.0, dz: 20.0 }, Default::default());
    history.execute(&world, stock.expect("box stock"));
    let t = "EM-0600-4FL";
    let tool = catalog().into_iter().find(|(n, _)| *n == t).map(|(_, t)| t).expect("catalog entry");
    history.execute(&world, AddTool::new(&world, t, tool).expect("catalog tool"));
    let cmd = CreateOperation::new(&world, "Face1", t, SrcRef::Stock, face_params(19.5));
    history.execute(&world, cmd.expect("create"));
    history.execute(&world, GenerateOperation::new(&world, "Face1").expect("generate"));
    if let Some(c) = ask {
        history.execute(&world, EditOperationParams::with_coolant(&world, "Face1", c).expect("coolant edit"));
    }
    world
}

fn op_coolant(world: &World) -> CoolantMode {
    camcraft_app::model::find_by_name(world, "Face1")
        .and_then(|e| camcraft_app::model::operation_of(world, &e))
        .map(|o| o.coolant)
        .expect("the op")
}

fn script(name: &str) -> &'static str {
    posting::BUILTINS.iter().find(|(n, _)| *n == name).map(|(_, s)| *s).expect("a built-in")
}

fn post_with(world: &World, name: &str) -> post::PostOutput {
    let records = posting::stream_records(world).expect("a fresh world streams");
    post::post(script(name), &posting::machine_value(world), &cnc::serialize(&records)).expect("the script runs")
}

fn lines_with(program: &str, code: &str) -> usize {
    program.lines().filter(|l| l.split_whitespace().any(|w| w.trim_end_matches('*') == code)).count()
}

#[test]
fn an_air_only_carvera_starts_a_default_op_on_air_and_posts_m7_m9() {
    let world = one_face(shipped_carvera(), None);
    assert_eq!(op_coolant(&world), CoolantMode::Air, "the machine's default coolant");
    let out = post_with(&world, CARVERA);
    assert!(out.warnings.is_empty(), "{:?}\n{}", out.warnings, out.program);
    assert_eq!(lines_with(&out.program, "M7"), 1, "{}", out.program);
    assert_eq!(lines_with(&out.program, "M9"), 1, "{}", out.program);
    assert_eq!(lines_with(&out.program, "M8"), 0, "never flood on an air machine: {}", out.program);
}

#[test]
fn an_air_op_posts_the_air_code_not_the_seeded_flood() {
    // The user's profile: the blank machine's seeded Flood M8 kept, an
    // Air M7 added. Flood stays the default; the op asks for Air.
    let both = || bench(vec![coolant("Flood", "M8"), coolant("Air", "M7")]);
    for name in GENERIC.iter().copied().chain([CARVERA]) {
        let world = one_face(both(), None);
        assert_eq!(op_coolant(&world), CoolantMode::Flood, "a machine with a Flood code defaults to it");
        let world = one_face(both(), Some(CoolantMode::Air));
        let out = post_with(&world, name);
        assert!(out.warnings.is_empty(), "{name}: {:?}", out.warnings);
        assert_eq!(lines_with(&out.program, "M7"), 1, "{name}: {}", out.program);
        assert_eq!(lines_with(&out.program, "M8"), 0, "{name}: never M8 for an air ask: {}", out.program);
        // "Air blast" reads as Air through the spelling law.
        let world = one_face(bench(vec![coolant("Flood", "M8"), coolant("air_blast", "M7")]), Some(CoolantMode::Air));
        assert_eq!(lines_with(&post_with(&world, name).program, "M7"), 1, "{name}: air blast");
    }
}

#[test]
fn an_ask_the_machine_lacks_is_refused_by_name() {
    // Air on a flood/mist machine: every post that reads the ask refuses
    // it by name - never M7 from the Mist entry, never M8.
    for name in GENERIC.iter().copied().chain([CARVERA]) {
        let world = one_face(bench(vec![coolant("Flood", "M8"), coolant("Mist", "M7")]), Some(CoolantMode::Air));
        let out = post_with(&world, name);
        assert!(
            out.warnings.iter().any(|w| w.contains("operation Face1") && w.contains("no air coolant code")),
            "{name}: {:?}",
            out.warnings
        );
    }
    // Flood on the air-only Carvera: refused by name, never M7 for a flood ask.
    let world = one_face(shipped_carvera(), Some(CoolantMode::Flood));
    let out = post_with(&world, CARVERA);
    assert!(
        out.warnings.iter().any(|w| w.contains("operation Face1") && w.contains("no flood coolant code") && w.contains("Air")),
        "{:?}",
        out.warnings
    );
    // The Tormach mill post reads no ask: an Air op is the engine's
    // refusal, and its flood never falls back to the air code.
    let world = one_face(bench(vec![coolant("Flood", "M8"), coolant("Air", "M7")]), Some(CoolantMode::Air));
    let out = post_with(&world, "Tormach PathPilot");
    assert!(out.warnings.iter().any(|w| w.contains("operation Face1") && w.contains("coolantModes")), "{:?}", out.warnings);
    let world = one_face(bench(vec![coolant("Air", "M7")]), Some(CoolantMode::Flood));
    let out = post_with(&world, "Tormach PathPilot");
    assert!(out.warnings.iter().any(|w| w.contains("no flood coolant code") && w.contains("Face1")), "{:?}", out.warnings);
    assert_eq!(lines_with(&out.program, "M7"), 0, "{}", out.program);
}

#[test]
fn the_machine_default_coolant_reads_the_coded_list() {
    let d = |list: Vec<Coolant>| CoolantMode::machine_default(&list);
    assert_eq!(d(vec![]), CoolantMode::Flood, "no list: every op before #869");
    assert_eq!(d(vec![coolant("Air", "M7")]), CoolantMode::Air);
    assert_eq!(d(vec![coolant("Flood", "M8"), coolant("Air", "M7")]), CoolantMode::Flood);
    assert_eq!(d(vec![coolant("Air", "M7"), coolant("Flood", "M8")]), CoolantMode::Flood, "flood wins wherever listed");
    assert_eq!(d(vec![coolant("Flood", ""), coolant("Air", "M7")]), CoolantMode::Air, "a codeless Flood posts nothing");
    assert_eq!(d(vec![coolant("Mist", "M7")]), CoolantMode::Mist);
    assert_eq!(d(vec![coolant("Brine", "M51"), coolant("Air", "M7")]), CoolantMode::Flood, "a shop's own name is its flood");
    assert_eq!(d(vec![coolant("Through spindle", "M88")]), CoolantMode::ThroughTool);
    assert_eq!(CoolantMode::Air.param_word(), Some("air"));
    assert!(CoolantMode::ALL.contains(&CoolantMode::Air));
    assert!(camcraft_app::machine::COOLANT_KINDS.contains(&"Air"), "the editor's pick list offers Air");
}
