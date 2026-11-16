//! The setup's Tool list page, laid out like the WCS page (#1440, after
//! #739): titled groups - the tools the setup uses, the time and travel
//! readout, the exports - with the readout as a real table instead of
//! space-padded text lines. Read-only by law: tools live in the Tool
//! Library and operations pick them (M3.2 stage C), so the only writes
//! here are the existing `OpenToolEditor` hop and the export dialogs;
//! nothing new is a command and no project field changes.
//!
//! Provenance: the arithmetic is `camcraft_app::tool_list` (handbook
//! time = distance / rate); the layout is our own over the `layout` form
//! helpers; no vendor material.

use egui::{RichText, Ui};

use camcraft_app::{model, setup, tool_numbers};

use crate::state::State;
use camcraft_app::units;

use super::layout::group;
use super::panes::kind_name;

pub(super) fn tool_list_page(ui: &mut Ui, state: &mut State, ctx: Option<&str>) {
    group(ui, "Tools in this setup", None, |ui| {
        tool_table(ui, state, ctx)
    });
    group(ui, "Time and travel", None, |ui| readout(ui, state, ctx));
    // The two export rows keep their own widgets: `browser_files::button`
    // (#1438) routes each through the browser build's refusal, which an
    // equal-width `even_row` cannot carry.
    group(ui, "Export", None, |ui| {
        super::setup_post_page::tool_list_export(ui, state, ctx);
        super::pages::sheet_footer(ui, state);
    });
}

/// What goes into the spindle here, in change order, with the shop's time
/// law behind the estimate (M3.2 stage C).
fn readout(ui: &mut Ui, state: &mut State, ctx: Option<&str>) {
    let u = state.units();
    let rows = camcraft_app::tool_list::for_setup(&state.world, ctx);
    if rows.is_empty() {
        ui.label("Tool list: no generated operation in this setup yet.");
        return;
    }
    ui.label(
        RichText::new("In change order. Cut and rapid are path lengths.")
            .small()
            .weak(),
    );
    // The properties pane is narrow: the table scrolls sideways rather than clipping.
    egui::ScrollArea::horizontal()
        .id_salt("tool_list_readout_scroll")
        .show(ui, |ui| {
            egui::Grid::new("tool_list_readout")
                .striped(true)
                .show(ui, |ui| {
                    for h in ["Tool", "Ops", "Changes", "Cut", "Rapid", "Est (min)"] {
                        ui.label(RichText::new(h).strong());
                    }
                    ui.end_row();
                    for row in &rows {
                        ui.label(&row.tool);
                        ui.label(row.ops.len().to_string());
                        ui.label(row.changes.to_string());
                        ui.label(units::len(row.cut_mm, u, 2));
                        ui.label(units::len(row.rapid_mm, u, 2));
                        ui.label(camcraft_app::setup_sheet::minutes(row.minutes));
                        ui.end_row();
                    }
                });
        });
    ui.label(format!(
        "Setup total: est {} min",
        camcraft_app::setup_sheet::minutes(camcraft_app::tool_list::setup_minutes(
            &state.world,
            ctx
        )),
    ));
}

/// The tools this setup's operations use, in program order: number,
/// size, kind, how many operations - and the editor one click away.
/// Tools live in the document's library (decision #343 (b)), so the note
/// says plainly that an edit reaches every setup using the tool.
fn tool_table(ui: &mut Ui, state: &mut State, ctx: Option<&str>) {
    let used = tools_of(state, ctx);
    if used.is_empty() {
        ui.label("No operation in this setup uses a tool yet.");
        return;
    }
    ui.label(RichText::new("Tools live in the document's tool library: editing one here edits it in every setup that uses it.").small().weak());
    let mut open: Option<String> = None;
    egui::ScrollArea::horizontal()
        .id_salt("setup_tool_table_scroll")
        .show(ui, |ui| {
            egui::Grid::new("setup_tool_table")
                .striped(true)
                .show(ui, |ui| {
                    for h in ["T", "Tool", "Size", "Ops", ""] {
                        ui.label(RichText::new(h).strong());
                    }
                    ui.end_row();
                    for (tool, ops) in &used {
                        let number = tool_numbers::number_of(&state.world, tool)
                            .map_or_else(|| "?".to_owned(), |n| n.to_string());
                        ui.label(number);
                        ui.horizontal(|ui| {
                            super::tool_icons::named_glyph(ui, &state.world, tool);
                            ui.label(tool);
                        });
                        let size = model::find_by_name(&state.world, tool)
                            .and_then(|e| model::tool_of(&state.world, &e))
                            .map_or_else(
                                || "missing".to_owned(),
                                |t| format!("Ø{} {}", t.diameter, kind_name(t.kind)),
                            );
                        ui.label(size);
                        ui.label(ops.to_string());
                        if ui.small_button("Edit").clicked() {
                            open = Some(tool.clone());
                        }
                        ui.end_row();
                    }
                });
        });
    let names: Vec<String> = used.iter().map(|(t, _)| t.clone()).collect();
    if let Some(why) = tool_numbers::clash(&state.world, &names) {
        let ink = super::colorvision::text_inks(ui, &state.world).error;
        super::error_text::error_text(ui, ink, &why);
    }
    if let Some(tool) = open {
        crate::actions::open_tool_editor(state, &tool);
    }
}

/// (tool, operation count) for the setup's operations, in program order.
fn tools_of(state: &State, ctx: Option<&str>) -> Vec<(String, usize)> {
    let mut out: Vec<(String, usize)> = Vec::new();
    for (_, e) in setup::operation_order(&state.world) {
        let Some(op) = model::operation_of(&state.world, &e) else {
            continue;
        };
        if let Some(name) = ctx
            && !setup::belongs(&state.world, op.setup.as_deref(), name)
        {
            continue;
        }
        match out.iter_mut().find(|(t, _)| *t == op.tool) {
            Some((_, n)) => *n += 1,
            None => out.push((op.tool.clone(), 1)),
        }
    }
    out
}
