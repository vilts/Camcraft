//! View preferences (issue #877): the theme and the colour-vision mode
//! are kept per user, survive a restart through the per-user file, and a
//! missing, corrupt or half-known file starts at the defaults with a
//! reason - never a crash. A pick is checked by name before it is set,
//! and the project file never carries either preference.
//!
//! Provenance: our own tests, specced from the issue's "Done means".
//!
//! #889: the tool-holder import choice rides the same file.
//!
//! Gate map (M4.4): covers app::prefs, app::commands (theme, color_vision).

use std::path::PathBuf;

use camcraft_app::commands::{check_theme, color_vision_of, color_vision_set, theme_of};
use camcraft_app::prefs::{self, FILE_NAME, FORMAT, ViewPrefs};
use camcraft_app::schema::{ColorVision, ThemeRef};
use camcraft_app::world::new_world as world;

/// A fresh, empty folder for one test.
fn dir(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("camcraft_prefs_{tag}_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).expect("make the test folder");
    d
}

#[test]
fn the_preferences_survive_a_restart() {
    let path = dir("restart").join(FILE_NAME);
    let chosen = ViewPrefs { theme: ThemeRef::Named("light".into()), color_vision: ColorVision::Tritan, skip_tool_holders: false };
    prefs::save(&path, &chosen).expect("save");
    let (back, note) = prefs::load(&path);
    assert_eq!(back, chosen);
    assert_eq!(note, None);
    // A theme file rides its path; Dark and Off write no key.
    let file = ViewPrefs { theme: ThemeRef::File("C:/themes/mine.json".into()), color_vision: ColorVision::Off, skip_tool_holders: false };
    prefs::save(&path, &file).expect("save");
    assert_eq!(prefs::load(&path).0, file);
    prefs::save(&path, &ViewPrefs::default()).expect("save");
    let text = std::fs::read_to_string(&path).expect("written");
    assert!(text.contains(FORMAT) && !text.contains("theme") && !text.contains("colorVision"), "{text}");
}

#[test]
fn no_file_is_a_quiet_default_start() {
    let (p, note) = prefs::load(&dir("none").join(FILE_NAME));
    assert_eq!(p, ViewPrefs::default());
    assert_eq!(note, None, "a first run says nothing");
}

#[test]
fn a_corrupt_or_foreign_file_starts_at_the_defaults_and_says_why() {
    let d = dir("corrupt");
    for (name, text, why) in [
        ("broken.json", "{ not json", "not a preferences file"),
        ("foreign.json", r#"{"format": "camcraft-recent/1"}"#, "is not camcraft-prefs/1"),
    ] {
        let path = d.join(name);
        std::fs::write(&path, text).expect("write fixture");
        let (p, note) = prefs::load(&path);
        assert_eq!(p, ViewPrefs::default(), "{name}");
        let note = note.expect("a reason");
        assert!(note.contains(why) && note.contains("using the defaults"), "{note}");
    }
}

#[test]
fn an_unknown_value_falls_back_alone_and_is_named() {
    let path = dir("unknown").join(FILE_NAME);
    std::fs::write(
        &path,
        r#"{"format": "camcraft-prefs/1", "theme": {"named": "solarpunk"}, "colorVision": "tritan", "later": 1}"#,
    )
    .expect("write fixture");
    let (p, note) = prefs::load(&path);
    assert_eq!(p, ViewPrefs { theme: ThemeRef::Dark, color_vision: ColorVision::Tritan, skip_tool_holders: false }, "the rest survives");
    assert!(note.expect("named").contains("theme 'solarpunk' is not a built-in"));
    std::fs::write(&path, r#"{"format": "camcraft-prefs/1", "colorVision": "sepia"}"#).expect("write fixture");
    let (p, note) = prefs::load(&path);
    assert_eq!(p.color_vision, ColorVision::Off);
    assert!(note.expect("named").contains("colour-vision mode 'sepia' is not known"));
}

#[test]
fn a_pick_is_checked_by_name() {
    assert!(check_theme(&ThemeRef::Dark).is_ok());
    assert!(check_theme(&ThemeRef::Named("light".into())).is_ok());
    let stranger = check_theme(&ThemeRef::Named("solarpunk".into())).expect_err("not a built-in");
    assert!(stranger.contains("solarpunk"), "{stranger}");
    let gone = std::env::temp_dir().join(format!("camcraft-gone-{}.json", std::process::id()));
    let _ = std::fs::remove_file(&gone);
    let missing = check_theme(&ThemeRef::File(gone.display().to_string())).expect_err("no such file");
    assert!(missing.contains("camcraft-gone"), "{missing}");
    // Existence is all the app seam checks; the parse is the shell's.
    let real = dir("pick").join("t.json");
    std::fs::write(&real, "{}").expect("temp theme file");
    assert!(check_theme(&ThemeRef::File(real.display().to_string())).is_ok());
}

#[test]
fn the_project_file_never_carries_the_colour_vision_mode() {
    let w = world();
    color_vision_set(&w, ColorVision::Tritan);
    let path = dir("cvd").join("doc.camproj");
    camcraft_app::project::save(&w, &path).expect("save");
    let text = std::fs::read_to_string(&path).expect("read back");
    assert!(!text.to_lowercase().contains("vision"), "the project file never carries it");
    let elsewhere = world();
    camcraft_app::project::load(&elsewhere, &path).expect("open elsewhere");
    assert_eq!(color_vision_of(&elsewhere), ColorVision::Off, "a colleague's screen is not remapped");
    camcraft_app::project::load(&w, &path).expect("re-open here");
    assert_eq!(color_vision_of(&w), ColorVision::Tritan, "the viewer's mode outlives the document");
    assert_eq!(theme_of(&w), ThemeRef::Dark);
}

/// #889: the "Import tool holders" choice is the person's, kept here -
/// written only when off, so a file from before it (or with it on)
/// reads as on, and an off choice survives a restart.
#[test]
fn the_tool_holder_import_choice_survives_a_restart() {
    let on = ViewPrefs::default();
    assert!(!on.skip_tool_holders, "holders are imported by default");
    assert!(!on.to_json().contains("importToolHolders"), "on is the absent key: {}", on.to_json());
    let path = dir("holders").join(FILE_NAME);
    let off = ViewPrefs { skip_tool_holders: true, ..ViewPrefs::default() };
    prefs::save(&path, &off).expect("save");
    let (back, note) = prefs::load(&path);
    assert_eq!(note, None);
    assert!(back.skip_tool_holders, "off survives: {back:?}");
    let older = format!("{{\"format\":\"{FORMAT}\",\"colorVision\":\"tritan\"}}");
    let (p, _) = ViewPrefs::from_json(&older).expect("an older file reads");
    assert!(!p.skip_tool_holders, "a file without the key imports holders");
}
