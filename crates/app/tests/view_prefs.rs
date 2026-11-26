//! View preferences (issue #877): the theme and the colour-vision mode
//! are kept per user, survive a restart through the per-user file, and a
//! missing, corrupt or half-known file starts at the defaults with a
//! reason - never a crash. A pick is checked by name before it is set,
//! and the project file never carries either preference.
//!
//! Provenance: our own tests, specced from the issue's "Done means".
//!
//! #889: the tool-holder import choice rides the same file.
//! #1945: so does the AI Chat's offline-only switch - and never a key.
//! #1873: so does the optional path of the cadmpeg program.
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
    let chosen = ViewPrefs { theme: ThemeRef::Named("light".into()), color_vision: ColorVision::Tritan, ..ViewPrefs::default() };
    prefs::save(&path, &chosen).expect("save");
    let (back, note) = prefs::load(&path);
    assert_eq!(back, chosen);
    assert_eq!(note, None);
    // A theme file rides its path; Dark and Off write no key.
    let file = ViewPrefs { theme: ThemeRef::File("C:/themes/mine.json".into()), color_vision: ColorVision::Off, ..ViewPrefs::default() };
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
    assert_eq!(p, ViewPrefs { theme: ThemeRef::Dark, color_vision: ColorVision::Tritan, ..ViewPrefs::default() }, "the rest survives");
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

/// #1945: "Offline only" survives a restart (written only when on), and
/// the file has no place for an API key: with every preference set, its
/// keys are exactly the known ones and none names a key or a secret.
#[test]
fn offline_only_survives_and_the_file_stores_zero_keys() {
    let path = dir("ai").join(FILE_NAME);
    assert!(!ViewPrefs::default().to_json().contains("aiOfflineOnly"), "off is the absent key");
    let all = ViewPrefs {
        theme: ThemeRef::Named("light".into()),
        color_vision: ColorVision::Tritan,
        skip_tool_holders: true,
        ai_offline_only: true,
        cadmpeg_path: Some("/opt/CAD tools/cadmpeg".into()),
        check_for_updates: false,
        last_update_check_utc: Some(1_795_348_000),
    };
    prefs::save(&path, &all).expect("save");
    let (back, note) = prefs::load(&path);
    assert_eq!(note, None);
    assert_eq!(back, all, "every preference survives");
    let text = std::fs::read_to_string(&path).expect("read back");
    let v: serde_json::Value = serde_json::from_str(&text).expect("json");
    let mut keys: Vec<&str> = v.as_object().expect("object").keys().map(String::as_str).collect();
    keys.sort_unstable();
    assert_eq!(
        keys,
        [
            "aiOfflineOnly",
            "cadmpegPath",
            "checkForUpdates",
            "colorVision",
            "format",
            "importToolHolders",
            "lastUpdateCheckUtc",
            "theme",
        ]
    );
    let lower = text.to_lowercase();
    for word in ["key", "token", "secret", "password", "bearer"] {
        assert!(!lower.contains(word), "prefs.json names '{word}': {text}");
    }
}

/// #1873: the optional path of the cadmpeg program survives a restart,
/// is the absent key when not set, and a file written before the key
/// existed still loads. A blank text is "not set", never a path.
#[test]
fn cadmpeg_path_survives_a_restart_and_old_preferences_still_load() {
    assert_eq!(ViewPrefs::default().cadmpeg_path, None);
    assert!(!ViewPrefs::default().to_json().contains("cadmpegPath"), "not set is the absent key");
    let older = format!("{{\"format\":\"{FORMAT}\",\"colorVision\":\"tritan\"}}");
    let (p, notes) = ViewPrefs::from_json(&older).expect("an older file reads");
    assert_eq!((p.cadmpeg_path, notes.len()), (None, 0));
    let path = dir("cadmpeg").join(FILE_NAME);
    let set = ViewPrefs { cadmpeg_path: Some("/opt/CAD tools/cadmpeg".into()), ..ViewPrefs::default() };
    prefs::save(&path, &set).expect("save");
    let (back, note) = prefs::load(&path);
    assert_eq!(note, None);
    assert_eq!(back, set, "the path survives");
    let blank = format!("{{\"format\":\"{FORMAT}\",\"cadmpegPath\":\"  \"}}");
    let (p, _) = ViewPrefs::from_json(&blank).expect("a blank path reads");
    assert_eq!(p.cadmpeg_path, None, "a blank path is not set");
}

/// #2428: update check preference defaults to enabled (omitted from JSON),
/// disabled and timestamp values survive a restart, and a malformed timestamp
/// falls back without dropping theme/other preferences.
#[test]
fn update_check_preference_and_timestamp_survive_a_restart() {
    let def = ViewPrefs::default();
    assert!(def.check_for_updates);
    assert_eq!(def.last_update_check_utc, None);
    assert!(!def.to_json().contains("checkForUpdates"));
    assert!(!def.to_json().contains("lastUpdateCheckUtc"));

    let older = format!("{{\"format\":\"{FORMAT}\",\"colorVision\":\"tritan\"}}");
    let (p, notes) = ViewPrefs::from_json(&older).expect("an older file reads");
    assert!(p.check_for_updates);
    assert_eq!(p.last_update_check_utc, None);
    assert!(notes.is_empty());

    let path = dir("updates").join(FILE_NAME);
    let disabled = ViewPrefs {
        check_for_updates: false,
        last_update_check_utc: Some(1_795_348_123),
        ..ViewPrefs::default()
    };
    prefs::save(&path, &disabled).expect("save");
    let (back, note) = prefs::load(&path);
    assert_eq!(note, None);
    assert_eq!(back, disabled);

    let bad_ts = format!(
        "{{\"format\":\"{FORMAT}\",\"colorVision\":\"deutan\",\"checkForUpdates\":false,\"lastUpdateCheckUtc\":\"not-a-number\"}}"
    );
    let (recovered, notes) = ViewPrefs::from_json(&bad_ts).expect("partial recovery");
    assert_eq!(recovered.color_vision, ColorVision::Deutan);
    assert!(!recovered.check_for_updates);
    assert_eq!(recovered.last_update_check_utc, None);
    assert_eq!(notes.len(), 1);
}
