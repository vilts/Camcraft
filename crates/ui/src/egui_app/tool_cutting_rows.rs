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

use super::fields::{combo, num_feed_named, num_feed_row, num_named, num_plain_named, num_row};
use super::layout::prop_row;

/// Append a row for the first material the table does not name yet,
/// starting from the draft's running numbers; `false` when every
/// material already has one. (#2015: the `Unknown` row is always shown,
/// never added.)
pub(super) fn add_row(t: &mut ToolUi) -> bool {
    if t.tool_kind() == ToolKind::Waterjet {
        // Zero thickness deliberately needs the user's value before Apply succeeds.
        t.cutting.push(CuttingRow {
            material: Some(WorkMaterial::ALL[0]), rpm: 0.0, feed: t.feed, stepdown: None, stepover: None, plasma: None,
            waterjet: Some(WaterjetCutData {
                thickness: 0.0, kerf: t.diameter, pierce_time: 0.0,
                abrasive_delay: 0.0, re_pierce_time: 0.0, pierce_method: PierceMethod::Stationary,
            }),
        });
        return true;
    }
    let Some(m) = WorkMaterial::ALL.into_iter().find(|m| !t.cutting.iter().any(|r| r.material == Some(*m) && !r.is_recipe())) else {
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
/// stepover (#2015) - or a waterjet's recipes. They stand in the settings
/// grid's Cutting data group (#2014). The `Unknown` row is the tool's
/// default: its material is fixed and it cannot be removed.
pub(super) fn cutting_rows(ui: &mut Ui, t: &mut ToolUi, u: Unit, torch: bool) {
    cutting_rows_with(ui, t, u, torch, None);
}

/// #3896 (U5): read-only cells that the tool library window's Recipes tab
/// adds after the own fields of each speed-and-feed row (surface speed,
/// feed per tooth, the document material's mark): the column heads, then
/// one call for each drawn row.
pub(super) struct Extra<'a> {
    pub heads: Vec<String>,
    pub cells: &'a dyn Fn(&mut Ui, &CuttingRow),
}

/// [`cutting_rows`], with the `extra` cells on each speed-and-feed row.
pub(super) fn cutting_rows_with(ui: &mut Ui, t: &mut ToolUi, u: Unit, torch: bool, extra: Option<&Extra<'_>>) {
    let waterjet = t.tool_kind() == ToolKind::Waterjet;
    // #2014: no nested header here. The grid's "Cutting data" group
    // header is the one header for these rows. `main`'s plasma recipe PR
    // (#2596) wrapped this block in a CollapsingHeader; the merge of
    // `main` into this branch replaces that header with the group
    // header, with the plasma PR's rules intact (a recipe does not fill
    // a material slot). A six-column table is wider than the settings
    // column, so it scrolls across and never widens the page or an
    // auto-sized window.
    let remove = if waterjet {
        waterjet_rows(ui, t, u)
    } else {
        egui::ScrollArea::horizontal()
            .id_salt("tool_cutting_table_scroll")
            .show(ui, |ui| table(ui, t, u, torch, extra))
            .inner
    };
    if let Some(i) = remove
        && i < t.cutting.len()
    {
        t.cutting.remove(i);
    }
    let full = !waterjet
        && WorkMaterial::ALL
            .iter()
            .all(|m| t.cutting.iter().any(|r| r.material == Some(*m) && !r.is_recipe()));
    if ui
        .add_enabled(!full, egui::Button::new("Add material row"))
        .on_hover_text(if waterjet { "Add a recipe, then enter its material, thickness and pierce values" } else { "a row for the next material, starting from the rpm and feed above; the setup stock's material (else the document's) picks its row, else Unknown" })
        .clicked()
    {
        add_row(t);
    }
}

/// The speed-and-feed table; the index of a row to remove, if clicked.
fn table(ui: &mut Ui, t: &mut ToolUi, u: Unit, torch: bool, extra: Option<&Extra<'_>>) -> Option<usize> {
    // #2015: a tool whose Unknown row is derived from its own rpm / feed
    // shows it from the draft's numbers; edited, it is stored.
    let derived = CuttingRow::plain(None, t.rpm, t.feed);
    let shown = t.tool_kind() != ToolKind::Electrode && !t.cutting.iter().any(CuttingRow::is_unknown);
    if shown {
        t.cutting.insert(0, derived);
    }
    let remove = table_rows(ui, t, u, torch, extra);
    if shown && t.cutting.first() == Some(&derived) {
        t.cutting.remove(0);
        return remove.and_then(|i| i.checked_sub(1));
    }
    remove
}

fn table_rows(ui: &mut Ui, t: &mut ToolUi, u: Unit, torch: bool, extra: Option<&Extra<'_>>) -> Option<usize> {
    let keys = row_keys();
    let labels: Vec<&str> = keys.iter().skip(1).map(|m| material_label(*m)).collect();
    let mut remove = None;
    let columns = 6 + extra.map_or(0, |x| x.heads.len());
    egui::Grid::new("tool_cutting_table").num_columns(columns).striped(true).show(ui, |ui| {
        ui.label("Material");
        ui.label(if torch { "" } else { "Spindle rpm" });
        ui.label(format!("Feed ({})", u.feed_suffix()));
        ui.label("Stepdown");
        ui.label("Stepover");
        for head in extra.iter().flat_map(|x| x.heads.iter()) {
            ui.label(head);
        }
        ui.label("");
        ui.end_row();
        for (i, row) in t.cutting.iter_mut().enumerate() {
            // #1521: a torch's thickness recipe is not a speed-and-feed
            // row. It rides the draft untouched until it has an editor.
            if row.is_recipe() {
                continue;
            }
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
            if let Some(x) = extra {
                (x.cells)(ui, row);
            }
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
            prop_row(ui, "Material", |ui| {
                combo(ui, "material", &mut idx, &labels);
                if ui.small_button("Remove").on_hover_text("drop this material's row").clicked() {
                    remove = Some(i);
                }
            });
            if let Some(m) = WorkMaterial::ALL.get(idx) {
                row.material = Some(*m);
            }
            // The feed keeps the feed-rate widget: suffix `mm/min` or
            // `in/min`, feed precision, and the unit lens. A length row
            // here shows `731.0 mm` under a feed-rate label (#2663 review).
            prop_row(ui, "Feed", |ui| num_feed_row(ui, u, &mut row.feed));
            if let Some(w) = &mut row.waterjet {
                prop_row(ui, "Plate thickness", |ui| num_row(ui, u, &mut w.thickness));
                prop_row(ui, "Kerf width", |ui| num_row(ui, u, &mut w.kerf));
                prop_row(ui, "Pierce time (s)", |ui| super::fields::num_plain_row(ui, &mut w.pierce_time));
                prop_row(ui, "Abrasive delay (s)", |ui| super::fields::num_plain_row(ui, &mut w.abrasive_delay));
                prop_row(ui, "Re-pierce time (s)", |ui| super::fields::num_plain_row(ui, &mut w.re_pierce_time));
                let methods = [PierceMethod::Stationary, PierceMethod::Dynamic, PierceMethod::Wiggle];
                let mut method = methods.iter().position(|m| *m == w.pierce_method).unwrap_or(0);
                prop_row(ui, "Pierce method", |ui| {
                    combo(ui, "pierce_method", &mut method, &["Stationary", "Dynamic", "Wiggle"])
                });
                if let Some(m) = methods.get(method) { w.pierce_method = *m; }
            }
        });
    }
    remove
}

#[cfg(test)]
#[path = "tool_cutting_rows_tests.rs"]
mod tests;
