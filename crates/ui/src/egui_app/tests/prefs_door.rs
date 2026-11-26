//! #877: the theme and the colour-vision mode are the user's
//! preferences, not the document's. A pick is written to the per-user
//! file at once, survives a restart, is not an unsaved change, and
//! neither File > New nor opening a project (even an older one that
//! still names a theme) changes it. A corrupt file starts at the
//! defaults with a status note. The tests point the store at a temp
//! folder: a headless State never touches the user's real file.
//! Provenance: our own tests over our own shell; no HSMWorks material.
//!
//! Gate map (M4.4): covers ui::state (state_prefs).

use std::path::PathBuf;

use camcraft_app::commands::{color_vision_of, theme_of};
use camcraft_app::prefs::{self, FILE_NAME};
use camcraft_app::schema::{ColorVision, ThemeRef};

use crate::state::{CommandKind, State};

fn temp_dir(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("camcraft-prefs-ui-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).expect("make the test folder");
    d
}

fn run(state: &mut State, k: CommandKind) {
    state.pending.push_back(k);
    state.drain_actions();
}

fn light() -> ThemeRef {
    ThemeRef::Named("light".into())
}

#[test]
fn a_pick_survives_new_open_and_a_restart() {
    let d = temp_dir("keep");
    let store = d.join(FILE_NAME);
    let mut state = State::new();
    state.load_prefs_from(store.clone());
    assert_eq!(theme_of(&state.world), ThemeRef::Dark, "no file yet: the default");
    run(&mut state, CommandKind::SelectTheme(light()));
    run(&mut state, CommandKind::SelectColorVision(ColorVision::Deutan));
    let (saved, _) = prefs::load(&store);
    assert_eq!((saved.theme, saved.color_vision), (light(), ColorVision::Deutan), "written at the pick");
    assert!(!state.history.is_dirty(), "a preference is not an unsaved change");

    run(&mut state, CommandKind::NewDocument);
    assert_eq!(theme_of(&state.world), light(), "File > New keeps the theme");
    let old = d.join("old.camproj");
    std::fs::write(&old, r#"{"version": 41, "theme": {"named": "dark"}, "entities": []}"#).expect("old project");
    run(&mut state, CommandKind::OpenProjectFile(old));
    assert_eq!(theme_of(&state.world), light(), "an older file's theme key does not win");
    assert_eq!(color_vision_of(&state.world), ColorVision::Deutan);

    let mut restarted = State::new();
    restarted.load_prefs_from(store);
    assert_eq!(theme_of(&restarted.world), light(), "the theme survives a restart");
    assert_eq!(color_vision_of(&restarted.world), ColorVision::Deutan, "so does the colour-vision mode");
}

#[test]
fn a_corrupt_preferences_file_starts_dark_and_says_why() {
    let store = temp_dir("corrupt").join(FILE_NAME);
    std::fs::write(&store, "{ not json").expect("corrupt file");
    let mut state = State::new();
    state.load_prefs_from(store);
    assert_eq!(theme_of(&state.world), ThemeRef::Dark);
    assert_eq!(color_vision_of(&state.world), ColorVision::Off);
    assert!(state.status.contains("preferences ignored"), "{}", state.status);
}

#[test]
fn a_headless_state_keeps_its_picks_in_memory() {
    let mut state = State::new();
    run(&mut state, CommandKind::SelectTheme(light()));
    assert_eq!(theme_of(&state.world), light());
    assert!(state.prefs_path().is_none(), "no store: the user's file is never written by a test");
}

/// #1873: no control sets the cadmpeg path yet, so a pick that writes the
/// preferences keeps the path that the file states.
#[test]
fn a_pick_keeps_the_stated_cadmpeg_path() {
    let store = temp_dir("cadmpeg").join(FILE_NAME);
    let stated = prefs::ViewPrefs { cadmpeg_path: Some("/opt/cad/cadmpeg".into()), ..prefs::ViewPrefs::default() };
    prefs::save(&store, &stated).expect("a file with the path");
    let mut state = State::new();
    state.load_prefs_from(store.clone());
    run(&mut state, CommandKind::SelectTheme(light()));
    let (saved, note) = prefs::load(&store);
    assert_eq!(note, None);
    assert_eq!(saved.theme, light(), "the pick is written");
    assert_eq!(saved.cadmpeg_path.as_deref(), Some("/opt/cad/cadmpeg"), "the path is still there");
}
