//! The transport's line for the stock removal calculated before
//! playback (#2181): progress while it runs, a Cancel button, and the
//! word "incomplete" when it stopped early; and the line that names the
//! operation the stock simulation stops before (#2280). Provenance: our own
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

/// The sentence when the stock simulation stops before `op` (#2280):
/// it has no toolpath, so nothing after it is shown cut.
#[must_use]
pub fn stop_readout(op: &str) -> String {
    format!("Stock simulation stops before {op}: it has no toolpath. Generate it to see the rest.")
}

/// The stop sentence for the running playback, when it stops early.
#[must_use]
pub fn playback_stop(state: &State) -> Option<String> {
    state.field_play.as_ref()?.index().stop().map(stop_readout)
}

/// True while the calculation before playback runs (#2611). The stock
/// on screen stays uncut until the calculation ends, so the transport
/// holds its playhead (and its scrub) as the line says: a tool that
/// moved on would show cuts that the picture does not have.
#[must_use]
pub fn holds_playhead(state: &State) -> bool {
    state.field_play.as_ref().is_some_and(|p| matches!(p.precalc(), Precalc::Running { .. }))
}

/// One frame of the transport clock: `Sim::advance`, held while the
/// calculation runs (`holds_playhead`).
pub(crate) fn advance_playhead(state: &mut State, dt: f64, total: f64) {
    if holds_playhead(state) {
        return;
    }
    if let Some(sim) = state.sim.as_mut() {
        sim.advance(dt, total);
    }
}

/// Draw the line under the transport bar, when there is one.
pub(crate) fn transport_row(ui: &mut egui::Ui, state: &State) {
    let Some(play) = state.field_play.as_ref() else {
        return;
    };
    // #2280: its own line, never a clause on the status line.
    if let Some(text) = playback_stop(state) {
        ui.label(text);
    }
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
