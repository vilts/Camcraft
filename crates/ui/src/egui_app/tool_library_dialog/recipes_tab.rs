//! #3896 (U5 of #3663): the Recipes tab of the tool library window's
//! editor - the tool's speeds and feeds for each workpiece material. It
//! draws the same rows as Properties' Cutting data group
//! (`tool_cutting_rows::cutting_rows_with`), with read-only columns for
//! the surface speed and the feed per tooth, and a mark on the row that
//! the document's material runs. "Suggest" fills a row from the cutting
//! table and "Copy from tool" takes the rows of another tool. Both write
//! the window's draft only: the editor's one Apply lands it as
//! `ChangeTool`, one undo step. No command is new.
//! #3930: the rest of Properties' old Cutting data group (the tool's own
//! spindle rpm and feed, the lathe rows, centre cutting) follows the table
//! (`tool_settings_grid::cutting_base`), so the summary page loses no edit.
//! Provenance: our own tab over the tool draft and the handbook relations
//! in `recipes_fill`; no HSMWorks material involved.

use egui::{Id, RichText, Ui};

use camcraft_app::cutting;
use camcraft_app::model;
use camcraft_app::tool_cutting_rows::CuttingRow;
use camcraft_app::tools::{ToolKind, ToolMaterial, WorkMaterial};
use camcraft_app::units::Unit;

use super::super::fields::combo;
use super::super::layout::{prop_row, prop_row_hint};
use super::super::tool_cutting_rows::{Extra, cutting_rows, cutting_rows_with};
use super::recipes_fill::{
    copy_rows, copyable, feed_per_tooth, is_marked, names_material, rotating, suggest, suggest_refusal,
    surface_speed,
};
use crate::state::{State, ToolUi};

/// The tab's name, and its AccessKit label.
pub(crate) const TAB: &str = "Recipes";
/// The mark on the row that the document's material runs.
pub(crate) const MARK: &str = "document material";
/// The button that fills a row from the cutting table.
pub(crate) const SUGGEST: &str = "Suggest";
/// The button that copies the rows of another tool.
pub(crate) const COPY: &str = "Copy from tool";

/// The flute counts the combo offers; a stated count outside it is shown
/// as its own entry and never rewritten (the operation page's rule).
const FLUTES: [u32; 7] = [1, 2, 3, 4, 5, 6, 8];

pub(super) fn draw(ui: &mut Ui, state: &mut State) {
    rows(ui, state);
    ui.separator();
    super::super::tool_settings_grid::cutting_base(ui, state);
}

/// The per-material rows, with Suggest and Copy for a rotating tool.
fn rows(ui: &mut Ui, state: &mut State) {
    let kind = state.tool_ui.tool_kind();
    if kind == ToolKind::Wire {
        note(ui, "An EDM wire has no cutting rows: its cut is the machine's EDM condition.");
        return;
    }
    let u = state.units();
    let torch = matches!(kind, ToolKind::Torch | ToolKind::Waterjet);
    if !rotating(kind) {
        cutting_rows(ui, &mut state.tool_ui, u, torch);
        return;
    }
    let doc = cutting::material_of(&state.world);
    edge_rows(ui, &mut state.tool_ui);
    suggest_row(ui, state, doc);
    copy_row(ui, state);
    note(ui, &match doc {
        Some(m) => format!("Document material: {}. The marked row is the one its operations run.", m.label()),
        None => "No workpiece material is stated (an operation's Feeds tab sets it), so no row is marked.".to_owned(),
    });
    let t = &state.tool_ui;
    let (d, z) = (t.diameter, t.flutes);
    let named = doc.is_some_and(|m| names_material(&t.cutting, m));
    let cells = move |ui: &mut Ui, row: &CuttingRow| {
        ui.label(surface_speed(d, row.rpm).map_or_else(|| "-".to_owned(), |vc| vc_text(vc, u)));
        match feed_per_tooth(row.feed, row.rpm, z) {
            Some(fz) => ui.label(fz_text(fz, u)),
            None => ui.label("-").on_hover_text("needs a spindle speed and a stated flute count"),
        };
        if is_marked(row, doc, named) {
            ui.label(RichText::new(MARK).italics().color(ui.visuals().text_color()))
                .on_hover_text("the row that operations on the document's workpiece material run");
        } else {
            ui.label("");
        }
    };
    let extra = Extra { heads: heads(u), cells: &cells };
    cutting_rows_with(ui, &mut state.tool_ui, u, false, Some(&extra));
}

/// A dimmed line of the tab (the form's `hint` would turn into the
/// tooltip of the row above).
fn note(ui: &mut Ui, text: &str) {
    ui.add(egui::Label::new(RichText::new(text).weak()).wrap());
}

/// The read-only column heads, in the document's units.
fn heads(u: Unit) -> Vec<String> {
    let (vc, fz) = match u {
        Unit::Millimeters => ("Vc (m/min)", "fz (mm)"),
        Unit::Inches => ("Vc (sfm)", "fz (in)"),
    };
    vec![vc.to_owned(), fz.to_owned(), String::new()]
}

/// Surface speed: m/min, or surface feet per minute in an inch document.
fn vc_text(vc: f64, u: Unit) -> String {
    match u {
        Unit::Millimeters => format!("{vc:.0}"),
        Unit::Inches => format!("{:.0}", vc * 1000.0 / 304.8),
    }
}

/// Feed per tooth: mm, or inch in an inch document.
fn fz_text(fz: f64, u: Unit) -> String {
    match u {
        Unit::Millimeters => format!("{fz:.3}"),
        Unit::Inches => format!("{:.4}", fz / 25.4),
    }
}

/// The tool's edge material and flute count: the inputs of the feed per
/// tooth and of "Suggest". Draft fields; Apply lands them.
fn edge_rows(ui: &mut Ui, t: &mut ToolUi) {
    let tm_names = ["Not stated", ToolMaterial::Hss.label(), ToolMaterial::Carbide.label()];
    let mut tm = t.tool_material.map_or(0, |m| m as usize + 1);
    prop_row_hint(ui, "Tool material", "the cutting edge's material; the table's speeds depend on it", |ui| {
        combo(ui, "tool_recipes_tool_material", &mut tm, &tm_names)
    });
    t.tool_material = tm.checked_sub(1).and_then(|i| ToolMaterial::ALL.get(i).copied());
    let odd = t.flutes.filter(|z| !FLUTES.contains(z));
    let names: Vec<String> = std::iter::once("Not stated".to_owned())
        .chain(FLUTES.iter().chain(odd.iter()).map(u32::to_string))
        .collect();
    let refs: Vec<&str> = names.iter().map(String::as_str).collect();
    let mut z = match (odd, t.flutes) {
        (Some(_), _) => names.len() - 1,
        (None, Some(n)) => FLUTES.iter().position(|f| *f == n).map_or(0, |i| i + 1),
        (None, None) => 0,
    };
    prop_row(ui, "Flutes", |ui| combo(ui, "tool_recipes_flutes", &mut z, &refs));
    t.flutes = names.get(z).and_then(|n| n.parse::<u32>().ok());
}

/// "Suggest for <material>": fills that material's row from the cutting
/// table, clamped to the document machine's stated limits.
fn suggest_row(ui: &mut Ui, state: &mut State, doc: Option<WorkMaterial>) {
    let id = Id::new("tool_recipes_suggest_for");
    let fallback = doc.map_or(0, |m| m as usize);
    let mut idx = ui.data(|d| d.get_temp::<usize>(id)).unwrap_or(fallback);
    let before = idx;
    let labels: Vec<&str> = WorkMaterial::ALL.iter().map(|m| m.label()).collect();
    let refusal = suggest_refusal(&state.tool_ui);
    let clicked = prop_row_hint(ui, "Suggest for", "fill this material's row from the cutting table; Apply writes it", |ui| {
        let b = ui
            .add_enabled(refusal.is_none(), egui::Button::new(SUGGEST))
            .on_hover_text("the table's spindle speed and feed for this tool on this material")
            .on_disabled_hover_text(refusal.as_deref().unwrap_or_default())
            .clicked();
        combo(ui, "tool_recipes_suggest_for", &mut idx, &labels);
        b
    });
    if idx != before {
        ui.data_mut(|d| d.insert_temp(id, idx));
    }
    let Some(m) = WorkMaterial::ALL.get(idx).copied() else {
        return;
    };
    if clicked {
        let machine = camcraft_app::machine::of_setup(&state.world, None).1;
        state.status = match suggest(&mut state.tool_ui, m, machine.as_ref()) {
            Ok(()) => format!("{} row suggested - Apply writes it to the tool", m.label()),
            Err(why) => format!("No suggestion: {why}"),
        };
    }
}

/// "Copy from tool": another document tool's material rows into the draft.
fn copy_row(ui: &mut Ui, state: &mut State) {
    let me = state.tool_ui.name.clone();
    let others: Vec<String> = state.tool_names().into_iter().filter(|n| *n != me).collect();
    let id = Id::new("tool_recipes_copy_from");
    let kept = ui.data(|d| d.get_temp::<String>(id));
    let mut idx = kept.and_then(|k| others.iter().position(|n| *n == k)).unwrap_or(0);
    let before = idx;
    let source = others.get(idx).cloned();
    let rows: Vec<CuttingRow> = source
        .as_deref()
        .and_then(|n| model::find_by_name(&state.world, n))
        .and_then(|e| model::tool_of(&state.world, &e))
        .map(|t| copyable(&t.cutting).copied().collect())
        .unwrap_or_default();
    let why = match &source {
        None => Some("no other tool is in the document".to_owned()),
        Some(n) if rows.is_empty() => Some(format!("tool {n} has no material rows")),
        Some(_) => None,
    };
    let names: Vec<&str> = others.iter().map(String::as_str).collect();
    let clicked = prop_row_hint(ui, "Copy from", "the other tool's material rows replace or join this draft's", |ui| {
        let b = ui
            .add_enabled(why.is_none(), egui::Button::new(COPY))
            .on_hover_text("copy the material rows into this draft; Apply writes them")
            .on_disabled_hover_text(why.as_deref().unwrap_or_default())
            .clicked();
        if !names.is_empty() {
            combo(ui, "tool_recipes_copy_from", &mut idx, &names);
        }
        b
    });
    if idx != before
        && let Some(n) = others.get(idx)
    {
        ui.data_mut(|d| d.insert_temp(id, n.clone()));
    }
    if clicked && let Some(n) = source {
        let count = copy_rows(&mut state.tool_ui, &rows);
        state.status = format!("{count} rows copied from tool {n} - Apply writes them to the tool");
    }
}
