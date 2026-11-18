//! The stock page's workpiece material (#2015): the material the active
//! setup's stock is, which picks the tools' cutting-data rows its
//! operations are seeded from (the stock's, else the document's, else
//! each tool's Unknown row). A pick is one undoable `SetStockMaterial`,
//! queued like every page edit.
//! Provenance: our own page law.

use egui::Ui;

use camcraft_app::tools::WorkMaterial;

use crate::state::{CommandKind, State};

use super::fields::combo;

/// The picker's first entry: the stock states none.
pub(super) const DOCUMENT: &str = "As the document";

/// Draw the row; a hint while the setup has no stock to state it on.
pub(super) fn stock_material_row(ui: &mut Ui, state: &mut State) {
    let active = camcraft_app::setup::active(&state.world);
    if camcraft_app::setup::stock_entity(&state.world, active.as_deref()).is_none() {
        ui.label("Apply a stock to state its workpiece material.");
        return;
    }
    let now = camcraft_app::stock_material::of_setup(&state.world, active.as_deref());
    let mut names: Vec<&str> = vec![DOCUMENT];
    names.extend(WorkMaterial::ALL.iter().map(|m| m.label()));
    let mut idx = now.and_then(|m| WorkMaterial::ALL.iter().position(|x| *x == m)).map_or(0, |i| i + 1);
    ui.horizontal(|ui| {
        ui.label("Workpiece material");
        combo(ui, "stock.material", &mut idx, &names);
    })
    .response
    .on_hover_text("Picks each tool's cutting-data row for this setup's operations: the stock's material, else the document's, else the tool's Unknown row");
    let picked = idx.checked_sub(1).and_then(|i| WorkMaterial::ALL.get(i).copied());
    if picked != now {
        state.pending.push_back(CommandKind::SetStockMaterial(picked));
    }
}
