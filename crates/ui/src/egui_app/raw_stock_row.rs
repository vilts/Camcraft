//! The stock page's "raw stock only" checkbox (#1094): for a stock made
//! from a model, whether that source model is a casting or blank (not
//! judged as the part) or the part itself (judged, the default). A tick
//! is one undoable `SetStockSourceOnly`, queued like every page edit.
//! Provenance: our own page law.

use egui::Ui;

use camcraft_app::stock_source::{raw_stock_only_of, void_note_of, RAW_STOCK_ONLY};

use crate::state::{CommandKind, State};

/// Draw the row under the Source picker; nothing while the setup's stock
/// is not made from a model (the mark has no source to speak of).
pub(super) fn raw_stock_row(ui: &mut Ui, state: &mut State) {
    let active = camcraft_app::setup::active(&state.world);
    let Some(marked) = raw_stock_only_of(&state.world, active.as_deref()) else {
        ui.label("Apply a stock from a model to say whether its source is raw stock only.");
        return;
    };
    let mut on = marked;
    ui.checkbox(&mut on, RAW_STOCK_ONLY).on_hover_text(
        "Tick when the source model is a casting or blank you cut into and the finished part is its own model: the post's gouge check then does not judge the source. An operation that machines the source judges it again.",
    );
    // A mark whose model changed under it is void: say so, by name.
    if let Some(note) = void_note_of(&state.world, active.as_deref()) {
        ui.label(note);
    }
    if on != marked {
        state.pending.push_back(CommandKind::SetRawStockOnly(on));
    }
}
