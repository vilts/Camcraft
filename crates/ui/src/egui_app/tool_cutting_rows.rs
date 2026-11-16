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
use camcraft_app::tools::ToolKind;
use camcraft_app::waterjet_tools::{PierceMethod, WaterjetCutData};
use camcraft_app::units::Unit;

use crate::state::ToolUi;

use super::fields::{combo, field, num_feed, num_plain};

/// Append a row for the first material the table does not name yet,
/// starting from the draft's running numbers; `false` when every
/// material already has one.
pub(super) fn add_row(t: &mut ToolUi) -> bool {
    if t.tool_kind() == ToolKind::Waterjet {
        // Zero thickness deliberately needs the user's value before Apply succeeds.
        t.cutting.push(CuttingRow {
            material: WorkMaterial::ALL[0], rpm: 0.0, feed: t.feed,
            waterjet: Some(WaterjetCutData {
                thickness: 0.0, kerf: t.diameter, pierce_time: 0.0,
                abrasive_delay: 0.0, re_pierce_time: 0.0, pierce_method: PierceMethod::Stationary,
            }),
        });
        return true;
    }
    let Some(m) = WorkMaterial::ALL.into_iter().find(|m| !t.cutting.iter().any(|r| r.material == *m)) else {
        return false;
    };
    t.cutting.push(CuttingRow { material: m, rpm: t.rpm, feed: t.feed, waterjet: None });
    true
}

/// The rows, under one collapsed header (open when the tool states a
/// row) so the editor keeps its height.
pub(super) fn cutting_rows(ui: &mut Ui, t: &mut ToolUi, u: Unit, torch: bool) {
    let waterjet = t.tool_kind() == ToolKind::Waterjet;
    let labels: Vec<&str> = WorkMaterial::ALL.iter().map(|m| m.label()).collect();
    egui::CollapsingHeader::new(if waterjet { "Cutting data by material and thickness" } else { "Cutting data by material" })
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
                    if let Some(w) = &mut row.waterjet {
                        field(ui, "Plate thickness");
                        super::fields::num(ui, u, &mut w.thickness);
                        field(ui, "Kerf width");
                        super::fields::num(ui, u, &mut w.kerf);
                        for (label, value) in [("Pierce time (s)", &mut w.pierce_time), ("Abrasive delay (s)", &mut w.abrasive_delay), ("Re-pierce time (s)", &mut w.re_pierce_time)] {
                            field(ui, label);
                            num_plain(ui, value);
                        }
                        let methods = [PierceMethod::Stationary, PierceMethod::Dynamic, PierceMethod::Wiggle];
                        let mut method = methods.iter().position(|m| *m == w.pierce_method).unwrap_or(0);
                        field(ui, "Pierce method");
                        combo(ui, "pierce_method", &mut method, &["Stationary", "Dynamic", "Wiggle"]);
                        if let Some(m) = methods.get(method) { w.pierce_method = *m; }
                    }
                });
            }
            if let Some(i) = remove
                && i < t.cutting.len()
            {
                t.cutting.remove(i);
            }
            let full = !waterjet && t.cutting.len() >= WorkMaterial::ALL.len();
            if ui
                .add_enabled(!full, egui::Button::new("Add material row"))
                .on_hover_text(if waterjet { "Add a recipe, then enter its material, thickness and pierce values" } else { "a row for the next material, starting from the rpm and feed above; the document's material picks its row" })
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

    #[test]
    fn waterjet_rows_require_a_thickness_and_preserve_every_recipe_field() {
        use camcraft_app::waterjet_tools::{PierceMethod, waterjet_tool};
        let mut d = ToolUi::from_tool("Nozzle", &waterjet_tool(0.8), true);
        assert!(add_row(&mut d));
        assert!(d.to_tool().torch_refusal().is_some_and(|s| s.contains("thickness")));
        d.cutting[0].material = WorkMaterial::MildSteel;
        d.cutting[0].feed = 700.0;
        let w = d.cutting[0].waterjet.as_mut().expect("waterjet row");
        w.thickness = 5.0;
        w.kerf = 0.9;
        w.pierce_time = 2.0;
        w.abrasive_delay = 0.3;
        w.re_pierce_time = 1.2;
        w.pierce_method = PierceMethod::Dynamic;
        assert!(d.to_tool().is_sane());
        let first = d.cutting[0];
        assert!(add_row(&mut d));
        d.cutting[1] = first;
        d.cutting[1].waterjet.as_mut().unwrap().thickness = 10.0;
        let saved = d.to_tool();
        assert!(saved.is_sane());
        assert_eq!(saved.rpm, 0.0);
        assert_eq!(saved.kind, camcraft_app::tools::ToolKind::Waterjet);
        assert_eq!(ToolUi::from_tool("Nozzle", &saved, true).to_tool(), saved);
        assert_eq!(d.cutting[0], first, "adding another thickness keeps the first recipe");
        for _ in 0..WorkMaterial::ALL.len() { assert!(add_row(&mut d)); }
        assert!(d.cutting.len() > WorkMaterial::ALL.len(), "waterjet rows are not capped at one per material");
    }
}
