//! The overwrite prompt (#3608): a modal over the shell while a Save waits on
//! `state.save_guard.held`, because the project file on disk changed after
//! the window opened or saved it. The three
//! buttons answer through `actions::resolve_overwrite`; Escape or a click
//! on the backdrop is Cancel.
//!
//! Provenance: our own layout, the unsaved-changes prompt's shape; egui's
//! `Modal` container. No HSMWorks material.

use egui::{Id, Modal};

use crate::actions;
use crate::save_guard::{OverwriteChoice, file_name};
use crate::state::State;

pub(crate) fn window(ctx: &egui::Context, state: &mut State) {
    let Some(path) = state.save_guard.held.as_ref() else {
        return;
    };
    let name = file_name(path);
    let mut choice = None;
    let response = Modal::new(Id::new("overwrite-prompt")).show(ctx, |ui| {
        ui.set_max_width(360.0);
        ui.heading(format!("{name}: the file on disk changed"));
        ui.label(format!(
            "{name} on disk changed after this window opened or saved it. \
             Overwrite puts your document in place of that file. \
             Save As writes your document to a new file and does not change the file on disk."
        ));
        ui.add_space(8.0);
        // The answer that keeps the file on disk comes first, the answer
        // that writes over it comes last.
        ui.horizontal(|ui| {
            if ui.button("Save As").clicked() {
                choice = Some(OverwriteChoice::SaveAs);
            }
            if ui.button("Cancel").clicked() {
                choice = Some(OverwriteChoice::Cancel);
            }
            if ui.button("Overwrite").clicked() {
                choice = Some(OverwriteChoice::Overwrite);
            }
        });
    });
    if choice.is_none() && response.should_close() {
        choice = Some(OverwriteChoice::Cancel);
    }
    if let Some(choice) = choice {
        actions::resolve_overwrite(state, choice);
    }
}
