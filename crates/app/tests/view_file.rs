//! View preferences (issue #645, the toggle asked on top of #727):
//! the per-user `view.json` round-trips a flipped flag through a
//! restart, a missing file is a quiet default start, a corrupt or
//! foreign-format file starts at the defaults with a reason - never a
//! crash - and unknown keys stay ignored so newer builds keep working.
//! The default is names ON: shipping the switch must not change a
//! viewport a user already has.
//!
//! Provenance: our own tests, specced from #645's review ask.
//!
//! Gate map (M4.4): covers app::view.

use std::path::{Path, PathBuf};

use camcraft_app::recent;
use camcraft_app::view::{self, ViewPrefs, FILE_NAME, FORMAT};

/// A fresh, empty folder for one test.
fn dir(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("camcraft_view_{tag}_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).expect("make the test folder");
    // Canonical, so comparisons hold where temp is a symlink (macOS /var).
    std::fs::canonicalize(&d).expect("canonical test folder")
}

/// `dir/tag`'s view file, folder created, file not necessarily present.
fn file_in(d: &Path) -> PathBuf {
    d.join(FILE_NAME)
}

#[test]
fn a_flipped_choice_round_trips_through_the_file() {
    let d = dir("roundtrip");
    let file = file_in(&d);
    assert_eq!(view::load(&file), (ViewPrefs::default(), None), "no file yet: quiet defaults");
    view::save(&file, &ViewPrefs { show_gizmo_names: false }).expect("save creates the file");
    let (prefs, note) = view::load(&file);
    assert!(!prefs.show_gizmo_names, "the restart reads the choice back");
    assert_eq!(note, None, "a good file says nothing");
    view::save(&file, &ViewPrefs { show_gizmo_names: true }).expect("flip back");
    assert!(view::load(&file).0.show_gizmo_names, "and back on survives too");
}

#[test]
fn a_corrupt_or_foreign_file_starts_at_the_defaults_and_says_why() {
    let d = dir("corrupt");
    let file = file_in(&d);
    for (text, wants) in [
        ("{ not json", "not a view file"),
        (r#"{"format":"camcraft-recent/1","view":{"show_gizmo_names":false}}"#, FORMAT),
    ] {
        std::fs::write(&file, text).expect("write the bad file");
        let (prefs, note) = view::load(&file);
        assert_eq!(prefs, ViewPrefs::default(), "a bad file starts at the defaults: {text}");
        let note = note.expect("and says so");
        assert!(note.contains(wants), "the reason names the trouble ({wants}): {note}");
    }
}

#[test]
fn unknown_keys_and_missing_sections_still_read() {
    let d = dir("tolerant");
    let file = file_in(&d);
    // A newer build's extra key must not break this one's read.
    std::fs::write(
        &file,
        r#"{"format":"camcraft-view/1","view":{"show_gizmo_names":false,"sound_off":true}}"#,
    )
    .expect("write the newer file");
    assert!(!view::load(&file).0.show_gizmo_names, "the key it knows still reads");
    // A whole missing `view` section is the all-defaults view, quietly.
    std::fs::write(&file, format!(r#"{{"format":"{FORMAT}"}}"#)).expect("write the thin file");
    let (prefs, note) = view::load(&file);
    assert_eq!(prefs, ViewPrefs::default(), "missing sections default");
    assert_eq!(note, None, "and that is not a complaint");
}

#[test]
fn defaults_wear_the_seventy_twenty_seven_decision() {
    // #727 drew the names for everyone; #645's switch ships ON so no
    // existing viewport changes on the update.
    assert!(ViewPrefs::default().show_gizmo_names);
    let d = dir("defaults");
    assert!(view::load(&file_in(&d)).0.show_gizmo_names);
}

#[test]
fn the_config_dir_override_houses_the_view_file() {
    // The re-review's note: the persistence claim names CAMCRAFT_CONFIG_DIR,
    // so the override gets its own gate. The folder resolver is the
    // recents' (#662) and view.rs hangs FILE_NAME on it (`store_path`),
    // so an absolute override must be the folder the choice round-trips
    // through. Fake getter, as `recent_files`' resolver test does -
    // edition 2024 keeps process env writes out of tests.
    let d = dir("override");
    let over =
        |k: &str| (k == recent::ENV_VAR).then(|| std::ffi::OsString::from(d.as_os_str()));
    let folder = recent::config_dir_for(std::env::consts::OS, over)
        .expect("the override names a folder on every platform");
    assert_eq!(folder, d, "the absolute override wins over any home folder");
    let file = file_in(&folder);
    view::save(&file, &ViewPrefs { show_gizmo_names: false }).expect("save under the override");
    assert!(!view::load(&file).0.show_gizmo_names, "the restart reads it back from there");
}
