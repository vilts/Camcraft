//! #1054: the STEP facet budget is set on the model page, refused by name
//! outside its bounds, and read by the next STEP import: a lowered budget
//! lands the model flagged, a raised one re-imports it clear - each import
//! one undoable command that records the budget with the model.
//! Provenance: our own tests over our own shell; no vendor material.
//!
//! Gate map (#1054): covers ui::state (step_budget, begin_import,
//! finish_import), ui::egui_app::step_budget_row, ui::egui_app::pages
//! (model_page), app::model_fidelity (budget_of).

use super::*;
use camcraft_app::geometry::import::step_budget;

fn step_path() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../testdata/models/Milling-test.step")
}

fn flags(st: &State) -> (Option<u64>, Option<f64>) {
    let e = model::find_by_name(&st.world, "Milling-test").expect("imported");
    (camcraft_app::model_fidelity::budget_of(&st.world, &e), camcraft_app::model_fidelity::chord_of(&st.world, &e))
}

#[test]
fn the_model_page_sets_the_budget_the_next_step_import_runs_under() {
    let mut h = shell_harness();
    h.state_mut().state.ui_target = UiTarget::Model("Block".into());
    h.run();
    assert!(h.query_by_label("STEP facet budget (next import)").is_some(), "the setting is on the model page");
    assert!(h.query_by_label("Imported under a STEP facet budget of 30000000 facets").is_some());

    // Out of bounds: refused by name, on the row; the setting keeps its value.
    {
        let st = &mut h.state_mut().state;
        let err = st.set_step_budget("7").expect_err("refused");
        assert!(err.starts_with("STEP facet budget 7 is outside"), "{err}");
        assert_eq!(st.step_budget.value, step_budget::DEFAULT);
        assert!(st.set_step_budget("lots").is_err());
        assert!(st.set_step_budget(&(step_budget::CEILING + 1).to_string()).is_err());
    }
    h.run();
    let said = h.query_all_by_label_contains("STEP facet budget 60000001 is outside").count();
    assert!(said >= 2, "the refusal is on the row and the status line ({said})");

    // Lowered: the import lands flagged and records the budget.
    {
        let st = &mut h.state_mut().state;
        assert_eq!(st.set_step_budget("2,000"), Ok(2_000));
        st.pending.push_back(CommandKind::ImportFile(step_path()));
        st.settle();
        let (budget, chord) = flags(st);
        assert_eq!(budget, Some(2_000));
        assert!(chord.is_some(), "a smaller budget flags, never passes silently");
        // Raised: the same file again updates the model to full fidelity.
        assert_eq!(st.set_step_budget("5000000"), Ok(5_000_000));
        st.pending.push_back(CommandKind::ImportFile(step_path()));
        st.settle();
        assert_eq!(flags(st), (Some(5_000_000), None));
        // One undo takes the re-import back, budget and flag together.
        st.history.undo(&st.world);
        assert_eq!(flags(st).0, Some(2_000));
        assert!(flags(st).1.is_some());
        st.ui_target = UiTarget::Model("Milling-test".into());
    }
    h.run();
    assert!(h.query_by_label("Imported under a STEP facet budget of 2000 facets").is_some());
    assert!(h.query_all_by_label_contains("Reduced fidelity").next().is_some(), "the undone model is flagged again");
}
