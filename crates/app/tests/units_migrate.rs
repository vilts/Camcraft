//! #902, the v59 -> v60 rung: before v60 a document's unit was a
//! display lens and a post took its unit from the machine file, so an
//! older project opens on the unit its programs posted in - its posted
//! program is byte-identical - and says so when its display changes. A
//! file whose setups posted in different units keeps its display unit
//! and names each setup whose program unit changes.
//!
//! Provenance: our own persistence contract; no vendor material.
//!
//! Gate map (M4.4): covers app::project (units_migrate, apply's v60
//! rung, file PROJECT_VERSION).

use camcraft_app::commands::{CreateSetup, History, LoadMachine, SetSetupMachine, SetUnits};
use camcraft_app::project;
use camcraft_app::units::{self, Unit};
use camcraft_app::world::new_world;
use flecs_ecs::prelude::World;
use serde_json::Value;

/// `world` saved as an older (v59) file would have held it.
fn as_v59(world: &World) -> String {
    let mut v: Value = serde_json::to_value(project::collect(world)).expect("serializes");
    assert_eq!(v["version"], Value::from(project::PROJECT_VERSION), "this build writes v{}", project::PROJECT_VERSION);
    v["version"] = Value::from(59);
    v.to_string()
}

fn reopened(text: &str) -> (Unit, Vec<String>) {
    let w = new_world();
    let notes = project::open_noted(&w, text).expect("opens");
    (units::of(&w), notes)
}

fn tormach(world: &World, h: &mut History) {
    h.execute(world, LoadMachine::builtin(world, "Tormach 1100MX").expect("shipped inch machine"));
}

#[test]
fn an_older_file_on_an_inch_machine_opens_in_inch_and_says_so() {
    let w = new_world();
    let mut h = History::default();
    tormach(&w, &mut h);
    let (u, notes) = reopened(&as_v59(&w));
    assert_eq!(u, Unit::Inches, "it posted inch before - it must post inch now");
    assert_eq!(notes.len(), 1, "{notes:?}");
    assert!(notes[0].contains("displayed mm") && notes[0].contains("opens in inch"), "{notes:?}");
}

#[test]
fn an_older_inch_display_on_a_metric_machine_opens_in_mm() {
    let w = new_world();
    let mut h = History::default();
    h.execute(&w, SetUnits::new(&w, Unit::Inches).expect("to inch"));
    let (u, notes) = reopened(&as_v59(&w));
    assert_eq!(u, Unit::Millimeters, "it posted mm before - it must post mm now");
    assert!(notes.iter().any(|n| n.contains("displayed inch") && n.contains("opens in mm")), "{notes:?}");
}

#[test]
fn an_older_file_whose_display_matched_opens_silently() {
    for inch in [false, true] {
        let w = new_world();
        let mut h = History::default();
        if inch {
            tormach(&w, &mut h);
            h.execute(&w, SetUnits::new(&w, Unit::Inches).expect("to inch"));
        }
        let (u, notes) = reopened(&as_v59(&w));
        assert_eq!(u == Unit::Inches, inch);
        assert!(notes.is_empty(), "{notes:?}");
    }
}

#[test]
fn an_older_file_with_setups_in_both_units_keeps_its_display_and_names_the_setup() {
    let w = new_world();
    let mut h = History::default();
    h.execute(&w, CreateSetup::new(&w, None, None).expect("Setup1"));
    h.execute(&w, CreateSetup::new(&w, None, Some("Setup1")).expect("Setup2"));
    h.execute(&w, SetSetupMachine::builtin(&w, "Setup2", "Tormach 1100MX").expect("Setup2 on an inch machine"));
    let (u, notes) = reopened(&as_v59(&w));
    assert_eq!(u, Unit::Millimeters, "the display unit stands");
    assert_eq!(notes.len(), 1, "{notes:?}");
    assert!(notes[0].contains("setup 'Setup2' posted in inch") && notes[0].contains("document's unit (mm)"), "{notes:?}");
}

#[test]
fn a_v60_file_keeps_its_unit_whatever_the_machine() {
    let w = new_world();
    let mut h = History::default();
    tormach(&w, &mut h); // an inch machine file, an mm document
    assert_eq!(camcraft_app::posting::program_options(&w)["Units"], "mm", "the machine file does not choose");
    h.execute(&w, SetUnits::new(&w, Unit::Inches).expect("to inch"));
    assert_eq!(camcraft_app::posting::program_options(&w)["Units"], "inch", "the document does");
    h.undo(&w);
    let text = serde_json::to_string(&project::collect(&w)).expect("serializes");
    let (u, notes) = reopened(&text);
    assert_eq!(u, Unit::Millimeters, "v60: the document's unit, never the machine file's");
    assert!(notes.is_empty(), "{notes:?}");
}
