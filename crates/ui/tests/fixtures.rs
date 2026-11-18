//! The fixture and setup-sheet commands through the UI queue (M3.2
//! stages B-D): the dialog arms, the refuse wording, and the law that
//! an import-as-fixture lands as ONE undo unit (#339 slice 4; it was
//! two before). No window, no modal
//! picker - the file arms and the public `import_as_fixture` hand-off
//! stand in for the dialogs.
//! Provenance: our own harness; behavior specced from the pages.
//!
//! Gate map (M4.4): covers ui::state, app::fixture, app::model.

use camcraft_app::fixture;
use camcraft_app::model;
use camcraft_ui::state::{CommandKind, State};

fn cube_path() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../testdata/models/cube.stl")
}

fn is_fixture(state: &State, name: &str) -> bool {
    model::find_by_name(&state.world, name)
        .map(|e| fixture::def_of(&state.world, &e).is_some())
        .unwrap_or(false)
}

#[test]
fn marking_a_model_as_a_fixture_roundtrips_through_undo() {
    let mut state = State::new();
    state.pending.push_back(CommandKind::ImportFile(cube_path()));
    state.settle();
    assert!(state.status.starts_with("imported cube"), "{:?}", state.status);

    state.pending.push_back(CommandKind::TagFixture {
        name: "cube".into(),
        setup: None,
    });
    state.drain_actions();
    assert_eq!(state.status, "marked cube as a fixture");
    assert!(is_fixture(&state, "cube"), "the marker is on");

    // Undo removes the marker but never the model: the geometry is a
    // plain solid again, not deleted furniture.
    state.pending.push_back(CommandKind::Undo);
    state.drain_actions();
    assert!(!is_fixture(&state, "cube"));
    assert!(model::find_by_name(&state.world, "cube").is_some());

    state.pending.push_back(CommandKind::Redo);
    state.drain_actions();
    assert!(is_fixture(&state, "cube"), "redo re-marks");
}

#[test]
fn moving_an_unmarked_model_is_refused() {
    let mut state = State::new();
    state.pending.push_back(CommandKind::ImportFile(cube_path()));
    state.settle();
    state.pending.push_back(CommandKind::MoveFixture {
        name: "cube".into(),
        pos: [10.0, 0.0, 0.0],
        roll_deg: 0.0,
        pitch_deg: 0.0,
        yaw_deg: 0.0,
    });
    state.drain_actions();
    assert!(
        state.status.starts_with("fixture not moved"),
        "{:?}",
        state.status
    );
    // Refusal wrote nothing - the cube stands where it was.
    let e = model::find_by_name(&state.world, "cube").expect("the model");
    assert_eq!(model::transform_of(&state.world, &e).unwrap().pos.x, 0.0);
}

#[test]
fn delete_fixture_takes_the_solid_and_undo_brings_it_back_marked() {
    let mut state = State::new();
    state.pending.push_back(CommandKind::ImportFile(cube_path()));
    state.settle();
    state.pending.push_back(CommandKind::TagFixture {
        name: "cube".into(),
        setup: None,
    });
    state.drain_actions();

    state.pending.push_back(CommandKind::DeleteFixture("cube".into()));
    state.drain_actions();
    assert_eq!(state.status, "deleted fixture cube");
    assert!(model::find_by_name(&state.world, "cube").is_none());

    state.pending.push_back(CommandKind::Undo);
    state.drain_actions();
    assert!(is_fixture(&state, "cube"), "undo restores the marker");
    assert_eq!(
        model::facets_of(&state.world, &model::find_by_name(&state.world, "cube").unwrap())
            .map(|f| f.len()),
        Some(12),
        "and the geometry"
    );
}

#[test]
fn import_as_fixture_lands_as_one_undo_unit() {
    // #339 slice 4: the dialog hands the worker a binding; the landed
    // mesh and the marker are one command (`ImportFixture`), so one
    // undo takes the whole import away and one redo brings it back marked.
    let mut state = State::new();
    state.import_as_fixture = Some(None);
    state.pending.push_back(CommandKind::ImportFile(cube_path()));
    state.settle();
    assert!(
        state.status.contains("; marked as a fixture"),
        "{:?}",
        state.status
    );
    assert!(is_fixture(&state, "cube"));
    assert!(state.import_as_fixture.is_none(), "the binding is spent");

    state.pending.push_back(CommandKind::Undo);
    state.drain_actions();
    assert!(model::find_by_name(&state.world, "cube").is_none(), "one undo takes model and marker");
    state.pending.push_back(CommandKind::Redo);
    state.drain_actions();
    assert!(is_fixture(&state, "cube"), "one redo brings it back marked");
}

#[test]
fn import_as_fixture_binds_to_the_active_setup() {
    let mut state = State::new();
    state.pending.push_back(CommandKind::CreateSetup {
        name: None,
        datum_from: None,
    });
    state.drain_actions();
    state.import_as_fixture = Some(Some("Setup1".into()));
    state.pending.push_back(CommandKind::ImportFile(cube_path()));
    state.settle();
    assert!(
        state.status.contains("; marked as Setup1's fixture"),
        "{:?}",
        state.status
    );
    let e = model::find_by_name(&state.world, "cube").expect("the model");
    assert_eq!(
        fixture::def_of(&state.world, &e).map(|d| d.setup),
        Some(Some("Setup1".to_string())),
    );
}

#[test]
fn untag_is_refused_on_a_plain_model_and_works_on_a_fixture() {
    let mut state = State::new();
    state.pending.push_back(CommandKind::ImportFile(cube_path()));
    state.settle();
    state.pending.push_back(CommandKind::UntagFixture("cube".into()));
    state.drain_actions();
    assert!(
        state.status.starts_with("fixture not unmarked"),
        "{:?}",
        state.status
    );
    state.pending.push_back(CommandKind::TagFixture {
        name: "cube".into(),
        setup: None,
    });
    state.drain_actions();
    state.pending.push_back(CommandKind::UntagFixture("cube".into()));
    state.drain_actions();
    assert!(state.status.starts_with("unmarked cube"), "{:?}", state.status);
    assert!(!is_fixture(&state, "cube"));
    assert!(model::find_by_name(&state.world, "cube").is_some());
}

#[test]
fn sheet_file_arm_writes_one_page_per_setup() {
    let mut state = State::new();
    state.pending.push_back(CommandKind::CreateSetup {
        name: None,
        datum_from: None,
    });
    state.drain_actions();
    state.pending.push_back(CommandKind::CreateSetup {
        name: None,
        datum_from: None,
    });
    state.drain_actions();

    let path = std::env::temp_dir().join(format!(
        "camcraft_sheet_test_{}.txt",
        std::process::id()
    ));
    state.pending.push_back(CommandKind::SetupSheetFile(path.clone()));
    state.drain_actions();
    assert!(state.status.contains("2 setup sheet(s) written"), "{:?}", state.status);
    let text = std::fs::read_to_string(&path).expect("the sheet file");
    let _ = std::fs::remove_file(&path);
    assert!(text.starts_with("SETUP SHEET - Setup1"), "{text}");
    assert!(text.contains('\u{0c}'), "form feed between pages");
    assert!(text.contains("SETUP SHEET - Setup2"), "second page");
}
