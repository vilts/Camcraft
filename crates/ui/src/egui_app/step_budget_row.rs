//! The model page's STEP facet budget row (#1054): the budget the next
//! STEP import runs under (typed, committed by Enter or Set; refused by
//! name outside its bounds) and the budget this model was imported under.
//! A model imported at reduced fidelity is cleared by raising it here and
//! importing the file again.
//! Provenance: our own layout over our own `state_step_budget`; no vendor
//! material.

use egui::{Key, TextEdit, Ui};
use flecs_ecs::prelude::Entity;

use camcraft_app::geometry::import::step_budget;

use crate::state::State;

pub(super) fn row(ui: &mut Ui, state: &mut State, entity: &Entity) {
    ui.add_space(6.0);
    let recorded = camcraft_app::model_fidelity::budget_of(&state.world, entity).unwrap_or(step_budget::DEFAULT);
    ui.label(format!("Imported under a {} of {recorded} facets", step_budget::NAME));
    if !state.step_budget.explicit {
        ui.label(format!("A re-import keeps {recorded} unless a budget is set below"));
    }
    let mut commit = false;
    ui.horizontal(|ui| {
        let l = ui.label(format!("{} (next import)", step_budget::NAME));
        let r = ui.add(TextEdit::singleline(&mut state.step_budget.draft).desired_width(110.0)).labelled_by(l.id);
        commit |= r.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter));
        commit |= ui.button("Set").clicked();
    });
    if commit {
        let text = state.step_budget.draft.clone();
        // A refusal is kept on the row and the status line by the setter.
        let _ = state.set_step_budget(&text);
    }
    if let Some(err) = &state.step_budget.error {
        ui.colored_label(ui.visuals().error_fg_color, err);
    }
}
