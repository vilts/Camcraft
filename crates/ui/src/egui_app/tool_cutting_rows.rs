//! The tool editor's per-material cutting data (#890): one row of
//! material, spindle rpm and feed per workpiece material group, added
//! and removed in the draft. Apply / Add carry the table through
//! `ChangeTool` / `AddTool` like every other tool number, so one edit is
//! one undo step and a bad row (a material twice, a non-positive rpm or
//! feed) is refused there by the tool's name.
//!
//! Provenance: our own widget over the tool model's cutting rows.

use egui::Ui;

use camcraft_app::tool_cutting_rows::CuttingRow;
use camcraft_app::tools::WorkMaterial;
use camcraft_app::units::Unit;

use crate::state::ToolUi;

use super::fields::{combo, field, num_feed, num_plain};

/// Append a row for the first material the table does not name yet,
/// starting from the draft's running numbers; `false` when every
/// material already has one.
pub(super) fn add_row(t: &mut ToolUi) -> bool {
    let Some(m) = WorkMaterial::ALL.into_iter().find(|m| !t.cutting.iter().any(|r| r.material == *m)) else {
        return false;
    };
    t.cutting.push(CuttingRow { material: m, rpm: t.rpm, feed: t.feed });
    true
}

/// The rows, under one collapsed header (open when the tool states a
/// row) so the editor keeps its height.
pub(super) fn cutting_rows(ui: &mut Ui, t: &mut ToolUi, u: Unit, torch: bool) {
    let labels: Vec<&str> = WorkMaterial::ALL.iter().map(|m| m.label()).collect();
    egui::CollapsingHeader::new("Cutting data by material")
        .id_salt("tool_cutting_header")
        .default_open(!t.cutting.is_empty())
        .show(ui, |ui| {
            let mut remove = None;
            for (i, row) in t.cutting.iter_mut().enumerate() {
                ui.push_id(("tool_cutting_row", i), |ui| {
                    let mut idx = WorkMaterial::ALL.iter().position(|m| *m == row.material).unwrap_or(0);
                    ui.horizontal(|ui| {
                        combo(ui, "material", &mut idx, &labels);
                        if ui.small_button("Remove").on_hover_text("drop this material's row").clicked() {
                            remove = Some(i);
                        }
                    });
                    if let Some(m) = WorkMaterial::ALL.get(idx) {
                        row.material = *m;
                    }
                    if !torch {
                        field(ui, "Spindle rpm");
                        num_plain(ui, &mut row.rpm);
                    }
                    field(ui, &format!("Feed ({})", u.feed_suffix()));
                    num_feed(ui, u, &mut row.feed);
                });
            }
            if let Some(i) = remove
                && i < t.cutting.len()
            {
                t.cutting.remove(i);
            }
            let full = t.cutting.len() >= WorkMaterial::ALL.len();
            if ui
                .add_enabled(!full, egui::Button::new("Add material row"))
                .on_hover_text("a row for the next material, starting from the rpm and feed above; the document's material picks its row")
                .clicked()
            {
                add_row(t);
            }
        });
}

#[cfg(test)]
mod tests {
    use super::add_row;
    use crate::state::ToolUi;
    use camcraft_app::tools::WorkMaterial;

    #[test]
    fn rows_add_per_material_and_ride_the_draft_both_ways() {
        let mut d = ToolUi { rpm: 9000.0, feed: 1200.0, ..ToolUi::default() };
        assert!(add_row(&mut d));
        assert!(add_row(&mut d));
        assert_eq!(d.cutting[0].material, WorkMaterial::ALL[0]);
        assert_ne!(d.cutting[1].material, d.cutting[0].material, "a new row takes an unused material");
        assert_eq!((d.cutting[0].rpm, d.cutting[0].feed), (9000.0, 1200.0));
        d.cutting[1].rpm = 3000.0;
        let back = ToolUi::from_tool("T", &d.to_tool(), true);
        assert_eq!(back.cutting, d.cutting);
        while add_row(&mut d) {}
        assert_eq!(d.cutting.len(), WorkMaterial::ALL.len(), "one row per material, no more");
    }
}
