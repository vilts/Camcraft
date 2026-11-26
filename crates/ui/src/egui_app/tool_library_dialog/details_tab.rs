//! #3893 (U2 of #3663): the Details tab of the tool library window's
//! editor - the workpiece material group, the catalog facts of #898 (part
//! number, vendor, coating, helix angle, notes) and the tool number. It
//! calls the same rows as Properties' Catalog group
//! (`tool_settings_grid::catalog`, which draws
//! `tool_catalog_rows::catalog_rows`), so the two cannot drift.
//! Provenance: our own tab over the tool draft; no HSMWorks material
//! involved.

use egui::Ui;

use crate::state::State;

pub(super) fn draw(ui: &mut Ui, state: &mut State) {
    super::super::tool_settings_grid::catalog(ui, state);
}
