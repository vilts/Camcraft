//! #2015: a tool's cutting data by work material - the `Unknown` default
//! row, a row's stepdown and stepover, and the material of a setup's
//! stock. Pins: a new tool gets the row; a v62 project and a v1/v2
//! `.camtools` file load with no loss, their own rpm/feed read as the
//! `Unknown` row; the row is picked by the stock's material, then the
//! document's, then `Unknown`; steps seed an operation only inside an
//! undoable command, never on load; a project with no new rows posts the
//! very S and F words it posted before.
//! Provenance: our own tests over our own command model.
//!
//! Gate map (#2015): covers app::cutting_rows, app::stock_material, app::commands (tool, cutting_data, stock_material), app::project (ToolDto, EntityDto), app::tool_library, app::mcp.

use camcraft_app::commands::{
    AddTool, ChangeTool, CreateOperation, CreateSetup, GenerateOperation, History, SetStock, SetStockMaterial,
    SetWorkMaterial,
};
use camcraft_app::cutting_rows;
use camcraft_app::mcp::Server;
use camcraft_app::model;
use camcraft_app::operation::{Height, OpParams, SrcRef};
use camcraft_app::posting;
use camcraft_app::project;
use camcraft_app::stock::StockShape;
use camcraft_app::stock_material;
use camcraft_app::tool_cutting_rows::{CuttingRow, unknown_of};
use camcraft_app::tool_library::{CUTTING_STEPS_VERSION, export_tool_library, parse_tool_library};
use camcraft_app::tools::{Tool, WorkMaterial, catalog};
use camcraft_app::world::new_world;
use flecs_ecs::prelude::World;
use serde_json::{Value, json};

const STEEL: CuttingRow = CuttingRow {
    material: Some(WorkMaterial::MildSteel),
    rpm: 3100.0,
    feed: 410.0,
    stepdown: Some(1.5),
    stepover: None,
    waterjet: None,
    plasma: None,
};
const BRASS: CuttingRow = CuttingRow::plain(Some(WorkMaterial::Brass), 8200.0, 950.0);

fn base() -> Tool {
    catalog().into_iter().find(|(n, _)| *n == "EM-0600-4FL").map(|(_, t)| t).expect("catalog entry")
}

/// The base tool with an `Unknown` row stating steps, and two material rows.
fn tabled() -> Tool {
    let b = base();
    let unknown = CuttingRow { stepdown: Some(2.5), stepover: Some(2.0), ..CuttingRow::plain(None, b.rpm, b.feed) };
    Tool { cutting: vec![unknown, STEEL, BRASS], ..b }
}

fn read(w: &World, n: &str) -> Tool {
    model::find_by_name(w, n).and_then(|e| model::tool_of(w, &e)).expect("tool")
}

fn op(w: &World, n: &str) -> OpParams {
    model::find_by_name(w, n).and_then(|e| model::operation_of(w, &e)).expect("op").params
}

/// (feed, stepdown, stepover) of a pocket op.
fn numbers(w: &World, n: &str) -> (f64, f64, f64) {
    match op(w, n) {
        OpParams::Pocket { feed, stepdown, stepover, .. } => (feed, stepdown, stepover),
        other => panic!("not a pocket: {other:?}"),
    }
}

fn tmp(name: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!("cc2015-{}-{name}", std::process::id()))
}

fn pocket() -> OpParams {
    let mut p = camcraft_app::operation::seed_params(
        camcraft_app::operation::KindInfo::ALL.iter().position(|k| k.label == "2D Pocket").expect("pocket row"),
    );
    if let OpParams::Pocket { top_z, bottom_z, stepdown, stepover, feed, .. } = &mut p {
        *top_z = Height::fixed(20.0);
        *bottom_z = Height::fixed(14.0);
        *stepdown = 3.0;
        *stepover = 4.0;
        *feed = 800.0;
    }
    p
}

/// A stocked document with tool `T` and a pocket on it.
fn doc(tool: Tool) -> (World, History) {
    let w = new_world();
    let mut h = History::default();
    h.execute(&w, AddTool::new(&w, "T", tool).expect("tool"));
    h.execute(&w, SetStock::new(&w, StockShape::Box { dx: 100.0, dy: 100.0, dz: 20.0 }, Default::default()).expect("stock"));
    h.execute(&w, CreateOperation::new(&w, "Pocket1", "T", SrcRef::Stock, pocket()).expect("op"));
    (w, h)
}

/// Save, rewrite the JSON with `edit`, load into a fresh world.
fn reload(w: &World, name: &str, edit: impl FnOnce(&mut Value)) -> World {
    let path = tmp(name);
    project::save(w, &path).expect("save");
    let mut v: Value = serde_json::from_str(&std::fs::read_to_string(&path).expect("read")).expect("json");
    edit(&mut v);
    std::fs::write(&path, v.to_string()).expect("write");
    let w2 = new_world();
    let r = project::load(&w2, &path);
    let _ = std::fs::remove_file(&path);
    r.expect("load");
    w2
}

fn post(w: &World, name: &str) -> String {
    let path = tmp(name);
    posting::post_setup(w, &path).expect("posts");
    let nc = std::fs::read_to_string(&path).expect("nc");
    let _ = std::fs::remove_file(&path);
    nc
}

/// The S and F words of a program, in order.
fn s_and_f(nc: &str) -> Vec<String> {
    nc.split_whitespace()
        .filter(|w| (w.starts_with('S') || w.starts_with('F')) && w[1..].starts_with(|c: char| c.is_ascii_digit() || c == '.'))
        .map(str::to_owned)
        .collect()
}

#[test]
fn a_new_tool_gets_its_unknown_row_from_its_own_numbers() {
    let (w, _h) = doc(base());
    let t = read(&w, "T");
    assert_eq!(t, base(), "derived: the tool is stored as given");
    assert_eq!(unknown_of(&t), Some(CuttingRow::plain(None, base().rpm, base().feed)));
    // A stated Unknown row is stored, and a hand edit of the running
    // numbers while it is in force carries it along.
    let mut h = History::default();
    let stated = CuttingRow { stepdown: Some(2.0), ..CuttingRow::plain(None, 7000.0, 900.0) };
    h.execute(&w, ChangeTool::new(&w, "T", Tool { cutting: vec![stated, BRASS], ..t.clone() }).expect("state it"));
    let t = read(&w, "T");
    assert_eq!((t.rpm, t.feed), (7000.0, 900.0), "the row in force runs");
    h.execute(&w, ChangeTool::new(&w, "T", Tool { rpm: 6500.0, ..t.clone() }).expect("hand rpm"));
    assert_eq!(unknown_of(&read(&w, "T")).map(|r| (r.rpm, r.stepdown)), Some((6500.0, Some(2.0))));
    // An editor hands back the derived row untouched: it stays derived.
    let (w, _h) = doc(base());
    let mut h = History::default();
    let shown = camcraft_app::tool_cutting_rows::rows_with_unknown(&read(&w, "T"));
    h.execute(&w, ChangeTool::new(&w, "T", Tool { rpm: 8000.0, cutting: shown, ..base() }).expect("edit"));
    assert!(read(&w, "T").cutting.is_empty());
    assert_eq!(unknown_of(&read(&w, "T")).map(|r| r.rpm), Some(8000.0));
}

#[test]
fn a_v62_project_loads_its_tool_numbers_as_the_unknown_row_with_no_loss() {
    let (w, _h) = doc(base());
    let before = (read(&w, "T").rpm, read(&w, "T").feed, numbers(&w, "Pocket1"));
    let w2 = reload(&w, "v62.camproj", |v| {
        v["version"] = json!(62);
        for e in v["entities"].as_array_mut().expect("entities") {
            if let Some(t) = e.get_mut("tool") {
                assert!(t.get("cutting_data").is_none(), "a derived Unknown row is not written: {t}");
            }
        }
    });
    let t = read(&w2, "T");
    assert_eq!(unknown_of(&t), Some(CuttingRow::plain(None, base().rpm, base().feed)), "old rpm/feed = Unknown row");
    assert_eq!((t.rpm, t.feed, numbers(&w2, "Pocket1")), before, "nothing moved on load");
    // A v62 table with a material row keeps it, Unknown in front.
    let (w, _h) = doc(Tool { cutting: vec![BRASS], ..base() });
    let w3 = reload(&w, "v62rows.camproj", |v| v["version"] = json!(62));
    let t = read(&w3, "T");
    assert_eq!(camcraft_app::tool_cutting_rows::rows_with_unknown(&t), vec![CuttingRow::plain(None, base().rpm, base().feed), BRASS]);
}

#[test]
fn a_v1_tool_library_loads_with_the_unknown_row_and_steps_write_v3() {
    let mut def = project::tool_to_json(&Tool { cutting: vec![BRASS], ..base() });
    def["cutting_data"] = json!([{ "material": "brass", "rpm": 8200.0, "feed": 950.0 }]);
    let v1 = json!({ "format": "camcraft-tools", "version": 1, "units": "mm", "tools": [{ "name": "T", "definition": def }] });
    let lib = parse_tool_library(&v1.to_string()).expect("v1 reads");
    assert_eq!(camcraft_app::tool_cutting_rows::rows_with_unknown(&lib[0].tool), vec![CuttingRow::plain(None, base().rpm, base().feed), BRASS]);
    assert_eq!((lib[0].tool.rpm, lib[0].tool.feed), (base().rpm, base().feed));
    // Plain rows keep the older number; stated steps need the new reader.
    let (w, _h) = doc(Tool { cutting: vec![BRASS], ..base() });
    let text = export_tool_library(&w).expect("export");
    assert_eq!(serde_json::from_str::<Value>(&text).expect("json")["version"], json!(1));
    let (w, _h) = doc(tabled());
    let text = export_tool_library(&w).expect("export");
    assert_eq!(serde_json::from_str::<Value>(&text).expect("json")["version"], json!(CUTTING_STEPS_VERSION));
    assert_eq!(parse_tool_library(&text).expect("v3 reads")[0].tool.cutting, tabled().cutting);
}

#[test]
fn the_row_is_picked_by_the_stock_then_the_document_then_unknown() {
    let (w, mut h) = doc(tabled());
    // Unknown: no material anywhere - its steps seed a new op.
    let mut p = pocket();
    cutting_rows::seed_feed_for_tool(&w, &mut p, &read(&w, "T"));
    assert!(matches!(p, OpParams::Pocket { feed: 800.0, stepdown: 2.5, stepover: 2.0, .. }), "{p:?}");
    // The document says brass: brass's feed, Unknown's steps (brass states none).
    h.execute(&w, SetWorkMaterial::new(&w, Some(WorkMaterial::Brass)).expect("brass"));
    assert_eq!(numbers(&w, "Pocket1"), (950.0, 2.5, 2.0));
    // The stock says steel: steel wins for this setup - its feed and
    // stepdown, Unknown's stepover.
    let cmd = SetStockMaterial::new(&w, Some(WorkMaterial::MildSteel)).expect("steel stock");
    assert_eq!(cmd.seeded(), 1);
    h.execute(&w, cmd);
    assert_eq!(numbers(&w, "Pocket1"), (410.0, 1.5, 2.0));
    assert_eq!(stock_material::in_force(&w, None), Some(WorkMaterial::MildSteel));
    let mut p = pocket();
    cutting_rows::seed_feed_for_tool(&w, &mut p, &read(&w, "T"));
    assert!(matches!(p, OpParams::Pocket { feed: 410.0, stepdown: 1.5, stepover: 2.0, .. }), "{p:?}");
    // The tool's spindle follows the document's material, not a stock's.
    assert_eq!(read(&w, "T").rpm, 8200.0);
    // A document change leaves a stock-stated setup's ops alone.
    h.execute(&w, SetWorkMaterial::new(&w, Some(WorkMaterial::Plastics)).expect("plastics"));
    assert_eq!(numbers(&w, "Pocket1"), (410.0, 1.5, 2.0));
    assert_eq!(read(&w, "T").rpm, base().rpm, "no plastics row: the Unknown row runs");
    // Undo walks every step back.
    assert!(h.undo(&w) && h.undo(&w));
    assert_eq!(numbers(&w, "Pocket1"), (950.0, 2.5, 2.0));
    assert_eq!(stock_material::in_force(&w, None), Some(WorkMaterial::Brass));
    assert!(h.undo(&w));
    assert_eq!(numbers(&w, "Pocket1"), (800.0, 3.0, 4.0));
    assert!(SetStockMaterial::new(&w, None).is_err(), "already unstated: refused, no no-op step");
}

#[test]
fn a_second_setup_stock_material_seeds_only_its_own_operations() {
    let (w, mut h) = doc(tabled());
    h.execute(&w, CreateSetup::new(&w, None, None).expect("Setup1 (materialized)"));
    h.execute(&w, CreateSetup::new(&w, Some("Setup2"), None).expect("Setup2"));
    assert_eq!(camcraft_app::setup::active(&w).as_deref(), Some("Setup2"));
    h.execute(&w, SetStock::new(&w, StockShape::Box { dx: 100.0, dy: 100.0, dz: 20.0 }, Default::default()).expect("stock 2"));
    h.execute(&w, CreateOperation::new(&w, "Pocket2", "T", SrcRef::Stock, pocket()).expect("op 2"));
    h.execute(&w, SetStockMaterial::new(&w, Some(WorkMaterial::MildSteel)).expect("steel"));
    assert_eq!(numbers(&w, "Pocket2"), (410.0, 1.5, 2.0));
    assert_eq!(numbers(&w, "Pocket1"), (800.0, 3.0, 4.0), "the first setup's op is not this stock's");
}

#[test]
fn steps_seed_only_in_commands_and_the_stock_material_round_trips() {
    let (w, mut h) = doc(tabled());
    h.execute(&w, SetStockMaterial::new(&w, Some(WorkMaterial::MildSteel)).expect("steel"));
    // Hand-set numbers after the seed: loading must keep them.
    let mut p = op(&w, "Pocket1");
    if let OpParams::Pocket { stepdown, stepover, feed, .. } = &mut p {
        (*stepdown, *stepover, *feed) = (4.0, 3.5, 600.0);
    }
    h.execute(&w, camcraft_app::commands::EditOperationParams::new(&w, "Pocket1", p).expect("hand"));
    let w2 = reload(&w, "steps.camproj", |_| {});
    assert_eq!(numbers(&w2, "Pocket1"), (600.0, 4.0, 3.5), "nothing seeds on load");
    assert_eq!(stock_material::in_force(&w2, None), Some(WorkMaterial::MildSteel));
    assert_eq!(read(&w2, "T").cutting, tabled().cutting);
    // An unknown stock material word refuses the file by name.
    let path = tmp("bad.camproj");
    project::save(&w, &path).expect("save");
    let text = std::fs::read_to_string(&path).expect("read").replace("\"stock_material\": \"mild_steel\"", "\"stock_material\": \"unobtainium\"");
    assert!(text.contains("unobtainium"), "the stock states its material: {text}");
    std::fs::write(&path, text).expect("write");
    let e = project::load(&new_world(), &path).expect_err("refused").to_string();
    let _ = std::fs::remove_file(&path);
    assert!(e.contains("unobtainium"), "{e}");
}

#[test]
fn a_project_with_no_new_rows_posts_the_same_s_and_f_words() {
    let (w, mut h) = doc(base());
    h.execute(&w, GenerateOperation::new(&w, "Pocket1").expect("generate"));
    let before = post(&w, "before.nc");
    let w2 = reload(&w, "same.camproj", |v| v["version"] = json!(62));
    let mut h2 = History::default();
    h2.execute(&w2, GenerateOperation::new(&w2, "Pocket1").expect("regenerate"));
    let after = post(&w2, "after.nc");
    let words = s_and_f(&before);
    assert!(words.iter().any(|w| w == &format!("S{}", base().rpm)), "{before}");
    assert!(words.iter().any(|w| w == "F800."  || w == "F800" || w == "F800.0"), "{before}");
    assert_eq!(words, s_and_f(&after), "the posted S and F words do not move");
}

#[test]
fn a_stated_stepover_wider_than_the_cutter_is_refused_by_tool_name() {
    let (w, _h) = doc(base());
    let wide = CuttingRow { stepover: Some(base().diameter + 0.1), ..BRASS };
    let e = ChangeTool::new(&w, "T", Tool { cutting: vec![wide], ..base() }).err().expect("refused").to_string();
    assert!(e.contains("tool 'T'") && e.contains("Brass") && e.contains("wider"), "{e}");
}

#[test]
fn mcp_sets_the_stock_material_as_one_undo_step() {
    let mut server = Server::new();
    let mut id = 0;
    let mut call = |name: &str, args: Value| {
        id += 1;
        let msg = json!({ "jsonrpc": "2.0", "id": id, "method": "tools/call", "params": { "name": name, "arguments": args } });
        let reply = server.handle(&msg).expect("a response");
        let r = reply["result"].clone();
        if r["isError"] == json!(true) { Err(r["content"][0]["text"].as_str().unwrap_or_default().to_owned()) } else { Ok(r["structuredContent"].clone()) }
    };
    let e = call("set_stock_material", json!({ "material": "brass" })).expect_err("no stock");
    assert!(e.contains("set the stock first"), "{e}");
    call("add_tool", json!({ "name": "T", "definition": project::tool_to_json(&tabled()), "holder": "none" })).expect("add");
    call("set_stock", json!({ "box": { "dx": 50.0, "dy": 50.0, "dz": 20.0 } })).expect("stock");
    let r = call("set_stock_material", json!({ "material": "brass" })).expect("set");
    assert_eq!(r["stock_material"], json!("brass"));
    let e = call("set_stock_material", json!({ "material": "unobtainium" })).expect_err("word");
    assert!(e.contains("unobtainium"), "{e}");
    call("undo", json!({})).expect("undo");
    let e = call("set_stock_material", json!({ "material": null })).expect_err("already none after undo");
    assert!(e.contains("already"), "{e}");
}
