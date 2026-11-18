//! The transport's line for the stock removal calculated before
//! playback (#2181): progress while it runs, a Cancel button, and the
//! word "incomplete" when it stopped early. Provenance: our own
//! transport bar (`egui_app::shell`); nothing from vendor binaries.
//!
//! The line is display state only: no command, nothing undoable, and
//! nothing that the Deviate verdict or the post gate reads.

use camcraft_app::stock_play::Precalc;

use crate::state::State;

/// The sentence for a calculation state, or `None` when there is
/// nothing to say (no calculation, or a finished one).
#[must_use]
pub fn precalc_readout(p: &Precalc) -> Option<String> {
    let pct = |done: usize, total: usize| done.saturating_mul(100).checked_div(total).unwrap_or(100);
    match p {
        Precalc::Off | Precalc::Ready { .. } => None,
        Precalc::Running { done, total } => {
            Some(format!("Calculating the stock removal: {}% - playback starts when it is done", pct(*done, *total)))
        }
        Precalc::Incomplete { done, total, why } => Some(format!(
            "Stock removal incomplete ({why} at {}%) - the stock after that point is calculated during playback",
            pct(*done, *total)
        )),
    }
}

/// The Cancel button's label.
pub const CANCEL: &str = "Cancel";

/// Draw the line under the transport bar, when there is one.
pub(crate) fn transport_row(ui: &mut egui::Ui, state: &State) {
    let Some(play) = state.field_play.as_ref() else {
        return;
    };
    if precalc_row(ui, &play.precalc()) {
        play.cancel_precalc();
    }
}

/// The line for a calculation state: the readout, and Cancel while it
/// runs. `true` when Cancel was clicked this frame. Draws nothing when
/// the readout has nothing to say.
pub(crate) fn precalc_row(ui: &mut egui::Ui, p: &Precalc) -> bool {
    let Some(text) = precalc_readout(p) else {
        return false;
    };
    ui.horizontal(|ui| {
        ui.label(text);
        matches!(p, Precalc::Running { .. })
            && ui.button(CANCEL).on_hover_text("Stop the calculation and calculate the stock during playback").clicked()
    })
    .inner
}
