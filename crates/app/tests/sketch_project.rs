//! #1862: a setup with only a stock and a placed DXF profile saves and
//! loads - the profile row (`kind: sketch`, project v71), its placement
//! on the stock face, its chains made again from the stored primitives,
//! and the 2D operation on it. A v62 file steps up unchanged; a corrupt
//! profile row refuses the file by name.
//!
//! Provenance: our own tests over our own project format; the DXF text
//! is hand-built from Autodesk's public ASCII DXF group conventions.
//!
//! Gate map (M4.4): covers app::project (sketch_dto, apply, validate,
//! wire, file PROJECT_VERSION), app::commands (ImportDxf, MoveSketch),
//! app::sketch, app::sketch_model, app::schema (Sketch).

use flecs_ecs::prelude::World;
use serde_json::{json, Value};

use camcraft_app::commands::{AddTool, Command, CommandError, CreateOperation, CreateSetup, DxfOptions, History, ImportDxf, MoveSketch, SetStock};
use camcraft_app::geometry::Point3;
use camcraft_app::model;
use camcraft_app::operation::{self, OpType, SrcRef};
use camcraft_app::project::{self, PROJECT_VERSION};
use camcraft_app::schema::Location;
use camcraft_app::stock::StockShape;
use camcraft_app::tools::catalog;
use camcraft_app::units::Unit;
use camcraft_app::world::new_world;
use camcraft_app::{sketch, sketch_model};

const TOOL: &str = "EM-0400-2FL";

/// A 4 x 2 rectangle with a rounded slot of lines and arcs inside it, a
/// loose line and a TEXT; no `$INSUNITS` (the caller states inches).
const DRAWING: &str = "0\nSECTION\n2\nENTITIES\n\
    0\nLWPOLYLINE\n8\ncut\n90\n4\n70\n1\n10\n0\n20\n0\n10\n4\n20\n0\n10\n4\n20\n2\n10\n0\n20\n2\n\
    0\nLINE\n8\nslot\n10\n1\n20\n0.75\n11\n3\n21\n0.75\n\
    0\nARC\n8\nslot\n10\n3\n20\n1\n40\n0.25\n50\n270\n51\n90\n\
    0\nLINE\n8\nslot\n10\n3\n20\n1.25\n11\n1\n21\n1.25\n\
    0\nARC\n8\nslot\n10\n1\n20\n1\n40\n0.25\n50\n90\n51\n270\n\
    0\nLINE\n8\nmarks\n10\n0.5\n20\n1.9\n11\n1.5\n21\n1.9\n\
    0\nTEXT\n8\nnotes\n10\n0\n20\n0\n40\n0.1\n1\nhello\n\
    0\nENDSEC\n0\nEOF\n";

fn run(world: &World, history: &mut History, cmd: Result<impl Command + 'static, CommandError>, what: &str) {
    history.execute(world, cmd.unwrap_or_else(|e| panic!("{what}: {e}")));
}

/// One setup `Sheet` with a 200 x 120 x 6 stock, a profile `plate` moved
/// and turned on it, and a 2D Contour on the profile. No model.
fn sheet_job() -> World {
    let world = new_world();
    let mut history = History::default();
    let tool = catalog().into_iter().find(|(n, _)| *n == TOOL).map(|(_, t)| t).expect("catalog tool");
    run(&world, &mut history, AddTool::new(&world, TOOL, tool), "tool");
    run(&world, &mut history, CreateSetup::new(&world, Some("Sheet"), None), "Sheet");
    let at = Location { pos: Point3::new(0.0, 0.0, 3.0), ..Default::default() };
    run(&world, &mut history, SetStock::new(&world, StockShape::Box { dx: 200.0, dy: 120.0, dz: 6.0 }, at), "stock");
    let options = DxfOptions { units: Some(Unit::Inches), join_tolerance: Some(0.02), name: Some("plate".into()) };
    run(&world, &mut history, ImportDxf::from_bytes(&world, Some("Sheet"), "plate.dxf", DRAWING.as_bytes(), &options), "import");
    run(&world, &mut history, MoveSketch::new(&world, "plate", -40.0, 12.5, -30.0), "move");
    let contour = operation::seed_params(OpType::Contour.ordinal());
    run(&world, &mut history, CreateOperation::new(&world, "Cut", TOOL, SrcRef::Entity("plate".into()), contour), "Cut");
    world
}

fn saved(world: &World) -> String {
    project::save_text(world).expect("the document saves")
}

fn reopened(text: &str) -> World {
    let world = new_world();
    project::open(&world, text).unwrap_or_else(|e| panic!("the file opens: {e}"));
    world
}

fn refused(v: &Value) -> String {
    let world = new_world();
    match project::open(&world, &v.to_string()) {
        Ok(()) => panic!("a corrupt profile row must refuse the file"),
        Err(e) => {
            assert!(model::named_entities(&world).is_empty(), "no half-loaded document");
            e.to_string()
        }
    }
}

/// The saved file as JSON, and the index of the profile's entity row.
fn file_and_row(text: &str) -> (Value, usize) {
    let v: Value = serde_json::from_str(text).expect("json");
    let row = v["entities"].as_array().expect("entities").iter().position(|e| e["kind"] == json!("sketch")).expect("a sketch row");
    (v, row)
}

#[test]
fn a_stock_and_profile_setup_saves_and_loads_with_its_2d_operation() {
    let world = sheet_job();
    let text = saved(&world);
    let (file, row) = file_and_row(&text);
    const {
        assert!(
            PROJECT_VERSION >= 71,
            "DXF profile rows require project v71 or later"
        );
    }
    assert_eq!(file["version"], json!(PROJECT_VERSION));
    let dto = &file["entities"][row];
    assert_eq!(dto["name"], json!("plate"));
    assert_eq!(dto["sketch"]["setup"], json!("Sheet"));
    assert_eq!(dto["sketch"]["insunits"], json!(0), "the file stated no unit");
    assert_eq!(dto["sketch"]["unit"], json!(1), "read in inches, the caller's answer");
    assert_eq!(dto["sketch"]["skipped"], json!({ "TEXT": 1 }));
    assert!(dto.get("facets").is_none(), "a profile row has no facets");
    assert!(dto["sketch"].get("chains").is_none(), "the chains are made again on load, never stored");

    let again = reopened(&text);
    let (e0, before) = sketch::find(&world, "plate").expect("profile");
    let (e1, after) = sketch::find(&again, "plate").expect("the profile loads");
    assert_eq!(after, before, "setup, primitives, layers, units, tolerance, chains and summary");
    assert_eq!(after.data.chains().closed.len(), 2, "the outer rectangle and the slot");
    assert_eq!(after.data.chains().open.len(), 1);
    assert!(after.data.units().overridden());
    assert_eq!(model::transform_of(&again, &e1), model::transform_of(&world, &e0), "the placement on the stock face");
    assert_eq!(sketch::world_pose(&again, &e1), sketch::world_pose(&world, &e0));
    assert_eq!(sketch::world_pose(&again, &e1).map(|p| p.pos), Some(Point3::new(-40.0, 12.5, 9.0)), "on the stock top");
    assert!(!model::is_model(&again, &e1) && model::model_entities(&again).is_empty(), "still not a model");

    // The setup with only a stock and a profile keeps its virtual reference and its operation.
    assert_eq!(sketch_model::of(&again, Some("Sheet")).map(|v| v.profiles), Some(vec!["plate".to_owned()]));
    let op = model::find_by_name(&again, "Cut").and_then(|e| model::operation_of(&again, &e)).expect("the 2D operation loads");
    assert_eq!(op.src, SrcRef::Entity("plate".into()));
    assert_eq!(op.params.kind(), OpType::Contour);
    assert!(sketch_model::checked_against(&again, &op).is_some());

    // Save, load, save: the same bytes.
    assert_eq!(saved(&again), text);
}

#[test]
fn rest_parameters_and_profile_rows_share_the_v71_project_file() {
    let world = sheet_job();
    let mut history = History::default();
    let mut pocket = operation::seed_params(OpType::Pocket.ordinal());
    let previous = camcraft_app::machining2d::RestParams {
        source: Default::default(),
        previous_tool_diameter: 12.0,
        previous_corner_radius: 1.0,
        previous_stock_to_leave: 0.2,
        allowance: 0.1,
    };
    if let operation::OpParams::Pocket { rest, strategy, entry, .. } = &mut pocket {
        *rest = Some(previous);
        *strategy = camcraft_app::machining2d::PocketStrategy::Waterline;
        *entry = camcraft_app::machining3d::Entry3D::Plunge;
    } else {
        panic!("the pocket seed must be a pocket");
    }
    run(&world, &mut history, CreateOperation::new(&world, "Rest", TOOL, SrcRef::Entity("plate".into()), pocket), "Rest");
    let text = saved(&world);
    let file: Value = serde_json::from_str(&text).expect("json");
    assert_eq!(file["version"], json!(PROJECT_VERSION), "main's additive rest keys do not consume the profile version");
    let again = reopened(&text);
    let op = model::find_by_name(&again, "Rest").and_then(|e| model::operation_of(&again, &e)).expect("the rest operation loads");
    assert!(matches!(op.params, operation::OpParams::Pocket { rest: Some(r), .. } if r == previous));
    assert!(sketch::find(&again, "plate").is_some(), "the profile DTO remains registered beside the rest DTO");
    assert_eq!(saved(&again), text, "both DTO payloads survive a round trip");
}

#[test]
fn a_v62_file_steps_up_unchanged() {
    // A document with no profile writes the bytes a v62 build wrote, but the version.
    let world = new_world();
    let mut history = History::default();
    run(&world, &mut history, SetStock::new(&world, StockShape::Box { dx: 50.0, dy: 40.0, dz: 10.0 }, Location::default()), "stock");
    let text = saved(&world);
    assert!(!text.contains("sketch"), "no profile, no new key");
    let mut v62: Value = serde_json::from_str(&text).expect("json");
    v62["version"] = json!(62);
    let again = reopened(&v62.to_string());
    assert_eq!(saved(&again), text, "the v62 file loads unchanged and saves as v71");
}

#[test]
fn a_v63_break_through_file_steps_up_without_losing_the_stated_limit() {
    // Build main's v63 shape: no sketch entity, and a stated zero exit allowance.
    let world = new_world();
    let mut history = History::default();
    let drill_tool = "DR-0600";
    let tool = catalog().into_iter().find(|(n, _)| *n == drill_tool).map(|(_, t)| t).expect("catalog tool");
    run(&world, &mut history, AddTool::new(&world, drill_tool, tool), "tool");
    run(&world, &mut history, SetStock::new(&world, StockShape::Box { dx: 50.0, dy: 40.0, dz: 10.0 }, Location::default()), "stock");
    let mut drill = operation::seed_params(OpType::Drill.ordinal());
    if let operation::OpParams::Drill { break_through, .. } = &mut drill {
        *break_through = Some(0.0);
    } else {
        panic!("the drill seed must be a drill");
    }
    run(&world, &mut history, CreateOperation::new(&world, "Drill", drill_tool, SrcRef::Stock, drill.clone()), "Drill");
    let mut v63: Value = serde_json::from_str(&saved(&world)).expect("json");
    assert!(!v63.to_string().contains("sketch"), "a main v63 file has no profile");
    let row = v63["entities"].as_array().expect("entities").iter().position(|e| e["name"] == json!("Drill")).expect("drill row");
    assert_eq!(v63["entities"][row]["operation"]["params"]["drill"]["break_through"], json!(0.0), "the input states the safety limit");
    v63["version"] = json!(63);

    let again = reopened(&v63.to_string());
    let op = model::find_by_name(&again, "Drill").and_then(|e| model::operation_of(&again, &e)).expect("the drill loads");
    assert_eq!(op.params, drill, "migration preserves the stated zero allowance");
    let text = saved(&again);
    v63["version"] = json!(PROJECT_VERSION);
    assert_eq!(serde_json::from_str::<Value>(&text).expect("json"), v63, "only the project version changes");
    assert_eq!(saved(&reopened(&text)), text, "the v71 file has a byte-stable round trip");
}

#[test]
fn a_v64_cutting_row_steps_up_without_losing_its_limits() {
    let world = sheet_job();
    let mut file: Value = serde_json::from_str(&saved(&world)).expect("json");
    // Main's v64 document has no profiles, but can state the Unknown row's limits.
    file["entities"].as_array_mut().expect("entities").retain(|e| e["kind"] != json!("sketch") && e["name"] != json!("Cut"));
    let tool = file["entities"].as_array_mut().expect("entities").iter_mut().find(|e| e["name"] == json!(TOOL)).expect("tool");
    tool["tool"]["cutting_data"] = json!([{ "material": "unknown", "rpm": 3100.0, "feed": 410.0, "stepdown": 1.5, "stepover": 2.0 }]);
    tool["tool"]["rpm"] = json!(3100.0);
    tool["tool"]["feed"] = json!(410.0);
    file["version"] = json!(64);
    let again = reopened(&file.to_string());
    file["version"] = json!(PROJECT_VERSION);
    let text = saved(&again);
    assert_eq!(serde_json::from_str::<Value>(&text).expect("json"), file, "only the version changes; cutting limits survive");
    assert_eq!(saved(&reopened(&text)), text);
}

#[test]
fn a_corrupt_profile_row_refuses_the_file_by_name() {
    let text = saved(&sheet_job());
    let (file, row) = file_and_row(&text);
    let with = |edit: &dyn Fn(&mut Value)| {
        let mut v = file.clone();
        edit(&mut v["entities"][row]);
        refused(&v)
    };
    // Off the stock face: a Z offset, and a tilt.
    let why = with(&|e| e["translation"][2] = json!(1.0));
    assert!(why.contains("DXF profile 'plate' is placed off its stock face"), "{why}");
    let why = with(&|e| e["rotation"] = json!([0.5, 0.5, 0.5, 0.5]));
    assert!(why.contains("placed off its stock face"), "{why}");
    // A row of kind sketch without its drawing, and a drawing on a row of another kind.
    let why = with(&|e| {
        e.as_object_mut().expect("row").remove("sketch");
    });
    assert!(why.contains("'plate' is a DXF profile without its drawing"), "{why}");
    let why = with(&|e| {
        e.as_object_mut().expect("row").remove("kind");
    });
    assert!(why.contains("carries a DXF profile's drawing but is not one"), "{why}");
    // A drawing that cannot be read back.
    let why = with(&|e| e["sketch"]["unit"] = json!(0));
    assert!(why.contains("DXF profile 'plate': unit code 0 is not a unit"), "{why}");
    let why = with(&|e| e["sketch"]["primitives"][0]["line"]["layer"] = json!(99));
    assert!(why.contains("names layer 99, which the profile does not list"), "{why}");
    let why = with(&|e| e["sketch"]["join_tolerance"] = json!(500.0));
    assert!(why.contains("DXF profile 'plate'") && why.contains("join tolerance"), "{why}");
    let why = with(&|e| e["sketch"]["primitives"] = json!([]));
    assert!(why.contains("no line, arc or circle"), "{why}");
    let why = with(&|e| e["sketch"]["primitives"][0]["line"]["a"] = json!([1.0e300, 0.0]));
    assert!(why.contains("DXF profile 'plate'"), "{why}");
    let why = with(&|e| e["sketch"]["setup"] = json!(""));
    assert!(why.contains("names an empty setup"), "{why}");
    // Two profile rows under one name.
    let mut twice = file.clone();
    let copy = twice["entities"][row].clone();
    twice["entities"].as_array_mut().expect("entities").push(copy);
    let why = refused(&twice);
    assert!(why.contains("two DXF profiles are named 'plate'"), "{why}");
    // The untouched file still opens.
    reopened(&file.to_string());
}

fn assert_reserved_profile_name_refused(name: &str) {
    let text = saved(&sheet_job());
    let (mut corrupt, row) = file_and_row(&text);
    corrupt["entities"][row]["name"] = json!(name);
    let expected = format!("'{name}' is not a usable name for a DXF profile");
    let why = refused(&corrupt);
    assert!(why.contains(&expected), "{why}");

    // Refusing an older file with a reserved name must preserve the current job.
    let existing = sheet_job();
    let before = saved(&existing);
    let profile = model::find_by_name(&existing, "plate").expect("existing profile");
    let why = project::open(&existing, &corrupt.to_string()).expect_err("reserved name must refuse").to_string();
    assert!(why.contains(&expected), "{why}");
    assert_eq!(saved(&existing), before, "a refusal made no world writes");
    assert_eq!(model::find_by_name(&existing, "plate"), Some(profile), "the current profile entity survives");

    // The unchanged file is a valid control and still loads its profile.
    let control = reopened(&text);
    assert!(sketch::find(&control, "plate").is_some());
    assert_eq!(saved(&control), text);
}

#[test]
fn stored_stock_profile_name_refuses_before_world_writes() {
    assert_reserved_profile_name_refused("stock");
}

#[test]
fn stored_uppercase_stock_profile_name_refuses_before_world_writes() {
    assert_reserved_profile_name_refused("STOCK");
}

#[test]
fn stored_mixed_case_stock_profile_name_refuses_before_world_writes() {
    assert_reserved_profile_name_refused("sToCk");
}

#[test]
fn stored_profile_names_follow_the_import_naming_rules() {
    for name in ["Stock", "G54", "Other/Stock", "Other/WCS", "Other/G55", "   "] {
        assert_reserved_profile_name_refused(name);
    }
}

fn assert_overflowing_profile_summary_refused(empty: bool) {
    let text = saved(&sheet_job());
    let (mut corrupt, row) = file_and_row(&text);
    corrupt["entities"][row]["sketch"]["skipped"] = json!({ "TEXT": usize::MAX, "DIMENSION": 1 });
    if empty {
        corrupt["entities"][row]["sketch"]["primitives"] = json!([]);
    }
    let why = refused(&corrupt);
    assert!(why.contains("DXF profile 'plate'") && why.contains("invalid DXF import summary"), "{why}");

    // A failed open must also leave an existing document intact.
    let existing = sheet_job();
    let before = saved(&existing);
    let why = project::open(&existing, &corrupt.to_string()).expect_err("invalid summary must refuse").to_string();
    assert!(why.contains("invalid DXF import summary"), "{why}");
    assert_eq!(saved(&existing), before, "a refusal made no world writes");
}

#[test]
fn overflowing_profile_summary_refuses_before_world_writes() {
    assert_overflowing_profile_summary_refused(false);
}

#[test]
fn overflowing_empty_profile_summary_refuses_before_world_writes() {
    assert_overflowing_profile_summary_refused(true);
}

#[test]
fn valid_stored_profile_summary_loads_and_get_job_reports_its_total() {
    let (mut file, row) = file_and_row(&saved(&sheet_job()));
    file["entities"][row]["sketch"]["skipped"] = json!({ "TEXT": 2, "DIMENSION": 1 });
    let world = reopened(&file.to_string());
    let request = json!({ "jsonrpc": "2.0", "id": 1, "method": "tools/call", "params": { "name": "get_job", "arguments": {} } });
    let answer = camcraft_app::mcp::answer_live(&world, &mut History::default(), None, &request.to_string());
    let reply: Value = serde_json::from_str(&answer.reply.expect("get_job responds")).expect("JSON reply");
    assert_ne!(reply["result"]["isError"], json!(true), "{reply}");
    assert_eq!(reply["result"]["structuredContent"]["profiles"][0]["skipped_total"], json!(3));
    assert_eq!(reply["result"]["structuredContent"]["profiles"][0]["skipped"], json!({ "TEXT": 2, "DIMENSION": 1 }));
}

#[test]
fn delete_undo_keeps_both_stock_material_and_profile_drawing() {
    use camcraft_app::commands::{DeleteEntity, SetStockMaterial};
    use camcraft_app::tools::WorkMaterial;
    let world = sheet_job();
    let mut history = History::default();
    history.execute(&world, SetStockMaterial::new(&world, Some(WorkMaterial::Brass)).expect("material"));
    let before = saved(&world);
    let stock_name = camcraft_app::setup::stock_entity_name(&world, Some("Sheet"));
    for name in ["plate", stock_name.as_str()] {
        history.execute(&world, DeleteEntity::new(&world, name).expect("delete"));
        assert!(model::find_by_name(&world, name).is_none());
        assert!(history.undo(&world));
        assert_eq!(saved(&world), before, "undo restores every payload of {name}");
    }
}
