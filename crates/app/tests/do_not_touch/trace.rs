//! Trace follows its selected line. A protected feature midway along the line
//! can meet the holder or shank while the cutter and both endpoints are clear.
//! Provenance: original synthetic stock, tool and tower dimensions.

use super::do_not_touch_support::{Client, setup_plate_and_tower};
use camcraft_app::commands::{History, MoveModel};
use camcraft_app::geometry::Point3;
use camcraft_app::schema::Location;
use camcraft_app::{model, operation, project, world::new_world};
use serde_json::json;
use std::path::Path;

fn saved_path(c: &mut Client, dir: &Path, name: &str) -> camcraft_app::cl::Toolpath {
    let file = dir.join("trace.camproj");
    c.ok("save_project", json!({ "path": file }));
    let w = new_world();
    project::load(&w, &file).expect("reload the Trace with its cache");
    assert!(operation::avoid::live_avoid_problems(&w).is_empty());
    let entity = model::find_by_name(&w, name).expect("saved operation");
    model::toolpath_of(&w, &entity).expect("cached Trace").path
}

#[test]
fn trace_protects_the_full_sweep_between_clear_endpoints() {
    let (dir, mut c, plate_top, guard_face, _, _) = setup_plate_and_tower("trace-sweep");
    c.ok("set_stock", json!({ "box": { "dx": 80.0, "dy": 80.0, "dz": 10.0 } }));
    let name = c.ok("add_operation", json!({
        "kind": "Trace", "tool": "SteppedMill",
        "params": {
            "stepdown": 1.0,
            "top_z": { "reference": "fixed", "offset": 10.0 },
            "bottom_z": { "reference": "fixed", "offset": 9.8 },
            "clearance_z": { "reference": "fixed", "offset": 50.0 },
            "retract_z": { "reference": "fixed", "offset": 45.0 }
        }
    }))["operation"].as_str().expect("Trace name").to_owned();
    c.ok("set_operation_geometry", json!({
        "operation": name, "picks": [{ "model": "plate", "id": plate_top }]
    }));
    // Trace follows x=40 from one end of the plate to the other. At the
    // height of the top of this tower the holder radius is 14 mm.
    c.ok("move_model", json!({ "name": "tower", "position": [61.1, 0.0, 14.0] }));
    c.ok("generate", json!({ "name": name }));
    let original = saved_path(&mut c, &dir, &name);
    assert!(original.moves.iter().any(|m| {
        (m.from.x - 40.0).abs() < 1e-6 && (m.to.x - 40.0).abs() < 1e-6
            && m.from.y * m.to.y < -100.0 && (m.to.z - 9.8).abs() < 1e-6
    }), "fixture must sweep past the tower between two clear endpoints");
    let picks = json!([{ "model": "tower", "id": guard_face }]);
    c.ok("set_operation_do_not_touch", json!({ "operation": name, "clearance": 1.0, "picks": picks }));
    c.ok("generate", json!({ "name": name }));
    let clear = saved_path(&mut c, &dir, &name);
    assert_eq!(clear.moves, original.moves, "a clear Trace keeps its selected line");

    // The tower is 12 mm wide. At x=60.9 its near side is 14.9 mm
    // from the line: a holder plus the required clearance needs 15 mm.
    // At z=25.0 and x=52.9 the corresponding reach is the 6 mm shank
    // plus 1 mm clearance. Neither the flute nor the endpoints touch.
    let loaded = new_world();
    project::load(&loaded, &dir.join("trace.camproj")).expect("load the cached protected Trace");
    let mut history = History::default();
    for (position, band) in [([60.9, 0.0, 14.0], "Holder"), ([52.9, 0.0, 1.0], "Shank")] {
        c.ok("move_model", json!({ "name": "tower", "position": position }));
        let report = c.ok("verify", json!({}));
        assert_eq!(report["ok"], json!(false), "{report}");
        // MCP stops at the stale stamp. Independently exercise the live
        // full-tool check on the cached path with the guard in its new place.
        let [x, y, z] = position;
        let loc = Location { pos: Point3::new(x, y, z), ..Location::default() };
        history.execute(&loaded, MoveModel::new(&loaded, "tower", loc).expect("move guard"));
        let hits = operation::avoid::live_avoid_problems(&loaded);
        assert!(hits.iter().any(|why| why.contains("tower face") && why.contains(band)), "{hits:?}");
        let problem = c.refused("generate", json!({ "name": name }));
        assert!(problem.contains("tower face") && problem.contains(band), "{problem}");
        let file = dir.join("unsafe.nc");
        c.refused("post", json!({ "output": file, "post": "Fanuc" }));
        assert!(!file.exists());
    }
    c.ok("set_operation_do_not_touch", json!({ "operation": name }));
    c.ok("generate", json!({ "name": name }));
    assert_eq!(saved_path(&mut c, &dir, &name).moves, original.moves, "removing protection restores the original moves");
    std::fs::remove_dir_all(dir).expect("remove test files");
}

#[test]
fn tilted_trace_refuses_a_protected_face_in_its_work_plane() {
    let (dir, mut c, _, guard_face, _, _) = setup_plate_and_tower("trace-tilt");
    let name = c.ok("add_operation", json!({
        "kind": "Trace", "tool": "SteppedMill", "source": "plate",
        "params": {
            "stepdown": 1.0,
            "top_z": { "reference": "fixed", "offset": 7.0 },
            "bottom_z": { "reference": "fixed", "offset": 6.8 },
            "clearance_z": { "reference": "fixed", "offset": 50.0 },
            "retract_z": { "reference": "fixed", "offset": 45.0 },
            "tilt": { "a_deg": 5.0, "b_deg": 0.0, "origin": [0.0, 0.0, 0.0] }
        }
    }))["operation"].as_str().expect("tilted Trace").to_owned();
    c.ok("generate", json!({ "name": name }));
    c.ok("move_model", json!({ "name": "tower", "position": [40.0, 0.0, 0.0] }));
    c.ok("set_operation_do_not_touch", json!({
        "operation": name, "clearance": 1.0, "picks": [{ "model": "tower", "id": guard_face }]
    }));
    let problem = c.refused("generate", json!({ "name": name }));
    assert!(problem.contains("tower face") && problem.contains("Do not touch"), "{problem}");
    std::fs::remove_dir_all(dir).expect("remove tilted test files");
}
