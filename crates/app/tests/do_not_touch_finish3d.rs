//! #3347: Finish3D honors protected faces and edges across the whole tool.
//! Provenance: original synthetic plate, tower, and stepped ball-tool fixtures.
//! Gate map (M4.4): covers app::operation::avoid, app::operation::generate, app::operation::types::avoid, app::mcp::do_not_touch.

mod common;
mod do_not_touch_support;

use camcraft_app::cl::{MoveKind, Toolpath};
use camcraft_app::{model, operation, project, world::new_world};
use do_not_touch_support::{Client, setup_plate_and_tower};
use serde_json::json;
use std::path::{Path, PathBuf};

fn setup(tag: &str) -> (PathBuf, Client, String, String, String) {
    let (dir, mut c, _, face, edge, _) = setup_plate_and_tower(tag);
    c.ok("set_stock", json!({ "box": { "dx": 80.0, "dy": 80.0, "dz": 10.0 } }));
    c.ok("add_tool", json!({ "catalog": "BN-0600", "name": "SteppedBall" }));
    c.ok("edit_tool", json!({ "name": "SteppedBall", "patch": {
        "flute_len": 15.0, "shank_d": 12.0, "shank_len": 12.0,
        "holder_d": 28.0, "holder_len": 20.0, "center_cutting": true
    } }));
    c.ok("move_model", json!({ "name": "tower", "position": [120.0, 0.0, 16.0] }));
    let op = c.ok("add_operation", json!({
        "kind": "3D Finish", "tool": "SteppedBall", "source": "plate",
        "params": {
            "stepover": 3.0, "tolerance": 0.05,
            "top_z": { "reference": "fixed", "offset": 10.0 },
            "bottom_z": { "reference": "fixed", "offset": 9.8 },
            "clearance_z": { "reference": "fixed", "offset": 50.0 },
            "retract_z": { "reference": "fixed", "offset": 45.0 }
        }
    }))["operation"].as_str().expect("Finish3D operation").to_owned();
    (dir, c, op, face, edge)
}

fn saved_path(c: &mut Client, dir: &Path, op: &str) -> Toolpath {
    let file = dir.join("finish.camproj");
    c.ok("save_project", json!({ "path": file }));
    let w = new_world();
    project::load(&w, &file).expect("load cached Finish3D");
    let problems = operation::avoid::live_avoid_problems(&w);
    assert!(problems.is_empty(), "full-tool clearance check: {problems:?}");
    let entity = model::find_by_name(&w, op).expect("Finish3D entity");
    model::toolpath_of(&w, &entity).expect("Finish3D path").path
}

fn protect(c: &mut Client, op: &str, face: &str, clearance: f64) {
    c.ok("set_operation_do_not_touch", json!({
        "operation": op, "clearance": clearance,
        "picks": [{ "model": "tower", "id": face }]
    }));
}

/// Independent samples measure the full-tool band against the protected
/// top face. The live check above also walks curved moves and the endpoints.
fn assert_face_clear(path: &Toolpath, centre: [f64; 3]) {
    let [cx, cy, top] = centre;
    for m in &path.moves {
        for (a, b) in m.segments3(0.01) {
            for i in 0..=100 {
                let t = f64::from(i) / 100.0;
                let x = a.x + (b.x - a.x) * t;
                let y = a.y + (b.y - a.y) * t;
                let z = a.z + (b.z - a.z) * t;
                let above_tip = top - z;
                let radius = if (27.0..=47.0).contains(&above_tip) {
                    14.0
                } else if (15.0..27.0).contains(&above_tip) {
                    6.0
                } else if (0.0..15.0).contains(&above_tip) {
                    3.0
                } else {
                    continue;
                };
                let dx = ((x - cx).abs() - 6.0).max(0.0);
                let dy = ((y - cy).abs() - 6.0).max(0.0);
                assert!(dx.hypot(dy) >= radius + 1.0 - 0.02,
                    "tool band radius {radius} violates the protected top face at ({x}, {y}, {z}): {m:?}");
            }
        }
    }
}

#[test]
fn finish3d_accepts_protected_faces_and_edges_and_keeps_unprotected_moves() {
    let (dir, mut c, op, face, edge) = setup("finish-clear");
    c.ok("generate", json!({ "name": op }));
    let original = saved_path(&mut c, &dir, &op);
    // This must fail before the fix because Finish3D refuses protected picks.
    c.ok("set_operation_do_not_touch", json!({
        "operation": op, "clearance": 1.0,
        "picks": [{ "model": "tower", "id": face }, { "model": "tower", "id": edge }]
    }));
    c.ok("undo", json!({}));
    let undone = c.ok("get_operation_do_not_touch", json!({ "operation": op }));
    assert_eq!(undone["picks"], json!([]), "undo removes the protected picks");
    c.ok("redo", json!({}));
    let redone = c.ok("get_operation_do_not_touch", json!({ "operation": op }));
    assert_eq!(redone["picks"].as_array().expect("protected picks").len(), 2);
    c.ok("generate", json!({ "name": op }));
    assert_eq!(saved_path(&mut c, &dir, &op).moves, original.moves,
        "clear protection preserves the existing path");
    c.ok("set_operation_do_not_touch", json!({ "operation": op }));
    c.ok("generate", json!({ "name": op }));
    assert_eq!(saved_path(&mut c, &dir, &op).moves, original.moves,
        "removing protection preserves the original unprotected path");
    std::fs::remove_dir_all(dir).expect("remove fixture");
}

#[test]
fn finish3d_keeps_flutes_shank_and_holder_clear_of_a_protected_face() {
    let (dir, mut c, op, face, _) = setup("finish-body");
    c.ok("generate", json!({ "name": op }));
    let original = saved_path(&mut c, &dir, &op);
    let row = original.moves.iter().filter(|m| {
        m.kind == MoveKind::Feed && (m.from.y - m.to.y).abs() < 1e-6
            && (m.from.x - m.to.x).abs() > 20.0 && (m.to.z - 10.0).abs() < 0.1
    }).max_by(|a, b| a.to.y.total_cmp(&b.to.y)).expect("long outer raster row");
    let x = row.from.x.midpoint(row.to.x);
    let y = row.to.y;
    for (top, reach, band) in [(40.0, 14.0, "Holder"), (30.0, 6.0, "Shank"), (20.0, 3.0, "Flute")] {
        // The near edge is 0.1 mm inside this band's required envelope.
        let centre = [x, y + 6.0 + reach + 0.9, top];
        c.ok("move_model", json!({ "name": "tower", "position": [centre[0], centre[1], top - 24.0] }));
        protect(&mut c, &op, &face, 1.0);
        // Before regeneration, the old cached path must fail the live
        // protected-geometry check even if its old generation stamp passed.
        let file = dir.join("before.camproj");
        c.ok("save_project", json!({ "path": file }));
        let w = new_world();
        project::load(&w, &file).expect("load old path with current protection");
        let problems = operation::avoid::live_avoid_problems(&w);
        assert!(problems.iter().any(|p| p.contains("tower face")), "{band}: {problems:?}");
        c.ok("generate", json!({ "name": op }));
        let protected = saved_path(&mut c, &dir, &op);
        assert_ne!(protected.moves, original.moves, "{band} must alter the nearby path");
        assert_face_clear(&protected, centre);
        c.ok("set_operation_do_not_touch", json!({ "operation": op }));
        c.ok("generate", json!({ "name": op }));
        assert_eq!(saved_path(&mut c, &dir, &op).moves, original.moves,
            "removing {band} protection restores the original moves");
    }
    std::fs::remove_dir_all(dir).expect("remove fixture");
}

#[test]
fn finish3d_refuses_an_impossible_protected_cut_by_name_and_blocks_post() {
    let (dir, mut c, op, face, _) = setup("finish-refuse");
    c.ok("move_model", json!({ "name": "tower", "position": [0.0, 0.0, 0.0] }));
    protect(&mut c, &op, &face, 200.0);
    let problem = c.refused("generate", json!({ "name": op }));
    assert!(problem.contains("Do not touch") && problem.contains("tower face"), "{problem}");
    let output = dir.join("unsafe.nc");
    c.refused("post", json!({ "output": output, "post": "Fanuc" }));
    assert!(!output.exists(), "a refused Finish3D must not write G-code");
    std::fs::remove_dir_all(dir).expect("remove fixture");
}

#[test]
fn finish3d_lifts_links_above_a_protected_face_higher_than_clearance() {
    let (dir, mut c, op, face, _) = setup("finish-high-link");
    c.ok("generate", json!({ "name": op }));
    let original = saved_path(&mut c, &dir, &op);
    let centre = [0.0, 0.0, 70.0];
    c.ok("move_model", json!({ "name": "tower", "position": [0.0, 0.0, 46.0] }));
    protect(&mut c, &op, &face, 1.0);
    c.ok("generate", json!({ "name": op }));
    let protected = saved_path(&mut c, &dir, &op);
    assert_ne!(protected.moves, original.moves);
    assert!(protected.moves.iter().any(|m| m.from.z > 50.0 || m.to.z > 50.0),
        "links must rise above the nominal clearance plane");
    assert_face_clear(&protected, centre);
    std::fs::remove_dir_all(dir).expect("remove fixture");
}

#[test]
fn finish3d_rotated_wcs_keeps_the_protected_face_in_setup_coordinates() {
    let (dir, mut c, op, face, _) = setup("finish-yaw");
    let half = 15.0_f64.to_radians();
    c.ok("set_wcs", json!({
        "origin": [0.0, 0.0, 0.0], "rotation": [0.0, 0.0, half.sin(), half.cos()]
    }));
    c.ok("generate", json!({ "name": op }));
    let original = saved_path(&mut c, &dir, &op);
    let centre = [20.0, 20.0, 40.0];
    c.ok("move_model", json!({ "name": "tower", "position": [20.0, 20.0, 16.0] }));
    protect(&mut c, &op, &face, 1.0);
    c.ok("generate", json!({ "name": op }));
    let protected = saved_path(&mut c, &dir, &op);
    assert!(!protected.moves.is_empty(), "the safe part of the cut remains");
    assert_ne!(protected.moves, original.moves, "the asymmetric guard clips the rotated raster");
    assert_face_clear(&protected, centre);
    std::fs::remove_dir_all(dir).expect("remove fixture");
}
