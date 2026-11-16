//! #890: a tool carries cutting data per workpiece material. The table
//! round-trips the project file and the tool library, is absent from
//! the files of tools without it (byte for byte), is refused by tool
//! name when unusable, and seeds the running numbers only through a
//! command - stating the material, adding the tool, editing the table,
//! retargeting an op's tool, quick-adding an op - never on load. An op
//! on a document of material X posts X's row.
//! Provenance: our own tests over our own command model.
//!
//! Gate map (M4.4): covers app::commands (tool, cutting_data, operation), app::cutting_rows, app::project (ToolDto), app::tool_library, app::mcp.

use camcraft_app::commands::{
    AddTool, ChangeTool, CreateOperation, EditOperationLinks, GenerateOperation, History, SetStock, SetWorkMaterial,
};
use camcraft_app::cutting_rows;
use camcraft_app::mcp::Server;
use camcraft_app::model;
use camcraft_app::operation::{self, Height, OpParams, SrcRef};
use camcraft_app::posting;
use camcraft_app::project;
use camcraft_app::stock::StockShape;
use camcraft_app::tool_cutting_rows::CuttingRow;
use camcraft_app::tool_library::{export_tool_library, parse_tool_library};
use camcraft_app::tools::{Tool, WorkMaterial, catalog};
use camcraft_app::world::new_world;
use flecs_ecs::prelude::World;
use serde_json::{Value, json};

const STEEL: CuttingRow = CuttingRow { material: WorkMaterial::MildSteel, rpm: 3100.0, feed: 410.0 };
const BRASS: CuttingRow = CuttingRow { material: WorkMaterial::Brass, rpm: 8200.0, feed: 950.0 };

fn base() -> Tool {
    catalog().into_iter().find(|(n, _)| *n == "EM-0600-4FL").map(|(_, t)| t).expect("catalog entry")
}

fn tabled() -> Tool {
    Tool { cutting: vec![STEEL, BRASS], ..base() }
}

fn read(w: &World, n: &str) -> Tool {
    model::find_by_name(w, n).and_then(|e| model::tool_of(w, &e)).expect("tool")
}

fn op_feed(w: &World, n: &str) -> f64 {
    let mut p = model::find_by_name(w, n).and_then(|e| model::operation_of(w, &e)).expect("op").params;
    *camcraft_app::cutting::feed_mut(&mut p).expect("milling feed")
}

fn tmp(name: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!("cc890-{}-{name}", std::process::id()))
}

fn slot(feed: f64) -> OpParams {
    OpParams::Slot {
        links: camcraft_app::links::Links::PLANAR,
        a: camcraft_app::geo2d::Point2::new(0.0, 10.0),
        b: camcraft_app::geo2d::Point2::new(40.0, 10.0),
        width: 12.0,
        stock_to_leave: 0.0,
        clearance_z: Height::fixed(30.0),
        retract_z: None,
        top_z: Height::fixed(20.0),
        bottom_z: Height::fixed(14.0),
        stepdown: 3.0,
        stepover: 4.0,
        feed,
        plunge_feed: 200.0,
        climb: true,
        entry: camcraft_app::machining3d::Entry3D::Plunge,
        helix_radius: 1.0,
        tilt: None,
        extend_start: 0.0,
        extend_end: 0.0,
    }
}

/// A stocked document with tool `T` (the table) and a slot on it at 800.
fn doc(tool: Tool) -> (World, History) {
    let w = new_world();
    let mut h = History::default();
    h.execute(&w, AddTool::new(&w, "T", tool).expect("tool"));
    h.execute(&w, SetStock::new(&w, StockShape::Box { dx: 100.0, dy: 100.0, dz: 20.0 }, Default::default()).expect("stock"));
    h.execute(&w, CreateOperation::new(&w, "Slot1", "T", SrcRef::Stock, slot(800.0)).expect("op"));
    (w, h)
}

#[test]
fn a_tool_without_rows_writes_no_key_and_a_table_round_trips() {
    let plain = project::tool_to_json(&base()).to_string();
    assert!(!plain.contains("cutting_data"), "{plain}");
    let v: Value = serde_json::from_str(&plain).expect("json");
    assert_eq!(project::tool_from_json(&v).expect("old shape loads"), base());
    let on = project::tool_to_json(&tabled());
    assert_eq!(on["cutting_data"][0], json!({ "material": "mild_steel", "rpm": 3100.0, "feed": 410.0 }));
    assert_eq!(project::tool_from_json(&on).expect("loads").cutting, vec![STEEL, BRASS]);
}

#[test]
fn a_project_without_rows_saves_byte_identical_and_a_table_survives_save_load() {
    let (w, _h) = doc(base());
    let p1 = tmp("a.camproj");
    project::save(&w, &p1).expect("save");
    let w2 = new_world();
    project::load(&w2, &p1).expect("load");
    let p2 = tmp("b.camproj");
    project::save(&w2, &p2).expect("save again");
    let (a, b) = (std::fs::read(&p1).expect("a"), std::fs::read(&p2).expect("b"));
    assert!(!String::from_utf8_lossy(&a).contains("cutting_data"));
    assert_eq!(a, b, "a tool with one rpm/feed saves byte for byte");

    let (w, _h) = doc(tabled());
    project::save(&w, &p1).expect("save");
    let w3 = new_world();
    project::load(&w3, &p1).expect("load");
    let _ = (std::fs::remove_file(&p1), std::fs::remove_file(&p2));
    assert_eq!(read(&w3, "T").cutting, vec![STEEL, BRASS]);
}

#[test]
fn loading_never_moves_the_numbers_an_existing_project_runs() {
    // A document whose material has a row, but whose tool and op run
    // other numbers (hand-set): the load keeps them exactly.
    let (w, mut h) = doc(tabled());
    h.execute(&w, SetWorkMaterial::new(&w, Some(WorkMaterial::MildSteel)).expect("material"));
    h.execute(&w, ChangeTool::new(&w, "T", Tool { rpm: 5000.0, ..read(&w, "T") }).expect("hand rpm"));
    let mut p = model::find_by_name(&w, "Slot1").and_then(|e| model::operation_of(&w, &e)).expect("op").params;
    *camcraft_app::cutting::feed_mut(&mut p).expect("feed") = 777.0;
    h.execute(&w, camcraft_app::commands::EditOperationParams::new(&w, "Slot1", p).expect("hand feed"));
    let path = tmp("load.camproj");
    project::save(&w, &path).expect("save");
    let w2 = new_world();
    project::load(&w2, &path).expect("load");
    let _ = std::fs::remove_file(&path);
    assert_eq!(read(&w2, "T").rpm, 5000.0);
    assert_eq!(op_feed(&w2, "Slot1"), 777.0);
}

#[test]
fn an_unknown_material_word_refuses_the_file_by_name() {
    let mut v = project::tool_to_json(&tabled());
    v["cutting_data"][0]["material"] = json!("unobtainium");
    let e = project::tool_from_json(&v).expect_err("refused");
    assert!(e.contains("unobtainium"), "{e}");
}

#[test]
fn the_table_round_trips_the_tool_library() {
    let (w, _h) = doc(tabled());
    let back = parse_tool_library(&export_tool_library(&w).expect("export")).expect("parse");
    assert_eq!(back[0].tool.cutting, vec![STEEL, BRASS]);
}

#[test]
fn unusable_rows_are_refused_by_tool_name_at_creation() {
    let (w, _h) = doc(base());
    let bad = [
        vec![STEEL, STEEL],
        vec![CuttingRow { rpm: f64::NAN, ..STEEL }],
        vec![CuttingRow { rpm: 0.0, ..STEEL }],
        vec![CuttingRow { feed: -1.0, ..STEEL }],
        vec![CuttingRow { feed: f64::INFINITY, ..STEEL }],
    ];
    for rows in bad {
        let t = Tool { cutting: rows, ..base() };
        for e in [
            AddTool::new(&w, "T2", t.clone()).err().expect("add refused").to_string(),
            ChangeTool::new(&w, "T", t).err().expect("change refused").to_string(),
        ] {
            assert!(e.contains("tool 'T") && e.contains("Mild steel"), "{e}");
        }
    }
    assert!(read(&w, "T").cutting.is_empty());
}

#[test]
fn stating_the_material_seeds_tool_and_ops_in_one_undo_step() {
    let (w, mut h) = doc(tabled());
    let before = (read(&w, "T"), op_feed(&w, "Slot1"));
    let cmd = SetWorkMaterial::new(&w, Some(WorkMaterial::MildSteel)).expect("material");
    assert_eq!(cmd.seeded(), (1, 1));
    h.execute(&w, cmd);
    assert_eq!((read(&w, "T").rpm, read(&w, "T").feed, op_feed(&w, "Slot1")), (3100.0, 410.0, 410.0));
    // Another material with a row moves them again; one without leaves them.
    h.execute(&w, SetWorkMaterial::new(&w, Some(WorkMaterial::Brass)).expect("brass"));
    assert_eq!((read(&w, "T").rpm, op_feed(&w, "Slot1")), (8200.0, 950.0));
    let none = SetWorkMaterial::new(&w, Some(WorkMaterial::Plastics)).expect("plastics");
    assert_eq!(none.seeded(), (0, 0), "no row: today's numbers stay");
    h.execute(&w, none);
    assert_eq!((read(&w, "T").rpm, op_feed(&w, "Slot1")), (8200.0, 950.0));
    assert!(h.undo(&w) && h.undo(&w) && h.undo(&w));
    assert_eq!((read(&w, "T"), op_feed(&w, "Slot1")), before);
}

#[test]
fn an_op_on_material_x_posts_x_rows_spindle_speed() {
    let (w, mut h) = doc(tabled());
    h.execute(&w, SetWorkMaterial::new(&w, Some(WorkMaterial::MildSteel)).expect("material"));
    h.execute(&w, GenerateOperation::new(&w, "Slot1").expect("generate"));
    let path = tmp("steel.nc");
    posting::post_setup(&w, &path).expect("posts");
    let nc = std::fs::read_to_string(&path).expect("nc");
    let _ = std::fs::remove_file(&path);
    assert!(nc.contains("S3100"), "the steel row's rpm is posted:\n{nc}");
    assert!(nc.contains("F410"), "the steel row's feed is posted:\n{nc}");
}

#[test]
fn adding_a_tool_and_editing_its_table_seed_the_running_numbers() {
    let w = new_world();
    let mut h = History::default();
    h.execute(&w, SetWorkMaterial::new(&w, Some(WorkMaterial::Brass)).expect("material"));
    h.execute(&w, AddTool::new(&w, "T", tabled()).expect("add"));
    assert_eq!((read(&w, "T").rpm, read(&w, "T").feed), (8200.0, 950.0));
    h.execute(&w, AddTool::new(&w, "P", base()).expect("plain"));
    assert_eq!(read(&w, "P").rpm, base().rpm, "no table: the tool's own numbers");
    // Adding a brass row to the plain tool loads it; a hand rpm edit
    // in the same Apply keeps the hand number.
    h.execute(&w, ChangeTool::new(&w, "P", Tool { cutting: vec![BRASS], ..base() }).expect("row"));
    assert_eq!(read(&w, "P").rpm, 8200.0);
    assert!(h.undo(&w));
    assert_eq!(read(&w, "P").rpm, base().rpm);
    h.execute(&w, ChangeTool::new(&w, "P", Tool { cutting: vec![BRASS], rpm: 7000.0, ..base() }).expect("both"));
    assert_eq!(read(&w, "P").rpm, 7000.0);
}

#[test]
fn quick_add_and_retarget_seed_the_op_feed_from_the_row() {
    let (w, mut h) = doc(base());
    h.execute(&w, AddTool::new(&w, "T2", tabled()).expect("tabled"));
    h.execute(&w, SetWorkMaterial::new(&w, Some(WorkMaterial::MildSteel)).expect("material"));
    assert_eq!(op_feed(&w, "Slot1"), 800.0, "an op on a tool without rows keeps its feed");
    h.execute(&w, EditOperationLinks::new(&w, "Slot1", Some("T2"), None).expect("retarget"));
    assert_eq!(op_feed(&w, "Slot1"), 410.0);
    assert!(h.undo(&w));
    assert_eq!(op_feed(&w, "Slot1"), 800.0);
    let mut params = slot(800.0);
    cutting_rows::seed_feed_for_tool(&w, &mut params, &read(&w, "T2"));
    assert_eq!(*camcraft_app::cutting::feed_mut(&mut params).expect("feed"), 410.0);
    let mut drill = operation::seed_params(operation::KindInfo::ALL.iter().position(|k| k.label == "Drill").expect("drill row"));
    let before = drill.clone();
    cutting_rows::seed_feed_for_tool(&w, &mut drill, &read(&w, "T2"));
    assert_eq!(drill, before, "a kind with no milling feed is left alone");
}

#[test]
fn mcp_add_tool_takes_the_table_and_refuses_a_bad_row() {
    let mut server = Server::new();
    let mut id = 0;
    let mut call = |name: &str, args: Value| {
        id += 1;
        let msg = json!({ "jsonrpc": "2.0", "id": id, "method": "tools/call", "params": { "name": name, "arguments": args } });
        let reply = server.handle(&msg).expect("a response");
        let r = reply["result"].clone();
        if r["isError"] == json!(true) { Err(r["content"][0]["text"].as_str().unwrap_or_default().to_owned()) } else { Ok(r["structuredContent"].clone()) }
    };
    let mut def = project::tool_to_json(&tabled());
    call("add_tool", json!({ "name": "T", "definition": def, "holder": "none" })).expect("add");
    let job = call("get_job", json!({})).expect("job").to_string();
    assert!(job.contains("mild_steel") && job.contains("3100"), "{job}");
    def["cutting_data"][1]["material"] = json!("mild_steel");
    let e = call("add_tool", json!({ "name": "T3", "definition": def, "holder": "none" })).expect_err("twice");
    assert!(e.contains("T3") && e.contains("twice"), "{e}");
}
