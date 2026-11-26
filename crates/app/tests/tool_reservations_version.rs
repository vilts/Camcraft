//! Provenance: original project-version pins for stored machine tool reservations (#2604).
//! Gate map (M4.4): covers app::project::file, app::project::apply, app::machine,
//! post::machine::def, post::machine::reservations.

use camcraft_app::commands::{History, LoadMachine};
use camcraft_app::machine;
use camcraft_app::project;
use camcraft_app::world::new_world;
use serde_json::json;

#[test]
fn saved_machine_reservations_need_the_v78_reader() {
    let world = new_world();
    let mounted = machine::parse_mch(
        r#"{"general": {"model": "Reserved probe pocket"},
            "tooling": {"reserved": [{"number": 99, "use": "probe"}]}}"#,
    )
    .expect("valid reservation machine");
    History::default().execute(
        &world,
        LoadMachine::new(&world, "reserved.mch".into(), Some(mounted)).expect("mount"),
    );
    let loaded = machine::of(&world);
    let saved = serde_json::to_value(project::collect(&world)).expect("save");
    assert_eq!(saved["version"], project::PROJECT_VERSION);
    assert!(
        saved["version"].as_u64().is_some_and(|v| v >= 78),
        "a v77 reader must refuse a file that stores reserved tools"
    );
    assert_eq!(
        saved["machine"]["machine"]["reserved_tools"],
        json!([{"number": 99, "use": "probe"}])
    );
    let reopened = new_world();
    project::open_noted(&reopened, &saved.to_string()).expect("reopen");
    assert_eq!(machine::of(&reopened), loaded);

    let mut future = saved;
    let next = project::NEWEST_PROJECT_VERSION + 1;
    future["version"] = json!(next);
    let error = project::open_noted(&reopened, &future.to_string()).expect_err("too new");
    assert!(
        matches!(error, project::ProjectError::UnsupportedVersion(v) if v == next),
        "{error}"
    );
    assert_eq!(machine::of(&reopened), loaded, "a refused file changes nothing");
}

#[test]
fn a_v77_machine_steps_up_with_no_reservation() {
    let legacy = json!({
        "version": 77, "entities": [], "machine": {
            "path": "legacy.mch", "machine": {
                "model": "Legacy mill", "spindle_max": 6000.0,
                "axes": [{"name": "X", "travel": [-40.0, 40.0]}]
            }
        }
    });
    let world = new_world();
    project::open_noted(&world, &legacy.to_string()).expect("existing v77 machine");
    let loaded = machine::of(&world);
    let mill = loaded.1.as_ref().expect("stored machine");
    assert_eq!(mill.model, "Legacy mill");
    assert_eq!(mill.spindle_max, Some(6000.0));
    assert!(mill.reserved_tools.is_empty(), "a v77 machine reserves nothing");
    let saved = serde_json::to_value(project::collect(&world)).expect("save");
    assert_eq!(saved["version"], project::PROJECT_VERSION);
    assert!(saved["machine"]["machine"].get("reserved_tools").is_none());
}
