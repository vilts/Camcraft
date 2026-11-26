//! #3349: "Do not touch" protection for Engrave operations across commands,
//! generation, tilted frames, project roundtrip, verify, and post gate.
//! Gate map (M4.4): covers app::operation, app::commands::operation::do_not_touch,
//! app::mcp, app::posting, motion::machining2d::trace_avoid, sim::verify::avoid.

mod common;
mod do_not_touch_support;

use camcraft_app::geometry::{Point3, Triangle};
use camcraft_app::project;
use camcraft_app::verify::{AvoidItem, AvoidSpec, Cutter, AVOID_VERIFY_TOL, FixturePart, Warning};
use camcraft_app::world::new_world;
use camcraft_app::{model, operation, posting};
use do_not_touch_support::{Client, setup_plate_and_tower, tmp_dir, write_box_stl};
use serde_json::json;

fn box_facets(min: [f64; 3], max: [f64; 3]) -> Vec<Triangle> {
    let p = |x, y, z| Point3::new(x, y, z);
    let v = |i: usize| p([min[0], max[0]][i & 1], [min[1], max[1]][(i >> 1) & 1], [min[2], max[2]][(i >> 2) & 1]);
    let quad = |a: usize, b: usize, c: usize, d: usize| -> [Triangle; 2] {
        [Triangle::new(v(a), v(b), v(c)), Triangle::new(v(a), v(c), v(d))]
    };
    let mut t = Vec::with_capacity(12);
    t.extend(quad(4, 5, 7, 6));
    t.extend(quad(0, 2, 3, 1));
    t.extend(quad(0, 1, 5, 4));
    t.extend(quad(2, 6, 7, 3));
    t.extend(quad(1, 3, 7, 5));
    t.extend(quad(2, 0, 4, 6));
    t
}

fn top_face_id(c: &mut Client, model_name: &str, z: f64) -> String {
    let feats = c.ok("list_features", json!({ "model": model_name, "kinds": ["faces"] }));
    feats["faces"]
        .as_array()
        .expect("faces")
        .iter()
        .find(|f| f["bounds"]["min"][2].as_f64().is_some_and(|fz| (fz - z).abs() < 1e-3))
        .expect("top face")["id"]
        .as_str()
        .expect("face id")
        .to_owned()
}

#[test]
fn engrave_without_do_not_touch_is_unchanged() {
    let dir = tmp_dir("engrave_parity");
    let pad_stl = dir.join("pad.stl");
    let guard_stl = dir.join("guard.stl");
    write_box_stl(&pad_stl, -20.0, -20.0, 0.0, 20.0, 20.0, 10.0);
    write_box_stl(&guard_stl, 30.0, -6.0, 0.0, 38.0, 6.0, 10.0);

    let make_doc = |with_guard: bool| {
        let mut c = Client::new();
        c.ok("import_model", json!({ "path": pad_stl.display().to_string(), "name": "pad" }));
        if with_guard {
            c.ok("import_model", json!({ "path": guard_stl.display().to_string(), "name": "guard" }));
        }
        c.ok("set_stock", json!({ "box": { "dx": 90.0, "dy": 50.0, "dz": 10.0 } }));
        c.ok("add_tool", json!({ "catalog": "EM-0600-4FL", "name": "Mill6" }));
        let pad_top = top_face_id(&mut c, "pad", 10.0);
        let op = c.ok("add_operation", json!({
            "kind": "Engrave",
            "tool": "Mill6",
            "params": {
                "stepdown": 5.0,
                "top_z": { "reference": "fixed", "offset": 10.0 },
                "bottom_z": { "reference": "fixed", "offset": 9.7 },
                "clearance_z": { "reference": "fixed", "offset": 30.0 },
                "retract_z": { "reference": "fixed", "offset": 16.0 }
            }
        }))["operation"].as_str().expect("op").to_owned();
        c.ok("set_operation_geometry", json!({
            "operation": op,
            "picks": [{ "model": "pad", "id": pad_top }]
        }));
        c.ok("generate", json!({ "name": op }));
        let nc = dir.join(if with_guard { "with_guard.nc" } else { "plain.nc" });
        c.ok("post", json!({ "output": nc.display().to_string(), "post": "Fanuc" }));
        std::fs::read_to_string(&nc).expect("read nc")
    };

    assert_eq!(make_doc(false), make_doc(true));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn engrave_do_not_touch_keeps_full_tool_clear_elevates_links_and_supports_tilted() {
    let dir = tmp_dir("engrave_clear");
    let left_stl = dir.join("left.stl");
    let right_stl = dir.join("right.stl");
    let tower_stl = dir.join("tower.stl");
    let ledge_stl = dir.join("ledge.stl");
    // Two separate pads at z=24 (matching stock dz=24) with a protected tower overlapping right_pad's
    // west edge [-6..14] and forcing rapid links at retract_z=25 to elevate over tower top z=24 + 2.0.
    write_box_stl(&left_stl, -28.0, -12.0, 0.0, -12.0, 12.0, 24.0);
    write_box_stl(&right_stl, 12.0, -12.0, 0.0, 28.0, 12.0, 24.0);
    write_box_stl(&tower_stl, -6.0, -8.0, 0.0, 14.0, 8.0, 24.0);
    write_box_stl(&ledge_stl, 34.0, -5.0, 0.0, 42.0, 5.0, 24.0);

    let mut c = Client::new();
    c.ok("import_model", json!({ "path": left_stl.display().to_string(), "name": "left" }));
    c.ok("import_model", json!({ "path": right_stl.display().to_string(), "name": "right" }));
    c.ok("import_model", json!({ "path": tower_stl.display().to_string(), "name": "tower" }));
    c.ok("import_model", json!({ "path": ledge_stl.display().to_string(), "name": "ledge" }));
    c.ok("set_stock", json!({ "box": { "dx": 90.0, "dy": 40.0, "dz": 24.0 } }));
    c.ok("add_tool", json!({ "catalog": "EM-0400-2FL", "name": "SteppedMill" }));
    c.ok("edit_tool", json!({
        "name": "SteppedMill",
        "patch": {
            "diameter": 6.0,
            "flute_len": 15.0,
            "shank_d": 12.0,
            "shank_len": 12.0,
            "holder_d": 28.0,
            "holder_len": 20.0,
            "center_cutting": true
        }
    }));

    let left_top = top_face_id(&mut c, "left", 24.0);
    let right_top = top_face_id(&mut c, "right", 24.0);
    let tower_top = top_face_id(&mut c, "tower", 24.0);
    let tower_feats = c.ok("list_features", json!({ "model": "tower", "kinds": ["chains", "vertices"] }));
    let tower_edge = tower_feats["chains"][0]["id"].as_str().expect("chain").to_owned();
    let tower_vert = tower_feats["vertices"][0]["id"].as_str().expect("vert").to_owned();
    let ledge_top = top_face_id(&mut c, "ledge", 24.0);

    let op_name = c.ok("add_operation", json!({
        "kind": "Engrave",
        "tool": "SteppedMill",
        "params": {
            "stepdown": 5.0,
            "top_z": { "reference": "fixed", "offset": 24.0 },
            "bottom_z": { "reference": "fixed", "offset": 23.7 },
            "clearance_z": { "reference": "fixed", "offset": 35.0 },
            "retract_z": { "reference": "fixed", "offset": 25.0 }
        }
    }))["operation"].as_str().expect("op").to_owned();
    c.ok("set_operation_geometry", json!({
        "operation": op_name,
        "picks": [
            { "model": "left", "id": left_top },
            { "model": "right", "id": right_top }
        ]
    }));

    // Validation refusals: vertex pick & overlap with machining selection.
    let v_err = c.refused("set_operation_do_not_touch", json!({
        "operation": op_name,
        "picks": [{ "model": "tower", "id": tower_vert }]
    }));
    assert!(v_err.contains("vertex"), "{v_err}");
    let ov_err = c.refused("set_operation_do_not_touch", json!({
        "operation": op_name,
        "picks": [{ "model": "left", "id": left_top }]
    }));
    assert!(ov_err.contains("left"), "{ov_err}");

    c.ok("set_operation_do_not_touch", json!({
        "operation": op_name,
        "clearance": 2.0,
        "picks": [
            { "model": "tower", "id": tower_top },
            { "model": "tower", "id": tower_edge },
            { "model": "ledge", "id": ledge_top }
        ]
    }));
    // Undo / redo around SetOperationDoNotTouch.
    c.ok("undo", json!({}));
    assert_eq!(c.ok("get_operation_do_not_touch", json!({ "operation": op_name }))["picks"].as_array().map(Vec::len), Some(0));
    c.ok("redo", json!({}));
    let got = c.ok("get_operation_do_not_touch", json!({ "operation": op_name }));
    assert_eq!(got["clearance"], json!(2.0));
    assert_eq!(got["picks"].as_array().map(Vec::len), Some(3));

    let generated = c.ok("generate", json!({ "name": op_name }));
    assert_eq!(generated["generated"][0]["status"], json!("ok"), "{generated}");
    let ver = c.ok("verify", json!({ "name": op_name }));
    assert_eq!(ver["ok"], json!(true), "Engrave with Do not touch must verify clean: {ver}");
    assert!(
        !ver["findings"].as_array().is_some_and(|fs| fs.iter().any(|f| f["kind"] == json!("avoid_hit"))),
        "no avoid_hit allowed: {ver}"
    );

    let nc = dir.join("engrave_protected.nc");
    let posted = c.ok("post", json!({ "output": nc.display().to_string(), "post": "Fanuc" }));
    assert!(posted["lines"].as_u64().is_some_and(|n| n > 0));

    // Save/load roundtrip preserves protection and verification status.
    let proj = dir.join("engrave_dnt.camproj");
    c.ok("save_project", json!({ "path": proj.display().to_string() }));
    let mut c2 = Client::new();
    c2.ok("open_project", json!({ "path": proj.display().to_string() }));
    assert_eq!(
        c2.ok("get_operation_do_not_touch", json!({ "operation": op_name }))["picks"].as_array().map(Vec::len),
        Some(3)
    );
    assert_eq!(c2.ok("verify", json!({ "name": op_name }))["ok"], json!(true));

    // Independent world-space full-assembly check against exact STL boxes.
    let world = new_world();
    project::load(&world, &proj).expect("reload");
    let op_ent = model::find_by_name(&world, &op_name).expect("op");
    let cached = model::toolpath_of(&world, &op_ent).expect("cached toolpath");
    assert!(
        cached.path.moves.iter().any(|m| m.to.z > 26.0 && (m.to.x - m.from.x).hypot(m.to.y - m.from.y) > 1.0),
        "rapid link across tower must elevate above z=24 + clearance=2: {:?}",
        cached.path.moves
    );
    // Check no move midpoint or endpoint cuts within flute radius + clearance = 5.0 mm of tower [-6..14]x[-8..8].
    for m in &cached.path.moves {
        for p in [m.from, m.to, Point3::new((m.from.x + m.to.x) * 0.5, (m.from.y + m.to.y) * 0.5, (m.from.z + m.to.z) * 0.5)] {
            if p.z <= 24.0 + 2.0 - 1e-3 {
                let dx = (p.x - 14.0).max(-6.0 - p.x).max(0.0);
                let dy = (p.y.abs() - 8.0).max(0.0);
                assert!(dx.hypot(dy) >= 5.0 - 1e-2, "move {m:?} came too close to tower at {p:?}");
            }
        }
    }
    let tool_ent = model::find_by_name(&world, "SteppedMill").expect("tool");
    let tool = model::tool_of(&world, &tool_ent).expect("tool def");
    let spec = AvoidSpec {
        items: vec![
            AvoidItem { label: "tower".into(), tris: box_facets([-6.0, -8.0, 0.0], [14.0, 8.0, 24.0]), edges: vec![] },
            AvoidItem { label: "ledge".into(), tris: box_facets([34.0, -5.0, 0.0], [42.0, 5.0, 24.0]), edges: vec![] },
        ],
        cutter: Cutter::of(&tool),
        clearance: 2.0,
    };
    let hits = spec.hits_named(&cached.path, AVOID_VERIFY_TOL).expect("hits_named");
    assert!(hits.is_empty(), "every cut/plunge/link must clear tower and ledge: {hits:?}");

    // Tilted 3+2 Engrave with Do not touch also generates & verifies clean.
    let tilted = c.ok("add_operation", json!({
        "kind": "Engrave",
        "tool": "SteppedMill",
        "source": "left",
        "params": {
            "stepdown": 5.0,
            "top_z": { "reference": "top_of_model", "offset": 0.0 },
            "bottom_z": { "reference": "top_of_model", "offset": -0.3 },
            "clearance_z": { "reference": "fixed", "offset": 40.0 },
            "retract_z": { "reference": "fixed", "offset": 30.0 },
            "tilt": { "a_deg": 5.0, "b_deg": 0.0, "origin": [0.0, 0.0, 0.0] }
        }
    }))["operation"].as_str().expect("tilted").to_owned();
    c.ok("set_operation_do_not_touch", json!({
        "operation": tilted,
        "clearance": 1.0,
        "picks": [{ "model": "tower", "id": tower_top }]
    }));
    let tgen = c.ok("generate", json!({ "name": tilted }));
    assert_eq!(tgen["generated"][0]["status"], json!("ok"), "{tgen}");
    let tver = c.ok("verify", json!({ "name": tilted }));
    assert_eq!(tver["ok"], json!(true), "tilted engrave verify failed: {tver}");

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn engrave_do_not_touch_refuses_impossible_cut_and_blocks_post() {
    // 1. Covering the entire plate with a protected face refuses generation naming the item, and blocks post.
    let (dir, mut c, plate_top, _tower_top_face, _tower_edge, _vert) = setup_plate_and_tower("engrave_block");
    let op_full = c.ok("add_operation", json!({
        "kind": "Engrave",
        "tool": "SteppedMill",
        "source": "plate",
        "params": {
            "stepdown": 5.0,
            "top_z": { "reference": "fixed", "offset": 10.0 },
            "bottom_z": { "reference": "fixed", "offset": 9.7 },
            "clearance_z": { "reference": "fixed", "offset": 35.0 },
            "retract_z": { "reference": "fixed", "offset": 26.0 }
        }
    }))["operation"].as_str().expect("op_full").to_owned();
    c.ok("set_operation_do_not_touch", json!({
        "operation": op_full,
        "clearance": 20.0,
        "picks": [{ "model": "plate", "id": plate_top }]
    }));
    let blocked = c.refused("generate", json!({ "name": op_full }));
    assert!(
        blocked.contains("plate") && blocked.contains("Do not touch"),
        "expected protected item name in refusal: {blocked}"
    );
    let nc_blocked = dir.join("blocked_full.nc");
    let post_err = c.refused("post", json!({ "output": nc_blocked.display().to_string(), "post": "Fanuc" }));
    assert!(!nc_blocked.exists(), "blocked post must never create NC file: {post_err}");
    let _ = std::fs::remove_dir_all(&dir);

    // 2. Moving a protected item into an already-generated Engrave path blocks verify & post via live_avoid_problems.
    let dir2 = tmp_dir("engrave_live");
    let plate24 = dir2.join("plate24.stl");
    let tower24 = dir2.join("tower24.stl");
    write_box_stl(&plate24, -40.0, -40.0, 0.0, 40.0, 40.0, 24.0);
    write_box_stl(&tower24, -6.0, -6.0, 0.0, 6.0, 6.0, 24.0);

    let mut c2 = Client::new();
    c2.ok("import_model", json!({ "path": plate24.display().to_string(), "name": "plate" }));
    c2.ok("import_model", json!({ "path": tower24.display().to_string(), "name": "tower" }));
    c2.ok("set_stock", json!({ "box": { "dx": 80.0, "dy": 80.0, "dz": 24.0 } }));
    c2.ok("add_tool", json!({ "catalog": "EM-0400-2FL", "name": "SteppedMill" }));
    let tower_top = top_face_id(&mut c2, "tower", 24.0);

    let op_live = c2.ok("add_operation", json!({
        "kind": "Engrave",
        "tool": "SteppedMill",
        "source": "plate",
        "params": {
            "stepdown": 5.0,
            "top_z": { "reference": "fixed", "offset": 24.0 },
            "bottom_z": { "reference": "fixed", "offset": 23.7 },
            "clearance_z": { "reference": "fixed", "offset": 35.0 },
            "retract_z": { "reference": "fixed", "offset": 26.0 }
        }
    }))["operation"].as_str().expect("op_live").to_owned();
    c2.ok("set_operation_do_not_touch", json!({
        "operation": op_live,
        "clearance": 1.5,
        "picks": [{ "model": "tower", "id": tower_top }]
    }));
    c2.ok("generate", json!({ "name": op_live }));
    assert_eq!(c2.ok("verify", json!({}))["ok"], json!(true));

    // Move tower from center onto the plate edge x=40 so the cached Engrave path now hits it.
    c2.ok("move_model", json!({ "name": "tower", "position": [38.0, 0.0, 0.0] }));
    let ver_bad = c2.ok("verify", json!({}));
    assert_eq!(ver_bad["ok"], json!(false), "moving protected tower into path must fail verify: {ver_bad}");
    let nc_moved = dir2.join("blocked_moved.nc");
    let moved_err = c2.refused("post", json!({ "output": nc_moved.display().to_string(), "post": "Fanuc" }));
    assert!(!nc_moved.exists(), "blocked post must never write NC output: {moved_err}");

    // Also verify cached Warning::AvoidHit on an Engrave operation directly blocks post_setup.
    let proj = dir2.join("engrave_live.camproj");
    c2.ok("move_model", json!({ "name": "tower", "position": [0.0, 0.0, 0.0] }));
    c2.ok("generate", json!({ "name": op_live }));
    c2.ok("save_project", json!({ "path": proj.display().to_string() }));
    let w = new_world();
    project::load(&w, &proj).expect("reload");
    assert!(operation::avoid::live_avoid_problems(&w).is_empty());
    let op_ent = model::find_by_name(&w, &op_live).expect("op entity");
    let mut cache = model::toolpath_of(&w, &op_ent).expect("cached");
    cache.warnings.push(Warning::AvoidHit { at: 0, item: 1, part: FixturePart::Shank, short_mm: 0.5 });
    op_ent.entity_view(&w).set(cache);
    let err = posting::post_setup(&w, &nc_moved).expect_err("cached AvoidHit must block post");
    assert!(err.contains("Do not touch") || err.contains("warning"), "{err}");
    assert!(!nc_moved.exists());

    let _ = std::fs::remove_dir_all(&dir2);
}
