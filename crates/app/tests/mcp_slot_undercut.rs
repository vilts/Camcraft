//! #1444: the Slot Undercut operation over the MCP tools, end to end on
//! a T-slot block - an end mill cuts the neck slot, a T-slot cutter
//! enters from outside the wall, cuts the undercut and leaves the way
//! it came; the program verifies and posts. The refusals: a cutter that
//! is no undercut cutter, an entry slot too narrow for the neck (by
//! name, at generation), no slot cut for the neck (the neck collides),
//! and an entry station under the stock (a Z move with the head under
//! material), each holding the post.
//! Provenance: our own fixture - a grid-aligned T-slot block written as
//! STL by this test.
//!
//! Gate map: covers app::operation::slot_undercut_op, the SlotUndercut
//! arms of app::operation::{generate, kinds, heights, keep_law, kept_rejudge},
//! app::project (undercut_dto, the undercut tool kind words),
//! app::stock_sim (the undercut plan) and app::mcp (add_operation,
//! edit_operation, verify findings).

mod undercut_support;

use serde_json::{json, Value};
use std::path::Path;
use undercut_support::{tmp_dir, write_box_stl, write_tslot_stl, Client};

/// The T-slot block in a stock of its own size, a 6 mm end mill and a
/// 20 x 6 mm T-slot cutter on an 8 mm neck (`TS-20`).
fn job(dir: &Path) -> Client {
    let stl = dir.join("tslot.stl");
    write_tslot_stl(&stl);
    let mut c = Client::new();
    let imp = c.ok("import_model", json!({ "path": stl.display().to_string() }));
    assert_eq!(imp["model"], json!("tslot"));
    c.ok("set_stock", json!({ "box": { "dx": 60.0, "dy": 40.0, "dz": 20.0 } }));
    c.ok("add_tool", json!({ "catalog": "EM-0600-4FL" }));
    let job = c.ok("get_job", json!({}));
    let mut def = job["tools"][0]["definition"].clone();
    def["kind"] = json!("t_slot");
    def["diameter"] = json!(20.0);
    def["flute_len"] = json!(6.0);
    def["shank_d"] = json!(8.0);
    def["shank_len"] = json!(30.0);
    def["rpm"] = json!(1500.0);
    def["feed"] = json!(120.0);
    if let Some(o) = def.as_object_mut() {
        o.remove("cutting_data");
    }
    c.ok("add_tool", json!({ "name": "TS-20", "definition": def, "holder": "none" }));
    c
}

/// The neck slot: 9 mm wide along X, from the top down to Z 14.
fn add_neck_slot(c: &mut Client) {
    c.ok("add_operation", json!({
        "kind": "Slot", "tool": "EM-0600-4FL", "source": "tslot", "name": "Neck",
        "params": {
            "a": [-30.0, 0.0], "b": [30.0, 0.0], "width": 9.0,
            "clearance_z": 30.0, "top_z": 20.0, "bottom_z": 14.0, "stepdown": 3.0, "stepover": 2.0
        }
    }));
}

/// The undercut: 20 mm wide on the floor Z 8, entered from 15 mm
/// outside the x -30 wall, with `patch` merged over it.
fn add_undercut(c: &mut Client, patch: Value) -> Value {
    let mut params = json!({
        "a": [-30.0, 0.0], "b": [30.0, 0.0], "width": 20.0, "run_in": 15.0, "neck_slot": 9.0,
        "clearance_z": 30.0, "retract_z": 25.0, "top_z": 20.0, "bottom_z": 8.0, "stepover": 2.0, "feed": 120.0, "plunge_feed": 60.0
    });
    if let (Some(p), Some(extra)) = (params.as_object_mut(), patch.as_object()) {
        for (k, v) in extra {
            p.insert(k.clone(), v.clone());
        }
    }
    c.ok("add_operation", json!({ "kind": "Slot Undercut", "tool": "TS-20", "source": "tslot", "name": "Undercut", "params": params }))
}

fn kinds_of(findings: &Value, op: &str) -> Vec<String> {
    findings["findings"]
        .as_array()
        .map(|f| f.iter().filter(|f| f["op"] == json!(op)).filter_map(|f| f["kind"].as_str().map(str::to_owned)).collect())
        .unwrap_or_default()
}

#[test]
fn a_t_slot_is_cut_verified_and_posted_over_the_tools() {
    let dir = tmp_dir("job");
    let mut c = job(&dir);
    let kinds = c.ok("list_operation_types", json!({}));
    let kind = kinds["kinds"].as_array().and_then(|k| k.iter().find(|k| k["label"] == json!("Slot Undercut"))).cloned();
    let kind = kind.expect("the Slot Undercut kind is listed");
    assert_eq!(kind["key"], json!("slot_undercut"));

    add_neck_slot(&mut c);
    let add = add_undercut(&mut c, json!({}));
    assert_eq!(add["operation"], json!("Undercut"));
    let job = c.ok("get_job", json!({}));
    let tool = job["tools"].as_array().and_then(|t| t.iter().find(|t| t["name"] == json!("TS-20"))).cloned().expect("TS-20");
    assert_eq!(tool["definition"]["kind"], json!("t_slot"));
    let op = job["operations"].as_array().and_then(|o| o.iter().find(|o| o["key"] == json!("Undercut"))).cloned().expect("Undercut");
    assert_eq!(op["params"]["slot_undercut"]["width"], json!(20.0));
    assert_eq!(op["params"]["slot_undercut"]["neck_slot"], json!(9.0));

    let generated = c.ok("generate", json!({}));
    let status: Vec<&Value> = generated["generated"].as_array().map(|g| g.iter().map(|g| &g["status"]).collect()).unwrap_or_default();
    assert_eq!(status, vec![&json!("ok"), &json!("ok")], "{generated}");
    let ver = c.ok("verify", json!({}));
    assert_eq!(ver["ok"], json!(true), "{ver}");
    assert!(kinds_of(&ver, "Undercut").is_empty(), "{ver}");

    let nc = dir.join("tslot.nc");
    let posted = c.ok("post", json!({ "output": nc.display().to_string(), "post": "Fanuc" }));
    assert!(posted["lines"].as_u64().is_some_and(|n| n > 0));
    let text = std::fs::read_to_string(&nc).expect("the program was written");
    // The cutter goes down at the station x -45, outside the wall, and
    // every block at the floor Z 8 follows it there.
    assert!(text.contains("X-45."), "the entry station is posted:\n{text}");
    assert!(text.contains("Z8."), "the undercut floor is posted:\n{text}");

    // Saved and opened again, the op and the cutter are what they were.
    let camproj = dir.join("tslot.camproj");
    c.ok("save_project", json!({ "path": camproj.display().to_string() }));
    c.ok("open_project", json!({ "path": camproj.display().to_string() }));
    let opened = c.ok("get_job", json!({}));
    let tool = opened["tools"].as_array().and_then(|t| t.iter().find(|t| t["name"] == json!("TS-20"))).cloned().expect("TS-20");
    assert_eq!(tool["definition"]["kind"], json!("t_slot"));
    let op = opened["operations"].as_array().and_then(|o| o.iter().find(|o| o["key"] == json!("Undercut"))).cloned().expect("Undercut");
    assert_eq!(op["params"]["slot_undercut"]["run_in"], json!(15.0));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn an_end_mill_and_a_narrow_entry_slot_are_refused_by_name() {
    let dir = tmp_dir("refuse");
    let mut c = job(&dir);
    let msg = c.refused("add_operation", json!({ "kind": "Slot Undercut", "tool": "EM-0600-4FL", "source": "tslot" }));
    assert!(msg.contains("Slot Undercut"), "{msg}");
    add_neck_slot(&mut c);
    add_undercut(&mut c, json!({ "neck_slot": 7.0 }));
    let msg = c.refused("generate", json!({ "name": "Undercut" }));
    assert!(msg.contains("entry slot 7 mm is too narrow for the 8 mm cutter neck"), "{msg}");
    // An undercut narrower than the head, the same way.
    c.ok("edit_operation", json!({ "name": "Undercut", "params": { "neck_slot": 9.0, "width": 18.0 } }));
    let msg = c.refused("generate", json!({ "name": "Undercut" }));
    assert!(msg.contains("narrower than the 20 mm cutter head"), "{msg}");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_neck_with_no_slot_cut_for_it_holds_the_post() {
    let dir = tmp_dir("neck");
    let mut c = job(&dir);
    // No neck slot operation: the raw stock stands over the neck's path.
    add_undercut(&mut c, json!({}));
    c.call("generate", json!({}));
    let ver = c.ok("verify", json!({}));
    assert_eq!(ver["ok"], json!(false), "{ver}");
    assert!(kinds_of(&ver, "Undercut").contains(&"shank_hit".to_owned()), "{ver}");
    let msg = c.refused("post", json!({ "output": dir.join("x.nc").display().to_string(), "post": "Fanuc" }));
    assert!(!msg.is_empty());
    assert!(!dir.join("x.nc").exists(), "nothing is posted");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_z_move_with_the_head_under_the_stock_holds_the_post() {
    let dir = tmp_dir("lift");
    let mut c = job(&dir);
    add_neck_slot(&mut c);
    // 5 mm of run-in: the station x -35 leaves half the head over the block.
    add_undercut(&mut c, json!({ "run_in": 5.0 }));
    c.call("generate", json!({}));
    let ver = c.ok("verify", json!({}));
    assert_eq!(ver["ok"], json!(false), "{ver}");
    let kinds = kinds_of(&ver, "Undercut");
    assert!(kinds.contains(&"undercut_lift".to_owned()), "{ver}");
    let lift = ver["findings"].as_array().and_then(|f| f.iter().find(|f| f["kind"] == json!("undercut_lift"))).cloned().expect("lift");
    assert_eq!(lift["blocks_post"], json!(true));
    assert!(lift["message"].as_str().is_some_and(|m| m.contains("moves the undercut cutter in Z")), "{lift}");
    c.refused("post", json!({ "output": dir.join("x.nc").display().to_string(), "post": "Fanuc" }));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn an_undercut_below_the_parts_floor_is_a_gouge_that_holds_the_post() {
    let dir = tmp_dir("gouge");
    let mut c = job(&dir);
    add_neck_slot(&mut c);
    // The part's undercut floor is Z 8: half a mm deeper cuts the part,
    // and takes the head's top out from under its roof.
    add_undercut(&mut c, json!({ "bottom_z": 7.5 }));
    c.call("generate", json!({}));
    let ver = c.ok("verify", json!({}));
    assert_eq!(ver["ok"], json!(false), "{ver}");
    assert!(kinds_of(&ver, "Undercut").contains(&"gouge".to_owned()), "{ver}");
    c.refused("post", json!({ "output": dir.join("x.nc").display().to_string(), "post": "Fanuc" }));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_part_source_undercut_keeps_neighbouring_parts() {
    let dir = tmp_dir("neighbour");
    let mut c = job(&dir);
    let box_stl = dir.join("neighbour.stl");
    write_box_stl(&box_stl, [40.0, -5.0, 0.0], [50.0, 5.0, 20.0]);
    c.ok("import_model", json!({ "path": box_stl.display().to_string(), "name": "neighbour" }));

    add_neck_slot(&mut c);
    add_undercut(&mut c, json!({}));
    c.ok("generate", json!({}));
    let ver = c.ok("verify", json!({}));
    assert_eq!(ver["ok"], json!(true), "{ver}");
    c.ok("post", json!({ "output": dir.join("clear.nc").display().to_string(), "post": "Fanuc" }));

    // Move the neighbour part into the undercut entry station (x -50..-40) without regenerating:
    // kept_rejudge must catch the moved neighbour at post time.
    c.ok("move_model", json!({ "name": "neighbour", "position": [-90.0, 0.0, 0.0] }));
    let blocked = c.refused("post", json!({ "output": dir.join("moved.nc").display().to_string(), "post": "Fanuc" }));
    assert!(blocked.contains("neighbour"), "{blocked}");

    // Regenerating after the move must also flag the neighbour gouge on the operation itself.
    c.ok("edit_operation", json!({ "name": "Undercut", "params": { "stepover": 1.9 } }));
    c.ok("generate", json!({ "name": "Undercut" }));
    let ver = c.ok("verify", json!({}));
    assert_eq!(ver["ok"], json!(false), "{ver}");
    assert!(kinds_of(&ver, "Undercut").contains(&"gouge".to_owned()), "{ver}");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn list_operation_types_and_edit_operation_keep_zero_bulge_and_neck_slot() {
    let dir = tmp_dir("zero-fields");
    let mut c = job(&dir);

    let kinds = c.ok("list_operation_types", json!({}));
    let kind = kinds["kinds"]
        .as_array()
        .and_then(|k| k.iter().find(|k| k["key"] == json!("slot_undercut")))
        .expect("slot_undercut kind");
    let fields = kind["fields"].as_array().expect("slot_undercut fields");
    assert!(fields.iter().any(|f| f["name"] == json!("bulge") && f["default"] == json!(0.0)), "{fields:?}");
    assert!(fields.iter().any(|f| f["name"] == json!("neck_slot") && f["default"] == json!(0.0)), "{fields:?}");

    add_undercut(&mut c, json!({ "bulge": 0.25, "neck_slot": 9.0 }));
    c.ok("edit_operation", json!({ "name": "Undercut", "params": { "bulge": 0.0, "neck_slot": 0.0 } }));
    let job = c.ok("get_job", json!({}));
    let op = job["operations"]
        .as_array()
        .and_then(|o| o.iter().find(|o| o["key"] == json!("Undercut")))
        .expect("Undercut");
    assert_eq!(op["params"]["slot_undercut"]["bulge"], json!(0.0));
    assert_eq!(op["params"]["slot_undercut"]["neck_slot"], json!(0.0));
    let _ = std::fs::remove_dir_all(&dir);
}
