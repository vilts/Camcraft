//! #3893 (U2 of #3663): the Tool tab of the tool library window's editor -
//! the type, the geometry and the rows that the tool's kind reads. It
//! calls the same rows as Properties (`tool_settings_grid::geometry`, which
//! draws `kind_rows`), so a new kind row lands in one place. The preview
//! stands above the tabs (U3, `preview`); #3895 (U4): the holder, the
//! gauge length and the stick-out follow the rows (`assembly_group`), and
//! #3930 its folded Body section (the stand-in holder, the body). The
//! cutting rows are on the Recipes tab (#3896, U5: `recipes_tab`).
//! Provenance: our own tab over the tool draft; no HSMWorks material
//! involved.

use egui::Ui;

use crate::state::State;

/// The rows, then "Edit tool shape". Returns whether that button was
/// clicked; the caller opens the shape window after the window's draft is
/// put back (`State::with_library_draft`).
pub(super) fn draw(ui: &mut Ui, state: &mut State) -> bool {
    super::super::tool_settings_grid::geometry(ui, state);
    super::assembly_group::draw(ui, state);
    ui.add_space(6.0);
    ui.button("Edit tool shape")
        .on_hover_text("open the drawn holder profile, the cutter and the body in their own window")
        .clicked()
}
