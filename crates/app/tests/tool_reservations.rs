//! Provenance: original regressions for machine tool reservations in issue #2604.
//! Gate map (M4.4): covers app::commands::tool, app::commands::tool_number,
//! app::commands::tool_import, app::tool_numbers, app::tool_reservations, app::machine, app::posting,
//! post::machine::tooling, post::machine::parse.

use camcraft_app::commands::{
    AddTool, CreateOperation, GenerateOperation, History, ImportTools, LoadMachine,
    SetStock, SetToolNumber,
};
use camcraft_app::machine;
use camcraft_app::model;
use camcraft_app::operation::{Height, OpParams, SrcRef};
use camcraft_app::posting;
use camcraft_app::schema::{Location, ToolDef};
use camcraft_app::stock::StockShape;
use camcraft_app::tool_library::LibraryTool;
use camcraft_app::tools::{Tool, catalog, probe_tool};
use camcraft_app::world::new_world;
use flecs_ecs::prelude::*;

fn mill() -> Tool {
    catalog().into_iter().find(|(name, _)| *name == "EM-0400-2FL")
        .map(|(_, tool)| tool).expect("catalog end mill")
}

fn mount(world: &World, history: &mut History) {
    let machine = machine::parse_mch(r#"{
        "general": {"model": "Reserved probe pocket"},
        "tooling": {"reserved": [
            {"number": 1, "use": "probe"},
            {"name": "PROBE", "use": "probe"}
        ]}
    }"#).expect("valid reservation machine");
    history.execute(world, LoadMachine::new(world, "reserved.mch".into(), Some(machine)).expect("mount"));
}

fn mounted() -> (World, History) {
    let world = new_world();
    let mut history = History::default();
    mount(&world, &mut history);
    (world, history)
}

fn number(world: &World, name: &str) -> Option<u32> {
    model::find_by_name(world, name).and_then(|e| model::tool_number_of(world, &e))
}

#[test]
fn added_cutter_skips_probe_pocket_and_probe_takes_it() {
    let (world, mut history) = mounted();
    history.execute(&world, AddTool::new(&world, "End mill", mill()).expect("mill"));
    assert_eq!(number(&world, "End mill"), Some(2), "T1 belongs to the probe");
    history.execute(&world, AddTool::new(&world, "PROBE", probe_tool(4.0, 30.0)).expect("probe"));
    assert_eq!(number(&world, "PROBE"), Some(1), "the probe takes its reserved pocket");
    assert!(history.undo(&world));
    assert!(history.redo(&world));
    assert_eq!(number(&world, "PROBE"), Some(1), "redo keeps the reserved pocket");
}

#[test]
fn setting_cutter_to_reserved_pocket_refuses_by_name() {
    let (world, mut history) = mounted();
    history.execute(&world, AddTool::new(&world, "End mill", mill()).expect("mill"));
    history.execute(&world, SetToolNumber::new(&world, "End mill", Some(12)).expect("T12"));
    let error = SetToolNumber::new(&world, "End mill", Some(1)).err()
        .expect("a cutter cannot use the probe pocket").to_string();
    assert!(error.contains("End mill") && error.contains("probe") && error.contains('1'), "{error}");
    assert_eq!(number(&world, "End mill"), Some(12));
    assert!(SetToolNumber::new(&world, "End mill", Some(0)).is_err(), "T0 empties the spindle");
}

#[test]
fn adding_cutter_with_reserved_probe_name_refuses_by_name() {
    let (world, _) = mounted();
    let error = AddTool::new(&world, "PROBE", mill()).err()
        .expect("a cutter cannot take a reserved probe name").to_string();
    assert!(error.contains("PROBE") && error.to_ascii_lowercase().contains("probe"), "{error}");
    assert!(model::find_by_name(&world, "PROBE").is_none());
}

#[test]
fn importing_cutter_skips_probe_pocket_and_probe_takes_it() {
    let (world, mut history) = mounted();
    let tools = vec![LibraryTool::bare("End mill", mill()), LibraryTool::bare("PROBE", probe_tool(4.0, 30.0))];
    history.execute(&world, ImportTools::new(&world, tools).expect("import"));
    assert_eq!(number(&world, "End mill"), Some(2), "import skips the probe pocket");
    assert_eq!(number(&world, "PROBE"), Some(1), "import gives the probe its pocket");
}

fn legacy_cutting_job() -> World {
    let world = new_world();
    let mut history = History::default();
    history.execute(&world, SetStock::new(&world, StockShape::Box {
        dx: 60.0, dy: 60.0, dz: 20.0,
    }, Location::default()).expect("stock"));
    history.execute(&world, AddTool::new(&world, "End mill", mill()).expect("mill"));
    history.execute(&world, CreateOperation::new(&world, "Face", "End mill", SrcRef::Stock, OpParams::Face {
        holes: Default::default(), blend_turns: false, blend_radius: None, tilt: None, stepover: 3.0,
        clearance_z: Height::fixed(25.0), cut_z: Height::fixed(19.5), top_z: None,
        stepdown: None, feed: 1200.0, plunge_feed: 300.0, climb: true,
        picks: Default::default(),
    }).expect("face"));
    history.execute(&world, GenerateOperation::new(&world, "Face").expect("generate"));
    // A saved or externally changed document can contain an old, unsafe number.
    let entity = model::find_by_name(&world, "End mill").expect("mill entity");
    entity.entity_view(&world).set(ToolDef { tool: mill(), number: Some(1) });
    mount(&world, &mut history);
    world
}

#[test]
fn existing_cutting_tool_in_probe_pocket_blocks_verification() {
    let world = legacy_cutting_job();
    let report = posting::verification_report(&world);
    let error = report.problems.join("; ");
    assert!(error.contains("End mill") && error.contains("probe") && error.contains('1'), "{error}");
}

#[test]
fn existing_cutting_tool_in_probe_pocket_blocks_post_stream() {
    let world = legacy_cutting_job();
    let error = posting::stream_records(&world).expect_err("unsafe reserved pocket must not post");
    assert!(error.contains("End mill") && error.contains("probe") && error.contains('1'), "{error}");
}
