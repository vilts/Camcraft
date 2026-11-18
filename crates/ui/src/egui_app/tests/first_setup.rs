//! #322's frame test: a fresh document reaches the Setups page from the
//! browser and from the ribbon, and its "Create first setup" button makes
//! the setup through the command history, so undo takes it away again.
//! Provenance: our own tests over our own shell; no HSMWorks material.

use super::*;

/// The blank window (no seed): the shape a new document opens in.
fn blank_harness() -> Harness<'static, ShellUi> {
    Harness::builder()
        .with_size(Vec2::new(1280.0, 720.0))
        .build_ui_state(frame, ShellUi::blank())
}

#[test]
fn a_fresh_document_reaches_setups_and_creates_the_first_one_undoably() {
    let mut harness = blank_harness();
    harness.run();
    assert!(
        camcraft_app::setup::names(&harness.state().state.world).is_empty(),
        "a new document starts with no setups"
    );

    // The browser: the Setups row stands in a document with no setups.
    let (rows, leaves) = panes::browser_rows(&harness.state().state);
    let row = leaves
        .iter()
        .position(|leaf| matches!(leaf, panes::Leaf::Setups))
        .expect("the browser shows Setups in a fresh document");
    assert_eq!(rows[row].label, "Setups");

    // The ribbon: a real click on the Setups cell opens the page.
    harness.get_by_role_and_label(egui::accesskit::Role::Button, "Setups").click();
    harness.run_steps(2);
    assert!(
        matches!(harness.state().state.ui_target, UiTarget::Setups),
        "the ribbon's Setups cell opens the Setups page (target {:?})",
        harness.state().state.ui_target
    );

    // The page's button creates the first setup through the queue.
    harness.get_by_label("Create first setup").click();
    harness.run_steps(2);
    let names = camcraft_app::setup::names(&harness.state().state.world);
    assert_eq!(names.len(), 1, "Create first setup made one setup: {names:?}");
    assert!(harness.state().state.history.can_undo(), "the creation is on the history");

    // Undo walks it back: the history is the source of truth.
    crate::actions::undo(&mut harness.state_mut().state);
    harness.run_steps(2);
    assert!(
        camcraft_app::setup::names(&harness.state().state.world).is_empty(),
        "undo removes the first setup"
    );
    assert!(
        harness.query_by_label("Create first setup").is_some(),
        "the page is back on its no-setup state"
    );
}
