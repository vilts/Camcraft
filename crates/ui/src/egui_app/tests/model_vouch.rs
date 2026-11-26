//! #1339: the model page offers "Vouch for this mesh" beside the
//! "Fidelity unknown" line. The button waits for a name, queues
//! `VouchModelFidelity`, and the page then says who vouched. Undo brings
//! the row back. A recorded reduced fidelity gets no row.
//! Provenance: our own cube meshes with the fidelity stated by the test.
//!
//! Gate map (#1339): covers ui::egui_app::vouch_row, ui::egui_app::pages
//! (model_page), ui::state (CommandKind::VouchModelFidelity, vouch_model).

use camcraft_app::commands::ImportModel;
use camcraft_app::geometry::{cube, Point3};
use camcraft_app::{model, model_fidelity};
use egui_kittest::kittest::NodeT;

use super::super::vouch_row::{BUTTON, FIELD};
use super::*;

type H = Harness<'static, ShellUi>;

fn unknown(h: &H, name: &str) -> bool {
    let st = &h.state().state;
    model::find_by_name(&st.world, name)
        .and_then(|e| model_fidelity::chord_of(&st.world, &e))
        .is_some_and(model_fidelity::is_unknown)
}

#[test]
fn model_page_vouches_for_an_unknown_mesh_and_undo_brings_the_row_back() {
    let _guard = snapshot_guard();
    let mut h = shell_harness();
    {
        let st = &mut h.state_mut().state;
        let brick = || cube(Point3::new(0.0, 0.0, 0.0), Point3::new(20.0, 10.0, 5.0));
        st.history.execute(&st.world, ImportModel::from_mesh("old", brick()).with_fidelity(Some(model_fidelity::UNKNOWN_MM)));
        st.history.execute(&st.world, ImportModel::from_mesh("coarse", brick()).with_fidelity(Some(0.03)));
        st.doc_version += 1;
        st.ui_target = UiTarget::Model("old".into());
    }
    h.run_steps(3);
    assert!(h.query_by_label("Model: old").is_some(), "the model page is open");
    assert!(h.query_by_label_contains("Fidelity unknown: saved before CamCraft recorded mesh fidelity").is_some());
    assert!(h.query_by_label(FIELD).is_some(), "the name field is drawn");
    assert!(h.get_by_label(BUTTON).accesskit_node().is_disabled(), "the button waits for a name");

    h.state_mut().state.vouch_by = "M. Kinney".into();
    h.run_steps(2);
    assert!(!h.get_by_label(BUTTON).accesskit_node().is_disabled(), "a name enables the button");
    h.get_by_label(BUTTON).click();
    h.run_steps(3);
    assert!(!unknown(&h, "old"), "the mark is cleared: {:?}", h.state().state.status);
    assert_eq!(h.state().state.status, "M. Kinney vouched for the mesh of old");
    assert_eq!(h.state().state.history.top_label(), Some("Vouch for the mesh of 'old' (by M. Kinney)"));
    assert!(h.query_by_label_contains("Fidelity vouched for by M. Kinney").is_some(), "the page says who vouched");
    assert!(h.query_by_label(BUTTON).is_none(), "a vouched model offers no second vouch");
    assert!(h.query_by_label_contains("Fidelity unknown").is_none());

    // Undo: the mark and the row are back.
    h.state_mut().state.pending.push_back(CommandKind::Undo);
    h.run_steps(3);
    assert!(unknown(&h, "old"), "undo puts the mark back");
    assert!(h.query_by_label(BUTTON).is_some());
    assert!(h.query_by_label_contains("Fidelity vouched for by").is_none());

    // A recorded reduced fidelity (#864) is never offered the row, and
    // the queue refuses it by name.
    h.state_mut().state.ui_target = UiTarget::Model("coarse".into());
    h.run_steps(3);
    assert!(h.query_by_label("Model: coarse").is_some());
    assert!(h.query_by_label_contains("Reduced fidelity").is_some());
    assert!(h.query_by_label(BUTTON).is_none(), "a recorded chord cannot be vouched away");
    h.state_mut().state.pending.push_back(CommandKind::VouchModelFidelity { name: "coarse".into(), by: "M. Kinney".into() });
    h.run_steps(2);
    let status = h.state().state.status.clone();
    assert!(status.starts_with("coarse not vouched for:") && status.contains("recorded fidelity"), "{status}");
}
