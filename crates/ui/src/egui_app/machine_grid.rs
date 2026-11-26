//! The machine overview as a two-column property grid (#745): labels in
//! one aligned column, values word-wrapping to the pane. Rows come from
//! `Machine::rows`, so the page and the editor header cannot drift.
//!
//! Provenance: our own egui layout; no HSMWorks material involved.

use camcraft_app::machine::Machine;
use camcraft_app::units::Unit;
use egui::Ui;

/// The row gap the overview keeps by default (the machine editor header).
pub(super) const ROW_GAP: f32 = 4.0;

/// Draw `m` as label | value rows, plus any `extra` rows (e.g. Source).
/// `row_gap` is the vertical gap between rows: the setup's Machine page
/// keeps its rows tighter than the editor header (#2105).
pub(super) fn machine_grid(ui: &mut Ui, id: &str, m: &Machine, unit: Unit, extra: &[(&str, String)], row_gap: f32) {
    egui::Grid::new(id).num_columns(2).spacing([12.0, row_gap]).show(ui, |ui| {
        let rows = m.rows(unit);
        let all = rows.iter().map(|(k, v)| (*k, v.as_str())).chain(extra.iter().map(|(k, v)| (*k, v.as_str())));
        for (label, value) in all {
            ui.label(egui::RichText::new(label).strong());
            ui.add(egui::Label::new(value).wrap());
            ui.end_row();
        }
    });
}
