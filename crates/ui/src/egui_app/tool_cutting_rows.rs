//! The tool editor's per-material cutting data (#890): one row of
//! material, spindle rpm and feed per workpiece material group, added
//! and removed in the draft. Apply / Add carry the table through
//! `ChangeTool` / `AddTool` like every other tool number, so one edit is
//! one undo step and a bad row (a material twice, a non-positive rpm or
//! feed) is refused there by the tool's name.
//!
//! Provenance: our own widget over the tool model's cutting rows.

use egui::Ui;

use camcraft_app::tool_cutting_rows::{CuttingRow, material_label, row_keys};
use camcraft_app::tools::WorkMaterial;
use camcraft_app::tools::ToolKind;
use camcraft_app::waterjet_tools::{PierceMethod, WaterjetCutData};
use camcraft_app::units::Unit;

use crate::state::ToolUi;

use super::fields::{combo, field, num_feed, num_feed_named, num_named, num_plain, num_plain_named};

/// Append a row for the first material the table does not name yet,
/// starting from the draft's running numbers; `false` when every
/// material already has one. (#2015: the `Unknown` row is always shown,
/// never added.)
pub(super) fn add_row(t: &mut ToolUi) -> bool {
    if t.tool_kind() == ToolKind::Waterjet {
        // Zero thickness deliberately needs the user's value before Apply succeeds.
        t.cutting.push(CuttingRow {
            material: Some(WorkMaterial::ALL[0]), rpm: 0.0, feed: t.feed, stepdown: None, stepover: None,
            waterjet: Some(WaterjetCutData {
                thickness: 0.0, kerf: t.diameter, pierce_time: 0.0,
                abrasive_delay: 0.0, re_pierce_time: 0.0, pierce_method: PierceMethod::Stationary,
            }),
        });
        return true;
    }
    let Some(m) = WorkMaterial::ALL.into_iter().find(|m| !t.cutting.iter().any(|r| r.material == Some(*m))) else {
        return false;
    };
    t.cutting.push(CuttingRow::plain(Some(m), t.rpm, t.feed));
    true
}

/// An optional step (mm) as a number field: 0 reads "not stated".
fn step_field(ui: &mut Ui, u: Unit, v: &mut Option<f64>, name: &str) {
    let mut x = v.unwrap_or(0.0);
    num_named(ui, u, &mut x, name);
    *v = (x > 0.0).then_some(x);
}

/// The rows: a table of material, spindle rpm, feed, stepdown and
/// stepover (#2015) - or a waterjet's recipes - under one collapsed
/// header (open when the tool states a material row) so the editor
/// keeps its height. The `Unknown` row is the tool's default: its
/// material is fixed and it cannot be removed.
pub(super) fn cutting_rows(ui: &mut Ui, t: &mut ToolUi, u: Unit, torch: bool) {
    let waterjet = t.tool_kind() == ToolKind::Waterjet;
    let open = t.cutting.iter().any(|r| r.material.is_some());
    egui::CollapsingHeader::new(if waterjet { "Cutting data by material and thickness" } else { "Cutting data by material" })
        .id_salt("tool_cutting_header")
        .default_open(open)
        .show(ui, |ui| {
            let remove = if waterjet { waterjet_rows(ui, t, u) } else { table(ui, t, u, torch) };
            if let Some(i) = remove
                && i < t.cutting.len()
            {
                t.cutting.remove(i);
            }
            let full = !waterjet && WorkMaterial::ALL.iter().all(|m| t.cutting.iter().any(|r| r.material == Some(*m)));
            if ui
                .add_enabled(!full, egui::Button::new("Add material row"))
                .on_hover_text(if waterjet { "Add a recipe, then enter its material, thickness and pierce values" } else { "a row for the next material, starting from the rpm and feed above; the setup stock's material (else the document's) picks its row, else Unknown" })
                .clicked()
            {
                add_row(t);
            }
        });
}

/// The speed-and-feed table; the index of a row to remove, if clicked.
fn table(ui: &mut Ui, t: &mut ToolUi, u: Unit, torch: bool) -> Option<usize> {
    // #2015: a tool whose Unknown row is derived from its own rpm / feed
    // shows it from the draft's numbers; edited, it is stored.
    let derived = CuttingRow::plain(None, t.rpm, t.feed);
    let shown = t.tool_kind() != ToolKind::Electrode && !t.cutting.iter().any(CuttingRow::is_unknown);
    if shown {
        t.cutting.insert(0, derived);
    }
    let remove = table_rows(ui, t, u, torch);
    if shown && t.cutting.first() == Some(&derived) {
        t.cutting.remove(0);
        return remove.and_then(|i| i.checked_sub(1));
    }
    remove
}

fn table_rows(ui: &mut Ui, t: &mut ToolUi, u: Unit, torch: bool) -> Option<usize> {
    let keys = row_keys();
    let labels: Vec<&str> = keys.iter().skip(1).map(|m| material_label(*m)).collect();
    let mut remove = None;
    egui::Grid::new("tool_cutting_table").num_columns(6).striped(true).show(ui, |ui| {
        ui.label("Material");
        ui.label(if torch { "" } else { "Spindle rpm" });
        ui.label(format!("Feed ({})", u.feed_suffix()));
        ui.label("Stepdown");
        ui.label("Stepover");
        ui.label("");
        ui.end_row();
        for (i, row) in t.cutting.iter_mut().enumerate() {
            ui.push_id(("tool_cutting_row", i), |ui| {
                ui.horizontal(|ui| match row.material {
                    None => {
                        ui.label(material_label(None))
                            .on_hover_text("the tool's default row: what it runs when no row names the material, and the stepdown / stepover a row leaves blank");
                    }
                    Some(m) => {
                        let mut idx = WorkMaterial::ALL.iter().position(|x| *x == m).unwrap_or(0);
                        combo(ui, "material", &mut idx, &labels);
                        if let Some(m) = WorkMaterial::ALL.get(idx) {
                            row.material = Some(*m);
                        }
                    }
                });
            });
            if torch {
                ui.label("");
            } else {
                num_plain_named(ui, &mut row.rpm, "Spindle rpm");
            }
            num_feed_named(ui, u, &mut row.feed, "Feed");
            step_field(ui, u, &mut row.stepdown, "Stepdown");
            step_field(ui, u, &mut row.stepover, "Stepover");
            if row.material.is_some() {
                if ui.small_button("Remove").on_hover_text("drop this material's row").clicked() {
                    remove = Some(i);
                }
            } else {
                ui.label("");
            }
            ui.end_row();
        }
    });
    remove
}

/// A waterjet's recipes, one block each; the index to remove, if any.
fn waterjet_rows(ui: &mut Ui, t: &mut ToolUi, u: Unit) -> Option<usize> {
    let labels: Vec<&str> = WorkMaterial::ALL.iter().map(|m| m.label()).collect();
    let mut remove = None;
    for (i, row) in t.cutting.iter_mut().enumerate() {
        ui.push_id(("tool_cutting_row", i), |ui| {
            let mut idx = row.material.and_then(|m| WorkMaterial::ALL.iter().position(|x| *x == m)).unwrap_or(0);
            ui.horizontal(|ui| {
                combo(ui, "material", &mut idx, &labels);
                if ui.small_button("Remove").on_hover_text("drop this material's row").clicked() {
                    remove = Some(i);
                }
            });
            if let Some(m) = WorkMaterial::ALL.get(idx) {
                row.material = Some(*m);
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
    remove
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
        assert_eq!(d.cutting[0].material, Some(WorkMaterial::ALL[0]), "#2015: Unknown is shown, never added");
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
        d.cutting[0].material = Some(WorkMaterial::MildSteel);
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
