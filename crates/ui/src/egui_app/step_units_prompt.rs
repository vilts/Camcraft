//! The length-unit prompt (#1874): a modal over the shell while a STEP
//! import waits on `state.step_units_ask`. The file states no length unit,
//! so the user states the unit that it was drawn in; the answer goes
//! through the command queue (`CommandKind::ResolveStepUnits`) and imports
//! the file again in that unit. Escape or a click on the backdrop is
//! Cancel, which imports nothing.
//! Provenance: our own layout over egui's `Modal` container, the same as
//! the unsaved-changes prompt. No HSMWorks material consulted.

use camcraft_app::units::Unit;
use egui::{Id, Modal};

use crate::state::{CommandKind, State};

/// The prompt's title.
pub(crate) const TITLE: &str = "Length unit";
/// The answer buttons.
pub(crate) const MILLIMETERS: &str = "Millimeters (mm)";
pub(crate) const INCHES: &str = "Inches (in)";
pub(crate) const CANCEL: &str = "Cancel";

pub(crate) fn window(ctx: &egui::Context, state: &mut State) {
    let Some(ask) = state.step_units_ask.as_ref() else {
        return;
    };
    let name = ask
        .path
        .file_name()
        .map_or_else(|| ask.path.display().to_string(), |n| n.to_string_lossy().into_owned());
    let mut answer: Option<Option<Unit>> = None;
    let response = Modal::new(Id::new("step-units-prompt")).show(ctx, |ui| {
        ui.set_max_width(380.0);
        ui.heading(TITLE);
        ui.label(format!("{name} states no length unit."));
        ui.label("Choose the unit that the model was drawn in. CamCraft does not guess the unit.");
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            if ui.button(MILLIMETERS).clicked() {
                answer = Some(Some(Unit::Millimeters));
            }
            if ui.button(INCHES).clicked() {
                answer = Some(Some(Unit::Inches));
            }
            if ui.button(CANCEL).clicked() {
                answer = Some(None);
            }
        });
    });
    if answer.is_none() && response.should_close() {
        answer = Some(None);
    }
    if let Some(unit) = answer {
        state.pending.push_back(CommandKind::ResolveStepUnits(unit));
    }
}
