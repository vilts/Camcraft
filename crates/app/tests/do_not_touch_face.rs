//! #3343: "Do not touch" on a Face operation - the rows keep the full tool
//! clear of each protected item, an impossible cut is refused by name, and
//! the post is blocked.
//! Gate map (M4.4): covers app::operation, app::commands::operation::do_not_touch,
//! app::mcp, app::posting, motion::machining2d::face, sim::verify::avoid.

mod do_not_touch_support;

use camcraft_app::cl::{Move, MoveKind};
use camcraft_app::world::new_world;
use camcraft_app::{model, project};
use do_not_touch_support::{Client, setup_plate_and_tower};
use serde_json::json;

/// A Face of the plate at Cut Z 20: without protection the rows run through the tower [-6..6].
fn add_face(c: &mut Client) -> String {
    let added = c.ok("add_operation", json!({
        "kind": "Face",
        "tool": "SteppedMill",
        "source": "plate",
        "params": {
            "stepover": 4.0,
            "cut_z": { "reference": "fixed", "offset": 20.0 },
            "clearance_z": { "reference": "fixed", "offset": 35.0 }
        }
    }));
    added["operation"].as_str().expect("op name").to_owned()
}

/// The cached moves of `op`, read back through a saved project.
fn moves(c: &mut Client, dir: &std::path::Path, op: &str) -> Vec<Move> {
    let path = dir.join("read.camproj");
    c.ok("save_project", json!({ "path": path.display().to_string() }));
    let w = new_world();
    project::load(&w, &path).expect("reload saved project");
    let e = model::find_by_name(&w, op).expect("op entity");
    model::toolpath_of(&w, &e).expect("cached toolpath").path.moves
}

#[test]
fn face_do_not_touch_keeps_full_tool_clear_of_protected_face() {
    let (dir, mut c, _plate_top, tower_top_face, _edge, _vert) = setup_plate_and_tower("face_clear");
    let op = add_face(&mut c);

    // The UI and the command accept protection on a Face operation.
    c.ok("set_operation_do_not_touch", json!({
        "operation": op,
        "clearance": 1.5,
        "picks": [{ "model": "tower", "id": tower_top_face }]
    }));
    let generated = c.ok("generate", json!({ "name": op }));
    assert_eq!(generated["generated"][0]["status"], json!("ok"), "{generated}");
    let ver = c.ok("verify", json!({ "name": op }));
    assert_eq!(ver["ok"], json!(true), "{ver}");
    assert!(
        !ver["findings"].as_array().is_some_and(|fs| fs.iter().any(|f| f["kind"] == json!("avoid_hit"))),
        "Face must have no avoid_hit finding: {ver}"
    );

    // Flute radius 3 + clearance 1.5 = 4.5 mm from the tower footprint [-6..6] for every move below its top.
    let ms = moves(&mut c, &dir, &op);
    assert!(!ms.is_empty(), "Face made no moves");
    let mut cut = 0;
    for m in &ms {
        // Sample each move, so that a row that crosses the tower between its ends is caught.
        for k in 0..=20 {
            let t = f64::from(k) / 20.0;
            let lerp = |a: f64, b: f64| a + (b - a) * t;
            let (x, y, z) = (lerp(m.from.x, m.to.x), lerp(m.from.y, m.to.y), lerp(m.from.z, m.to.z));
            if z >= 24.0 + 4.5 {
                continue;
            }
            let dist = (x.abs() - 6.0).max(0.0).hypot((y.abs() - 6.0).max(0.0));
            assert!(dist >= 4.5 - 1e-2, "move {m:?} came {dist:.3} mm near the tower, not 4.5 mm");
        }
        if m.kind != MoveKind::Rapid {
            cut += 1;
        }
    }
    assert!(cut > 0, "Face cut nothing");
    let nc = dir.join("face.nc");
    let posted = c.ok("post", json!({ "output": nc.display().to_string(), "post": "Fanuc" }));
    assert!(posted["lines"].as_u64().is_some_and(|n| n > 0));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn face_do_not_touch_refuses_an_impossible_cut_by_name_and_blocks_the_post() {
    let (dir, mut c, plate_top, _tower_top_face, _edge, _vert) = setup_plate_and_tower("face_block");
    let op = add_face(&mut c);
    // The plate top under the whole face, with a clearance wider than the cut leaves room for.
    c.ok("set_operation_do_not_touch", json!({
        "operation": op,
        "clearance": 16.0,
        "picks": [{ "model": "plate", "id": plate_top }]
    }));
    let blocked = c.refused("generate", json!({ "name": op }));
    assert!(blocked.contains("plate") && blocked.contains("Do not touch"), "refusal must name the item: {blocked}");
    let nc = dir.join("blocked.nc");
    let _ = c.refused("post", json!({ "output": nc.display().to_string(), "post": "Fanuc" }));
    assert!(!nc.exists(), "a blocked post must write no NC output");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn face_without_protection_keeps_its_toolpath() {
    let (dir, mut c, _plate_top, tower_top_face, _edge, _vert) = setup_plate_and_tower("face_same");
    let op = add_face(&mut c);
    c.ok("generate", json!({ "name": op }));
    let before = moves(&mut c, &dir, &op);
    // Set protection, then clear it again: the path is the unprotected path, move for move.
    c.ok("set_operation_do_not_touch", json!({
        "operation": op,
        "clearance": 1.5,
        "picks": [{ "model": "tower", "id": tower_top_face }]
    }));
    c.ok("generate", json!({ "name": op }));
    let guarded = moves(&mut c, &dir, &op);
    assert_ne!(before, guarded, "protection must change a Face that runs through the tower");
    // Undo the generate and the protection: the operation has no protected item again.
    c.ok("undo", json!({}));
    c.ok("undo", json!({}));
    let cleared = c.ok("get_operation_do_not_touch", json!({ "operation": op }));
    assert_eq!(cleared["picks"].as_array().map(Vec::len), Some(0), "{cleared}");
    c.ok("generate", json!({ "name": op }));
    assert_eq!(before, moves(&mut c, &dir, &op), "a Face without protected items must keep its toolpath");
    let _ = std::fs::remove_dir_all(&dir);
}
