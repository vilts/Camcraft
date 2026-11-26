//! #2055: Protection persistence, migrations and version refusals.
//! Gate map (M4.4): covers app::project, app::mcp.

mod common;
mod do_not_touch_support;

use camcraft_app::project::{self, NEWEST_PROJECT_VERSION, PROJECT_VERSION, ProjectError};
use camcraft_app::world::new_world;
use do_not_touch_support::{Client, setup_plate_and_tower};
use serde_json::{Value, json};

#[test]
fn protected_project_roundtrips_and_refuses_a_future_version() {
    let (dir, mut c, plate_top, tower_top_face, tower_edge, _vert) =
        setup_plate_and_tower("proj_persistence");
    let op_name = c.ok(
        "add_operation",
        json!({
            "kind": "2D Offset",
            "tool": "SteppedMill",
            "params": {
                "stepover": 2.5,
                "stepdown": 7.0,
                "stock_to_leave": 0.5,
                "top_z": { "reference": "fixed", "offset": 24.0 },
                "bottom_z": { "reference": "fixed", "offset": 10.0 },
                "clearance_z": { "reference": "fixed", "offset": 35.0 },
                "retract_z": { "reference": "fixed", "offset": 26.0 }
            }
        }),
    )["operation"]
        .as_str()
        .expect("op name")
        .to_owned();
    c.ok(
        "set_operation_geometry",
        json!({
            "operation": op_name,
            "picks": [{ "model": "plate", "id": plate_top }]
        }),
    );
    c.ok(
        "set_operation_do_not_touch",
        json!({
            "operation": op_name,
            "clearance": 1.5,
            "picks": [
                { "model": "tower", "id": tower_top_face },
                { "model": "tower", "id": tower_edge }
            ]
        }),
    );
    c.ok("generate", json!({ "name": op_name }));

    // Save -> inspect current version -> reload into fresh server -> still intact & clean.
    let proj_path = dir.join("protected.camproj");
    c.ok(
        "save_project",
        json!({ "path": proj_path.display().to_string() }),
    );
    let raw = std::fs::read_to_string(&proj_path).expect("read saved project");
    let mut parsed: Value = serde_json::from_str(&raw).expect("valid json");
    assert_eq!(parsed["version"], json!(PROJECT_VERSION));
    assert!(raw.contains("do_not_touch"));
    assert_eq!(v72_reader_area(&raw), Err(PROJECT_VERSION));
    let mut legacy = parsed.clone();
    legacy["version"] = json!(72);
    let legacy_area = v72_reader_area(&legacy.to_string()).expect("v72 accepts its own version");
    assert!(
        !legacy_area.to_string().contains("do_not_touch"),
        "the old DTO drops the protection"
    );

    let mut c2 = Client::new();
    c2.ok(
        "open_project",
        json!({ "path": proj_path.display().to_string() }),
    );
    let got2 = c2.ok(
        "get_operation_do_not_touch",
        json!({ "operation": op_name }),
    );
    assert_eq!(got2["clearance"], json!(1.5));
    assert_eq!(got2["picks"].as_array().map(Vec::len), Some(2));
    assert_eq!(c2.ok("verify", json!({}))["ok"], json!(true));

    // #1864: a v73 file (the first protection format) steps up to the
    // current version and keeps its protection.
    let mut v73 = parsed.clone();
    v73["version"] = json!(73);
    let v73_path = dir.join("protected_v73.camproj");
    std::fs::write(&v73_path, v73.to_string()).expect("v73 fixture");
    let mut c3 = Client::new();
    c3.ok("open_project", json!({ "path": v73_path.display().to_string() }));
    let got3 = c3.ok("get_operation_do_not_touch", json!({ "operation": op_name }));
    assert_eq!(got3["clearance"], json!(1.5));
    assert_eq!(got3["picks"].as_array().map(Vec::len), Some(2));

    // A future version is refused with UnsupportedVersion instead of ignoring protection.
    parsed["version"] = json!(NEWEST_PROJECT_VERSION + 1);
    let future_path = dir.join("future.camproj");
    std::fs::write(&future_path, serde_json::to_string(&parsed).unwrap()).unwrap();
    c.refused(
        "open_project",
        json!({ "path": future_path.display().to_string() }),
    );
    let future_world = new_world();
    let err =
        project::load(&future_world, &future_path).expect_err("newer version must be refused");
    assert!(
        matches!(err, ProjectError::UnsupportedVersion(v) if v == NEWEST_PROJECT_VERSION + 1),
        "{err:?}"
    );

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn project_keeps_linked_sources_and_protection_and_refuses_a_bad_source() {
    let (dir, mut c, _plate_top, tower_top_face, _edge, _vert) =
        setup_plate_and_tower("linked_protection");
    let op_name = c.ok(
        "add_operation",
        json!({
            "kind": "3D Pocket", "tool": "SteppedMill", "source": "plate"
        }),
    )["operation"]
        .as_str()
        .expect("operation name")
        .to_owned();
    c.ok(
        "set_operation_do_not_touch",
        json!({
            "operation": op_name,
            "clearance": 1.5,
            "picks": [{ "model": "tower", "id": tower_top_face }]
        }),
    );
    let path = dir.join("linked.camproj");
    c.ok(
        "save_project",
        json!({ "path": path.display().to_string() }),
    );
    let raw = std::fs::read_to_string(&path).expect("saved project");
    let mut doc: Value = serde_json::from_str(&raw).expect("json");
    assert_eq!(doc["version"], json!(PROJECT_VERSION));
    let rows = doc["entities"].as_array().expect("entities");
    let tower = rows
        .iter()
        .find(|row| row["name"] == "tower")
        .expect("tower");
    let source = tower["source"].clone();
    assert_eq!(
        source["path"],
        json!(dir.join("tower.stl").display().to_string())
    );
    assert_eq!(source["sha256"].as_str().map(str::len), Some(64));
    let op = rows
        .iter()
        .find(|row| row["name"] == op_name)
        .expect("operation");
    let area = op["operation"]["area"].clone();
    assert!(area.to_string().contains("do_not_touch"), "{area}");

    let world = new_world();
    project::open(&world, &raw).expect("linked sources and protection open together");
    let roundtrip = dir.join("roundtrip.camproj");
    project::save(&world, &roundtrip).expect("round trip");
    let before = std::fs::read_to_string(&roundtrip).expect("read round trip");
    let restored: Value = serde_json::from_str(&before).expect("round trip json");
    let rows = restored["entities"].as_array().expect("entities");
    assert_eq!(
        rows.iter()
            .find(|row| row["name"] == "tower")
            .expect("tower")["source"],
        source
    );
    assert_eq!(
        rows.iter()
            .find(|row| row["name"] == op_name)
            .expect("operation")["operation"]["area"],
        area
    );

    let rows = doc["entities"].as_array_mut().expect("entities");
    rows.iter_mut()
        .find(|row| row["name"] == "tower")
        .expect("tower")["source"]["sha256"] = json!("ABC");
    let err =
        project::open(&world, &doc.to_string()).expect_err("invalid linked source must refuse");
    assert!(
        err.to_string()
            .contains("'tower' records a source whose sha256 is not 64 lowercase hex digits"),
        "{err}"
    );
    project::save(&world, &roundtrip).expect("save after refusal");
    assert_eq!(
        std::fs::read_to_string(&roundtrip).expect("unchanged project"),
        before,
        "the refusal makes no world writes"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

// Reader contract from upstream 6df70dfc5: project/area.rs AreaDto and
// project/apply.rs:502-556. Serde ignores unknown area fields; the final
// version guard prevents an old build from using that incomplete area.
fn v72_reader_area(raw: &str) -> Result<Value, u32> {
    #[derive(serde::Deserialize, serde::Serialize)]
    struct AreaV72 {
        #[serde(default)]
        containment: Option<Value>,
        #[serde(default)]
        offset: f64,
        #[serde(default = "round")]
        corner: String,
        #[serde(default)]
        extension: f64,
        #[serde(default)]
        extra: Vec<Value>,
        #[serde(default)]
        open: Option<Value>,
    }
    fn round() -> String {
        "round".to_owned()
    }
    let file: Value = serde_json::from_str(raw).expect("project JSON");
    let mut version =
        u32::try_from(file["version"].as_u64().expect("version")).expect("u32 version");
    // The v72 reader steps supported files up before the final guard.
    if (53..=72).contains(&version) {
        version = 72;
    }
    if version != 72 {
        return Err(version);
    }
    let operation = file["entities"]
        .as_array()
        .expect("entities")
        .iter()
        .find_map(|row| row.get("operation"))
        .expect("operation");
    let area: AreaV72 = serde_json::from_value(operation["area"].clone()).expect("v72 area DTO");
    Ok(serde_json::to_value(area).expect("v72 area JSON"))
}

#[test]
fn v72_linked_project_migrates_without_protection() {
    let (dir, mut c, _plate, _face, _edge, _vert) = setup_plate_and_tower("v72_migration");
    let op_name = c.ok(
        "add_operation",
        json!({
            "kind": "3D Pocket", "tool": "SteppedMill", "source": "plate"
        }),
    )["operation"]
        .as_str()
        .expect("operation name")
        .to_owned();
    let path = dir.join("v72.camproj");
    c.ok(
        "save_project",
        json!({ "path": path.display().to_string() }),
    );
    let mut old: Value =
        serde_json::from_str(&std::fs::read_to_string(&path).expect("project")).expect("JSON");
    old["version"] = json!(72);
    for row in old["entities"].as_array_mut().expect("entities") {
        if let Some(area) = row
            .get_mut("operation")
            .and_then(|op| op.get_mut("area"))
            .and_then(Value::as_object_mut)
        {
            area.remove("do_not_touch");
        }
    }
    std::fs::write(&path, old.to_string()).expect("v72 fixture");
    let mut restored = Client::new();
    restored.ok(
        "open_project",
        json!({ "path": path.display().to_string() }),
    );
    let protection = restored.ok(
        "get_operation_do_not_touch",
        json!({ "operation": op_name }),
    );
    assert_eq!(protection["clearance"], json!(0.0));
    assert_eq!(protection["picks"], json!([]));
    restored.ok(
        "save_project",
        json!({ "path": path.display().to_string() }),
    );
    let migrated: Value =
        serde_json::from_str(&std::fs::read_to_string(&path).expect("migrated")).expect("JSON");
    assert_eq!(migrated["version"], json!(PROJECT_VERSION));
    let model = |file: &Value| {
        file["entities"]
            .as_array()
            .expect("entities")
            .iter()
            .find(|row| row["name"] == "tower")
            .expect("tower")["source"]
            .clone()
    };
    assert_eq!(model(&migrated), model(&old));
    let _ = std::fs::remove_dir_all(&dir);
}
