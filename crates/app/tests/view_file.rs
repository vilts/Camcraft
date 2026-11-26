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
//! #2958: the line width and the anti-aliasing ride the same file.
//!
//! Gate map (M4.4): covers app::view.

use std::path::{Path, PathBuf};

use camcraft_app::recent;
use camcraft_app::view::{self, Antialias, LineWidth, ViewPrefs, FILE_NAME, FORMAT};

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
    view::save(&file, &ViewPrefs { show_gizmo_names: false, ..ViewPrefs::default() }).expect("save creates the file");
    let (prefs, note) = view::load(&file);
    assert!(!prefs.show_gizmo_names, "the restart reads the choice back");
    assert_eq!(note, None, "a good file says nothing");
    view::save(&file, &ViewPrefs { show_gizmo_names: true, ..ViewPrefs::default() }).expect("flip back");
    assert!(view::load(&file).0.show_gizmo_names, "and back on survives too");
}

/// #2818: the display mode and the grid switch ride the same file. An old
/// file without the keys loads the defaults and says nothing.
#[test]
fn the_display_mode_and_the_grid_round_trip_and_an_old_file_loads_the_defaults() {
    let d = dir("display");
    let file = file_in(&d);
    let chosen = ViewPrefs { show_gizmo_names: true, display: "wireframe".into(), show_grid: false, ..ViewPrefs::default() };
    view::save(&file, &chosen).expect("save");
    assert_eq!(view::load(&file), (chosen, None), "the choice reads back");
    std::fs::write(&file, format!(r#"{{"format":"{FORMAT}","view":{{"show_gizmo_names":false}}}}"#))
        .expect("write the old file");
    let (old, note) = view::load(&file);
    assert_eq!(old.display, view::DEFAULT_DISPLAY, "an old file loads the default mode");
    assert!(old.show_grid, "and the grid that it always had");
    assert!(!old.show_gizmo_names, "the key that it has still reads");
    assert_eq!(note, None, "an old file is not a complaint");
    assert_eq!(ViewPrefs::default().display, view::DEFAULT_DISPLAY);
    assert!(ViewPrefs::default().show_grid);
}

/// #2834: the ground shadow switch and the ambient occlusion switch ride the
/// same file. An old file without the keys loads them off and says nothing.
#[test]
fn the_shading_switches_round_trip_and_an_old_file_loads_them_off() {
    let d = dir("shading");
    let file = file_in(&d);
    for (shadow, occlusion) in [(true, false), (false, true), (true, true)] {
        let chosen = ViewPrefs { ground_shadow: shadow, ambient_occlusion: occlusion, ..ViewPrefs::default() };
        view::save(&file, &chosen).expect("save");
        assert_eq!(view::load(&file), (chosen, None), "the choice reads back");
    }
    std::fs::write(&file, format!(r#"{{"format":"{FORMAT}","view":{{"display":"shaded","show_grid":false}}}}"#))
        .expect("write the old file");
    let (old, note) = view::load(&file);
    assert!(!old.ground_shadow && !old.ambient_occlusion, "an old file loads the two switches off");
    assert_eq!((old.display.as_str(), old.show_grid), ("shaded", false), "the keys that it has still read");
    assert_eq!(note, None, "an old file is not a complaint");
    let fresh = ViewPrefs::default();
    assert!(!fresh.ground_shadow && !fresh.ambient_occlusion, "a new user starts with the view of #2818");
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
    view::save(&file, &ViewPrefs { show_gizmo_names: false, ..ViewPrefs::default() }).expect("save under the override");
    assert!(!view::load(&file).0.show_gizmo_names, "the restart reads it back from there");
}

/// #2958: the line width and the anti-aliasing round-trip through the
/// file. The defaults are 1.5 pt and 4x, a file from before #2958 reads
/// as those defaults, and a word this build does not know is that one
/// setting's default - the other choices in the file survive.
#[test]
fn the_line_width_and_the_antialias_round_trip() {
    let d = ViewPrefs::default();
    assert_eq!((d.line_width, d.antialias), (LineWidth::Normal, Antialias::X4));
    assert!((LineWidth::Normal.points() - 1.5).abs() < f32::EPSILON);
    let file = file_in(&dir("lines"));
    for (w, a) in [(LineWidth::Thin, Antialias::Off), (LineWidth::Bold, Antialias::X2), (LineWidth::Heavy, Antialias::X4)] {
        let chosen = ViewPrefs { line_width: w, antialias: a, ..ViewPrefs::default() };
        view::save(&file, &chosen).expect("save");
        assert_eq!(view::load(&file), (chosen, None), "{w:?} / {a:?} survive");
    }
    let older = format!(r#"{{"format":"{FORMAT}","view":{{"show_gizmo_names":false}}}}"#);
    std::fs::write(&file, older).expect("write an older file");
    let (p, note) = view::load(&file);
    assert_eq!(note, None);
    assert_eq!(p, ViewPrefs { show_gizmo_names: false, ..ViewPrefs::default() }, "an older file reads the defaults");
    let odd = format!(r#"{{"format":"{FORMAT}","view":{{"show_gizmo_names":false,"line_width":"7","antialias":16}}}}"#);
    std::fs::write(&file, odd).expect("write a newer file");
    let (p, note) = view::load(&file);
    assert_eq!(note, None, "an unknown word does not refuse the file");
    assert!(!p.show_gizmo_names, "the other choice survives");
    assert_eq!((p.line_width, p.antialias), (LineWidth::Normal, Antialias::X4));
}
