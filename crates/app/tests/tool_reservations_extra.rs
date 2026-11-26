//! Provenance: original edit and legacy-name reservation regressions for #2604.
//! Gate map (M4.4): covers app::commands::tool, app::commands::tool_import,
//! app::tool_numbers, app::machine, app::posting, post::machine::tooling.

use camcraft_app::commands::{
    AddTool, ChangeTool, CreateOperation, GenerateOperation, History, ImportTools,
    LoadMachine, SetStock, SetToolNumber,
};
use camcraft_app::{machine, model, posting};
use camcraft_app::operation::{Height, OpParams, SrcRef};
use camcraft_app::schema::Location;
use camcraft_app::stock::StockShape;
use camcraft_app::tool_library::LibraryTool;
use camcraft_app::tools::{Tool, catalog, probe_tool};
use camcraft_app::world::new_world;
use flecs_ecs::prelude::World;

fn mill() -> Tool {
    catalog().into_iter().find(|(name, _)| *name == "EM-0400-2FL")
        .map(|(_, tool)| tool).expect("catalog end mill")
}

fn mount(world: &World, history: &mut History, entries: &str) {
    let text = format!(r#"{{"tooling": {{"reserved": {entries}}}}}"#);
    let machine = machine::parse_mch(&text).expect("reservation machine");
    history.execute(world, LoadMachine::new(world, "reserved.mch".into(), Some(machine)).expect("mount"));
}

#[test]
fn retyping_probe_in_reserved_pocket_as_cutter_refuses() {
    let world = new_world();
    let mut history = History::default();
    history.execute(&world, AddTool::new(&world, "Touch", probe_tool(4.0, 30.0)).expect("probe"));
    assert_eq!(model::find_by_name(&world, "Touch").and_then(|e| model::tool_number_of(&world, &e)), Some(1));
    mount(&world, &mut history, r#"[{"number":1,"use":"probe"}]"#);
    let error = ChangeTool::new(&world, "Touch", mill()).err().expect("reserved probe pocket").to_string();
    assert!(error.contains("Touch") && error.contains("probe"), "{error}");
}

#[test]
fn retyping_probe_with_reserved_name_as_cutter_refuses() {
    let world = new_world();
    let mut history = History::default();
    history.execute(&world, AddTool::new(&world, "PROBE", probe_tool(4.0, 30.0)).expect("probe"));
    mount(&world, &mut history, r#"[{"name":"PROBE","use":"probe"}]"#);
    let error = ChangeTool::new(&world, "PROBE", mill()).err().expect("reserved probe name").to_string();
    assert!(error.contains("PROBE") && error.to_ascii_lowercase().contains("probe"), "{error}");
}

#[test]
fn importing_explicit_reserved_number_for_cutter_refuses() {
    let world = new_world();
    let mut history = History::default();
    mount(&world, &mut history, r#"[{"number":1,"use":"probe"}]"#);
    let mut cutter = LibraryTool::bare("End mill", mill());
    cutter.number = Some(1);
    let error = ImportTools::new(&world, vec![cutter]).err().expect("reserved number").to_string();
    assert!(error.contains("End mill") && error.contains("probe") && error.contains('1'), "{error}");
    assert!(model::find_by_name(&world, "End mill").is_none());
}

#[test]
fn generic_machine_keeps_first_cutter_at_t1_and_accepts_probe_word() {
    let world = new_world();
    let mut history = History::default();
    let generic = machine::parse_mch("{}").expect("generic machine");
    history.execute(&world, LoadMachine::new(&world, "generic.mch".into(), Some(generic)).expect("mount"));
    history.execute(&world, AddTool::new(&world, "PROBE", mill()).expect("unreserved name"));
    let entity = model::find_by_name(&world, "PROBE").expect("tool");
    assert_eq!(model::tool_number_of(&world, &entity), Some(1));
    assert!(SetToolNumber::new(&world, "PROBE", Some(0)).is_err(), "zero never names a tool");
}

fn existing_named_cutting_job() -> World {
    let world = new_world();
    let mut history = History::default();
    history.execute(&world, SetStock::new(&world, StockShape::Box {
        dx: 60.0, dy: 60.0, dz: 20.0,
    }, Location::default()).expect("stock"));
    history.execute(&world, AddTool::new(&world, "PROBE", mill()).expect("old cutter"));
    history.execute(&world, CreateOperation::new(&world, "Face", "PROBE", SrcRef::Stock, OpParams::Face {
        holes: Default::default(), blend_turns: false, blend_radius: None, tilt: None, stepover: 3.0,
        clearance_z: Height::fixed(25.0), cut_z: Height::fixed(19.5), top_z: None,
        stepdown: None, feed: 1200.0, plunge_feed: 300.0, climb: true,
        picks: Default::default(),
    }).expect("face"));
    history.execute(&world, GenerateOperation::new(&world, "Face").expect("generate"));
    mount(&world, &mut history, r#"[{"name":"PROBE","use":"probe"}]"#);
    world
}

#[test]
fn existing_cutting_tool_with_reserved_name_blocks_verification() {
    let world = existing_named_cutting_job();
    let error = posting::verification_report(&world).problems.join("; ");
    assert!(error.contains("PROBE") && error.to_ascii_lowercase().contains("reserved"), "{error}");
}

#[test]
fn existing_cutting_tool_with_reserved_name_blocks_post_stream() {
    let world = existing_named_cutting_job();
    let error = posting::stream_records(&world).expect_err("reserved tool name must not post");
    assert!(error.contains("PROBE") && error.to_ascii_lowercase().contains("reserved"), "{error}");
}

#[test]
fn machine_validator_names_a_bad_reservation() {
    for entries in [
        r#"[{"number":0,"use":"probe"}]"#,
        r#"[{"number":1,"use":"cutter"}]"#,
        r#"[{"name":"","use":"probe"}]"#,
    ] {
        let text = format!(r#"{{"tooling": {{"reserved": {entries}}}}}"#);
        let refusals = machine::validate::validate_text(&text);
        assert!(refusals.iter().any(|r| r.key.starts_with("tooling.reserved")), "{refusals:?}");
    }
}
