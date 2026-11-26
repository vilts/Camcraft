//! Installation data across the machine catalogue and saved document (#3181), from Camcraft's .mch grammar.
//! Gate map (M4.4): covers app::machine, app::posting, app::project.

use camcraft_app::{
    commands::{History, LoadMachine},
    machine::{
        self,
        library::{MachineEntry, MachineSource},
    },
    project,
    world::new_world,
};
use serde_json::{Value, json};

const NAME: &str = "Fehlmann Picomax 55 (iTNC 530)";
const PICOMAX: &str = include_str!("../../../assets/machines/fehlmann-picomax55.mch");

#[test]
fn machine_sheet_reports_known_options_and_distinguishes_an_unstated_list() {
    for (options, expected) in [
        (None, None),
        (Some(json!([])), Some("None")),
        (Some(json!(["A", "B"])), Some("A, B")),
    ] {
        let mut file: Value = serde_json::from_str(PICOMAX).unwrap();
        if let Some(options) = options {
            file["controller"]["identity"]["installed_options"] = options;
        }
        let entry = MachineEntry::from_text(NAME, MachineSource::Builtin(NAME), &file.to_string());
        assert!(entry.is_valid());
        let general = entry.groups.iter().find(|g| g.title == "General").unwrap();
        let row = |name| {
            general
                .rows
                .iter()
                .find(|(k, _)| k == name)
                .map(|(_, v)| v.as_str())
        };
        assert_eq!(row("Control"), Some("iTNC 530"));
        assert_eq!(row("NC software"), Some("340490-03"));
        assert_eq!(row("Installed control options"), expected);
        assert_eq!(
            row("Tool change"),
            Some("Manual at the physical upper change position")
        );
    }
}

#[test]
fn mounting_saving_and_reopening_the_picomax_preserves_its_installation() {
    let world = new_world();
    History::default().execute(&world, LoadMachine::builtin(&world, NAME).unwrap());
    let loaded = machine::of(&world);
    let m = loaded.1.as_ref().unwrap();
    assert_eq!(m.axes.len(), 3);
    assert!(m.axes.iter().all(|a| a.travel.is_none()));
    assert_eq!(
        (m.spindle_min, m.spindle_max, m.spindles),
        (Some(50.0), Some(9000.0), Some(1))
    );
    let saved = serde_json::to_value(project::collect(&world)).unwrap();
    assert_eq!(saved["version"], project::PROJECT_VERSION);
    assert!(
        saved["version"].as_u64().unwrap() >= 75,
        "a v74 reader must refuse this save"
    );
    let record = &saved["machine"]["machine"];
    assert_eq!(
        record["controller_identity"],
        json!({"model":"iTNC 530","nc_software":"340490-03"})
    );
    assert_eq!(record["tool_change"], "manual-retract-top");
    let reopened = new_world();
    project::open_noted(&reopened, &saved.to_string()).unwrap();
    assert_eq!(machine::of(&reopened), loaded);
    let mut malformed = saved;
    malformed["machine"]["machine"]["controller_identity"]["installed_options"] = json!(["a", "a"]);
    assert!(project::open_noted(&reopened, &malformed.to_string()).is_err());
    assert_eq!(machine::of(&reopened), loaded);
}

#[test]
fn a_v74_machine_opens_with_unstated_installation_facts_and_a_future_file_preserves_it() {
    let legacy = json!({
        "version": 74, "entities": [], "machine": {
            "path": "legacy.mch", "machine": {
                "model": "Legacy mill", "spindle_max": 6000.0,
                "axes": [{"name": "X", "travel": [-40.0, 40.0]}]
            }
        }
    });
    let world = new_world();
    project::open_noted(&world, &legacy.to_string()).expect("existing v74 machine");
    let loaded = machine::of(&world);
    assert_eq!(loaded.0, "legacy.mch");
    let mill = loaded.1.as_ref().unwrap();
    assert_eq!(mill.model, "Legacy mill");
    assert_eq!(mill.spindle_max, Some(6000.0));
    assert_eq!(mill.axes[0].travel, Some((-40.0, 40.0)));
    assert!(mill.controller_identity.is_none());
    assert!(mill.tool_change.is_none());

    let mut saved = serde_json::to_value(project::collect(&world)).unwrap();
    assert_eq!(saved["version"], project::PROJECT_VERSION);
    let record = &saved["machine"]["machine"];
    assert!(record.get("controller_identity").is_none());
    assert!(record.get("tool_change").is_none());

    let future = project::NEWEST_PROJECT_VERSION + 1;
    saved["version"] = json!(future);
    let error =
        project::open_noted(&world, &saved.to_string()).expect_err("too new for this reader");
    assert!(
        matches!(error, project::ProjectError::UnsupportedVersion(v) if v == future),
        "{error}"
    );
    assert_eq!(machine::of(&world), loaded);
}
