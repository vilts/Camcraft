//! #31: what the browser's async file picker hands over - a name and the
//! file's bytes - lands through the same commands as a file opened from
//! disk, and a failed read or a refused file is said by name. Run natively:
//! the picker's DOM half is wasm32-only, but everything after the bytes
//! arrive is this shared code.
//!
//! Gate map (M4.4): covers ui::file_pick, ui::state (state_picked).
//! Provenance: our own tests.

use std::path::{Path, PathBuf};

use camcraft_app::{model, project};

use crate::file_dialog::FileDialog;
use crate::file_pick::{self, PickFor, PickedFile};
use crate::state::{CommandKind, State};

fn testdata(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../testdata/models").join(name)
}

fn picked(purpose: PickFor, name: &str, bytes: Result<Vec<u8>, String>) -> PickedFile {
    PickedFile { purpose, name: name.to_string(), bytes }
}

fn has_model(state: &State, name: &str) -> bool {
    model::find_by_name(&state.world, name).is_some()
}

#[test]
fn a_picked_model_imports_through_the_undoable_command() {
    let mut state = State::new();
    let bytes = std::fs::read(testdata("cube.stl")).expect("cube.stl");
    file_pick::deliver(picked(PickFor::Model, "cube.stl", Ok(bytes)));
    state.drain_actions();
    assert!(has_model(&state, "cube"), "status: {}", state.status);
    assert!(state.status.starts_with("imported cube"), "status: {}", state.status);
    assert!(file_pick::take_picked().is_empty(), "the inbox is drained");
    // One undo step takes it away, like an import from disk.
    state.run(CommandKind::Undo);
    assert!(!has_model(&state, "cube"));
    state.run(CommandKind::Redo);
    assert!(has_model(&state, "cube"));
}

#[test]
fn a_picked_file_that_cannot_be_used_is_refused_by_name() {
    let mut state = State::new();
    file_pick::deliver(picked(PickFor::Model, "big.stl", Err("over the limit".into())));
    state.drain_actions();
    assert_eq!(state.status, "import refused: big.stl could not be read: over the limit");
    file_pick::deliver(picked(PickFor::Model, "part.igs", Ok(b"anything".to_vec())));
    state.drain_actions();
    assert!(state.status.starts_with("import refused:") && state.status.contains("IGS"), "{}", state.status);
    file_pick::deliver(picked(PickFor::Model, "bad.stl", Ok(vec![0xff; 97])));
    state.drain_actions();
    assert!(state.status.starts_with("import refused:"), "{}", state.status);
    file_pick::deliver(picked(PickFor::Project, "p.camproj", Err("the browser could not read it".into())));
    state.drain_actions();
    assert_eq!(state.status, "open failed: p.camproj could not be read: the browser could not read it");
    assert!(model::find_by_name(&state.world, "big").is_none());
    assert!(!state.history.is_dirty(), "nothing reached the history");
}

#[test]
fn a_picked_project_opens_in_place_of_the_document() {
    // A saved project with one model in it.
    let mut author = State::new();
    author.run(CommandKind::ImportFile(testdata("cube.stl")));
    while author.import_in_progress() {
        author.drain_actions();
        std::thread::yield_now();
    }
    assert!(has_model(&author, "cube"), "{}", author.status);
    let dir = std::env::temp_dir().join(format!("camcraft-pick-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("temp dir");
    let file = dir.join("job.camproj");
    project::save(&author.world, &file).expect("save");
    let bytes = std::fs::read(&file).expect("read back");
    let _ = std::fs::remove_dir_all(&dir);

    let mut state = State::new();
    let other = std::fs::read(testdata("box.obj")).expect("box.obj");
    file_pick::deliver(picked(PickFor::Model, "box.obj", Ok(other)));
    state.drain_actions();
    assert!(has_model(&state, "box"));
    file_pick::deliver(picked(PickFor::Project, "job.camproj", Ok(bytes)));
    state.drain_actions();
    assert_eq!(state.status, "opened job.camproj");
    assert!(has_model(&state, "cube"), "the project's model is there");
    assert!(!has_model(&state, "box"), "the old document is gone");
    assert!(!state.history.is_dirty(), "an opened project is the clean point");

    // Not text, and text that is no project: refused by name.
    file_pick::deliver(picked(PickFor::Project, "x.camproj", Ok(vec![0xff, 0xfe, 0x00])));
    state.drain_actions();
    assert_eq!(state.status, "open failed: x.camproj is not a CamCraft project (not text)");
    assert!(has_model(&state, "cube"), "a non-text file changes nothing");
    file_pick::deliver(picked(PickFor::Project, "y.camproj", Ok(b"{ not json".to_vec())));
    state.drain_actions();
    assert!(state.status.starts_with("open failed: y.camproj:"), "{}", state.status);
}

#[test]
fn the_browser_picker_accepts_the_dialogs_extensions() {
    let mesh = FileDialog::new().add_filter("Mesh", &["stl", "obj", "step", "stp"]);
    assert_eq!(file_pick::accept_attr(&mesh), ".stl,.obj,.step,.stp");
    let messy = FileDialog::new().add_filter("A", &[".STL", "obj"]).add_filter("B", &["stl", " "]);
    assert_eq!(file_pick::accept_attr(&messy), ".stl,.obj");
    assert_eq!(file_pick::accept_attr(&FileDialog::new()), "");
}

#[test]
fn a_file_over_the_browser_read_limit_is_refused_before_it_is_read() {
    let limit = (file_pick::BROWSER_READ_LIMIT) as f64;
    assert_eq!(file_pick::too_big("ok.stl", limit), None);
    let why = file_pick::too_big("huge.step", limit + 1.0).expect("refused");
    assert!(why.starts_with("huge.step is 256 MiB, over the browser build's 256 MiB read limit"), "{why}");
    assert!(file_pick::too_big("nan.stl", f64::NAN).is_some());
    // The inbox's Debug never prints the bytes themselves.
    let shown = format!("{:?}", picked(PickFor::Model, "m.stl", Ok(vec![7; 1000])));
    assert!(shown.contains("1000 bytes") && !shown.contains("7, 7"), "{shown}");
}

#[test]
fn a_chooser_asked_for_by_a_click_opens_inside_that_click() {
    // Safari opens a file chooser only while the user's click is handled:
    // the actions that open one run at the end of the clicked frame
    // (still in eframe's pointerup handler), not on the next one.
    use crate::unsaved::UnsavedChoice;
    for opens in [
        CommandKind::OpenModelDialog,
        CommandKind::OpenProjectDialog,
        CommandKind::ResolveUnsaved(UnsavedChoice::DontSave),
    ] {
        assert!(crate::state::opens_a_chooser(&opens), "{opens:?}");
    }
    assert!(!crate::state::opens_a_chooser(&CommandKind::NewDocument));
    assert!(!crate::state::opens_a_chooser(&CommandKind::Undo));

    let mut state = State::new();
    // Nothing that opens a chooser: left for the next frame's drain.
    state.pending.push_back(CommandKind::NewDocument);
    assert!(!state.run_click_actions(true));
    assert_eq!(state.pending.len(), 1);
    // A chooser action outside a click (a shortcut): also left alone.
    state.pending.push_back(CommandKind::ResolveUnsaved(UnsavedChoice::Cancel));
    assert!(!state.run_click_actions(false));
    assert_eq!(state.pending.len(), 2);
    // Inside the click: the whole queue runs now, in order.
    assert!(state.run_click_actions(true));
    assert!(state.pending.is_empty());
    assert_eq!(state.status, "new document");
    assert!(!state.picker_in_click, "the flag lives only while the click's queue runs");

    // Open project over unsaved changes: the prompt is asked inside the
    // click, so its Don't Save (another click) can open the chooser.
    let bytes = std::fs::read(testdata("cube.stl")).expect("cube.stl");
    file_pick::deliver(picked(PickFor::Model, "cube.stl", Ok(bytes)));
    state.drain_actions();
    assert!(state.history.is_dirty(), "{}", state.status);
    state.pending.push_back(CommandKind::OpenProjectDialog);
    assert!(state.run_click_actions(true));
    assert!(matches!(state.unsaved_prompt, Some(CommandKind::OpenProjectDialog)));
    assert_eq!(state.status, "unsaved changes");
}

#[test]
fn a_chooser_outside_a_click_says_what_to_do_instead_of_failing_silently() {
    assert_eq!(file_pick::chooser_status(PickFor::Model, true), "choose a model to import…");
    assert_eq!(file_pick::chooser_status(PickFor::Project, true), "choose a project to open…");
    let model = file_pick::chooser_status(PickFor::Model, false);
    assert!(
        model.starts_with("choose a model to import…") && model.contains("Safari") && model.contains("File > Model"),
        "{model}"
    );
    let project = file_pick::chooser_status(PickFor::Project, false);
    assert!(project.contains("File > Open"), "{project}");
}

// #1613: both ways a project reaches the shell must keep the old document
// even if the loader gets past parsing and starts applying the new file.
fn refused_project_keeps_document(from_disk: bool) {
    use camcraft_app::commands::SetUnits;
    use camcraft_app::units::{self, Unit};

    let mut incoming: serde_json::Value =
        serde_json::from_str(&project::save_text(&State::new().world).expect("empty project")).expect("JSON");
    incoming["units"] = serde_json::json!("in");
    incoming["entities"] = serde_json::json!([{
        "name": "BrokenSetup", "translation": [0.0, 0.0, 0.0], "kind": "setup",
        "setup": {"stock": "Stock", "wcs": "WCS", "workholding_max_rpm": -1.0}
    }]);
    let late_failure = serde_json::to_vec(&incoming).expect("incoming bytes");
    // This is a semantic failure after document singleton writes, rather
    // than a second spelling of a JSON parse error.
    let probe = State::new();
    let error = project::open_noted(&probe.world, std::str::from_utf8(&late_failure).expect("text"))
        .expect_err("invalid workholding rpm");
    assert!(error.to_string().contains("BrokenSetup"), "{error}");
    assert_eq!(units::of(&probe.world), Unit::Inches, "the loader reached world writes");

    for (case, bytes) in [("json", b"{ not json".to_vec()), ("semantic", late_failure)] {
        let mut state = State::new();
        let model = std::fs::read(testdata("cube.stl")).expect("cube");
        state.run(CommandKind::OpenPicked(picked(PickFor::Model, "cube.stl", Ok(model))));
        assert!(has_model(&state, "cube"), "{}", state.status);
        let command = SetUnits::new(&state.world, Unit::Inches).expect("change units");
        state.history.execute(&state.world, command);
        assert!(state.history.undo(&state.world));
        state.project_path = Some(PathBuf::from("original.camproj"));
        state.hidden_stocks.push("Stock".into());
        let before = project::save_text(&state.world).expect("old document");
        let title = state.window_title();
        let version = state.doc_version;
        assert!(state.history.is_dirty());
        assert!(state.history.can_undo() && state.history.can_redo());

        let name = format!("refused-{case}.camproj");
        let dir = std::env::temp_dir().join(format!("camcraft-1613-{}-{case}", std::process::id()));
        if from_disk {
            std::fs::create_dir_all(&dir).expect("temp dir");
            let path = dir.join(&name);
            std::fs::write(&path, bytes).expect("corrupt project");
            state.run(CommandKind::OpenProjectFile(path));
            std::fs::remove_dir_all(&dir).expect("remove temp dir");
        } else {
            state.run(CommandKind::OpenPicked(picked(PickFor::Project, &name, Ok(bytes))));
        }

        assert_eq!(project::save_text(&state.world).expect("document after refusal"), before);
        assert!(state.status.starts_with("open failed:") && state.status.contains(&name), "{}", state.status);
        assert_eq!(state.project_path, Some(PathBuf::from("original.camproj")));
        assert_eq!(state.window_title(), title);
        assert_eq!(state.doc_version, version);
        assert_eq!(state.hidden_stocks, ["Stock"]);
        assert!(state.history.is_dirty());
        assert!(state.history.can_undo() && state.history.can_redo());
        assert!(state.history.redo(&state.world), "redo command survived");
        assert_eq!(units::of(&state.world), Unit::Inches);
        assert!(state.history.undo(&state.world));
        assert_eq!(project::save_text(&state.world).expect("redo reversed"), before);
        assert!(state.history.undo(&state.world), "import command survived");
        assert!(!has_model(&state, "cube"));
        assert!(!state.history.is_dirty(), "original clean point survived");
        assert!(state.history.redo(&state.world));
        assert_eq!(project::save_text(&state.world).expect("import redone"), before);
    }
}

#[test]
fn refused_disk_project_keeps_document_and_history() {
    refused_project_keeps_document(true);
}

#[test]
fn refused_byte_project_keeps_document_and_history() {
    refused_project_keeps_document(false);
}

#[test]
fn successful_disk_project_replaces_document_and_keeps_preferences() {
    use camcraft_app::commands::{color_vision_of, color_vision_set, theme_of, theme_set};
    use camcraft_app::schema::{ColorVision, ThemeRef};

    let dir = std::env::temp_dir().join(format!("camcraft-1613-success-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("temp dir");
    let path = dir.join("replacement.camproj");
    let mut author = State::new();
    let bytes = std::fs::read(testdata("cube.stl")).expect("cube");
    author.run(CommandKind::OpenPicked(picked(PickFor::Model, "cube.stl", Ok(bytes))));
    project::save(&author.world, &path).expect("save new document");

    let mut state = State::new();
    let bytes = std::fs::read(testdata("box.obj")).expect("box");
    state.run(CommandKind::OpenPicked(picked(PickFor::Model, "box.obj", Ok(bytes))));
    assert!(has_model(&state, "box"));
    theme_set(&state.world, &ThemeRef::Named("light".into()));
    color_vision_set(&state.world, ColorVision::Deutan);
    let version = state.doc_version;
    state.run(CommandKind::OpenProjectFile(path.clone()));
    assert!(state.status.starts_with("opened "), "{}", state.status);
    assert!(has_model(&state, "cube"));
    assert!(!has_model(&state, "box"));
    assert_eq!(state.project_path, Some(path));
    assert!(state.history.is_empty());
    assert!(!state.history.is_dirty());
    assert_eq!(state.doc_version, version + 1);
    assert_eq!(theme_of(&state.world), ThemeRef::Named("light".into()));
    assert_eq!(color_vision_of(&state.world), ColorVision::Deutan);
    std::fs::remove_dir_all(dir).expect("remove temp dir");
}
