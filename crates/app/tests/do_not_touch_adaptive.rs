//! #3346: "Do not touch" on 2D Adaptive operations, through MCP and the post.
//! Gate map (M4.4): covers app::operation, app::operation::generate, app::mcp,
//! motion::machining2d::adaptive_avoid.

mod common;
mod do_not_touch_support;

use camcraft_app::world::new_world;
use camcraft_app::{model, project};
use do_not_touch_support::{Client, setup_plate_and_tower_on};
use serde_json::json;
use std::path::Path;

fn add_adaptive(c: &mut Client, plate_top: &str, method: &str, load: Option<f64>) -> String {
    let mut params = json!({
        "method": method,
        "stepdown": 14.0,
        "stock_to_leave": 0.5,
        "top_z": { "reference": "fixed", "offset": 24.0 },
        "bottom_z": { "reference": "fixed", "offset": 10.0 },
        "clearance_z": { "reference": "fixed", "offset": 35.0 },
        "retract_z": { "reference": "fixed", "offset": 26.0 }
    });
    if let Some(l) = load {
        params["optimal_load"] = json!(l);
    }
    let added = c.ok("add_operation", json!({ "kind": "2D Adaptive", "tool": "SteppedMill", "params": params }));
    let name = added["operation"].as_str().expect("op name").to_owned();
    c.ok("set_operation_geometry", json!({ "operation": name, "picks": [{ "model": "plate", "id": plate_top }] }));
    name
}

/// The cached toolpath of `op`, read back from a saved project.
fn saved_path(c: &mut Client, dir: &Path, tag: &str, op: &str) -> String {
    let file = dir.join(format!("{tag}.camproj"));
    c.ok("save_project", json!({ "path": file.display().to_string() }));
    let w = new_world();
    project::load(&w, &file).expect("load saved project");
    let e = model::find_by_name(&w, op).expect("op entity");
    let cache = model::toolpath_of(&w, &e).expect("cached toolpath");
    format!("{:?}", cache.path.moves)
}

/// Protect the tower beside a spiral 2D Adaptive on `plate` at `load` (a
/// fraction of D; `None` is the default load): the path verifies clean and
/// posts. Returns the op and the plain path.
fn protected_spiral(tag: &str, plate: [f64; 4], load: Option<f64>) -> (std::path::PathBuf, Client, String, String) {
    let (dir, mut c, plate_top, tower_top_face, tower_edge, _vert) = setup_plate_and_tower_on(tag, plate);
    let op = add_adaptive(&mut c, &plate_top, "spiral", load);

    // Without protection the spiral cuts through the tower at Z 10.
    c.ok("generate", json!({ "name": op }));
    let plain = saved_path(&mut c, &dir, "plain", &op);
    let base_err = c.refused("post", json!({ "output": dir.join("plain.nc").display().to_string(), "post": "Fanuc" }));
    assert!(base_err.contains("gouge"), "the unprotected path must cut into the tower: {base_err}");

    // 1.5 mm clearance plus 0.5 mm stock to leave: 2.0 mm round the tower.
    c.ok("set_operation_do_not_touch", json!({
        "operation": op,
        "clearance": 1.5,
        "picks": [{ "model": "tower", "id": tower_top_face }, { "model": "tower", "id": tower_edge }]
    }));
    let generated = c.ok("generate", json!({ "name": op }));
    assert_eq!(generated["generated"][0]["status"], json!("ok"), "{generated}");
    let ver = c.ok("verify", json!({}));
    assert_eq!(ver["ok"], json!(true), "{ver}");
    assert!(
        !ver["findings"].as_array().is_some_and(|fs| fs.iter().any(|f| f["kind"] == json!("avoid_hit"))),
        "no avoid_hit finding: {ver}"
    );
    let posted = c.ok("post", json!({ "output": dir.join("protected.nc").display().to_string(), "post": "Fanuc" }));
    assert!(posted["lines"].as_u64().is_some_and(|n| n > 0), "{posted}");
    assert_ne!(saved_path(&mut c, &dir, "protected", &op), plain, "the protection must change the path");
    (dir, c, op, plain)
}

#[test]
fn adaptive2d_do_not_touch_keeps_the_full_tool_clear_and_clears_back_to_the_plain_path() {
    let (dir, mut c, op, plain) = protected_spiral("adaptive2d", [-2.0, -16.0, 30.0, 16.0], Some(0.4));
    // Undo the protected generate, then the protection: generated again,
    // the path is the plain one, move for move.
    c.ok("undo", json!({}));
    c.ok("undo", json!({}));
    let dnt = c.ok("get_operation_do_not_touch", json!({ "operation": op }));
    assert_eq!(dnt["picks"].as_array().map(Vec::len), Some(0), "{dnt}");
    c.ok("generate", json!({ "name": op }));
    assert_eq!(saved_path(&mut c, &dir, "cleared", &op), plain);
    let _ = std::fs::remove_dir_all(&dir);
}

/// On the 80 x 80 mm plate the keep-out leaves a ring channel round the
/// tower. The adaptive plan falls back to Offset rings there; the path
/// must still verify clean (no avoid hit, no crossing) and post.
#[test]
#[ignore = "heavy lane (#3346 do not touch): the default-load spiral in the ring round the tower exceeds its 60 s debug budget - run with --release -- --ignored"]
fn adaptive2d_do_not_touch_round_the_tower_at_the_default_load_posts() {
    let (dir, _c, _op, _plain) = protected_spiral("adaptive2d_default", [-40.0, -40.0, 40.0, 40.0], None);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn adaptive2d_with_no_room_clear_of_the_protection_is_refused_with_the_item_named() {
    let (dir, mut c, plate_top, tower_top_face, _edge, _vert) = setup_plate_and_tower_on("adaptive2d_full", [-20.0, -20.0, 20.0, 20.0]);
    for method in ["spiral", "trochoidal"] {
        let op = add_adaptive(&mut c, &plate_top, method, Some(0.4));
        // 40 mm round the tower covers the whole 40 x 40 mm plate.
        c.ok("set_operation_do_not_touch", json!({
            "operation": op,
            "clearance": 40.0,
            "picks": [{ "model": "tower", "id": tower_top_face }]
        }));
        let refused = c.refused("generate", json!({ "name": op }));
        assert!(refused.contains("tower") && refused.contains("Do not touch"), "{method}: {refused}");
        c.ok("delete_operation", json!({ "name": op }));
    }
    let _ = std::fs::remove_dir_all(&dir);
}
