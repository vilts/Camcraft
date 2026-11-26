//! #3892 (U1 of #3663): the tools list of the tool library window - the
//! file buttons (New, Export, Import and its mode, the holders choice),
//! the source (Document or Catalog), the Tool type and Name filters, and
//! the rows. Moved here from the retired dock tab (#2002, #2029); the
//! labels stay, so the tests and the MCP-driven flows still find them.
//! #3897 (U6): the tool categories stand on the left of the rows
//! (`categories`), and each Document row starts with a small drawing of
//! the tool (`thumbs`, cached) and has a Gauge column: the holder's gauge
//! length plus the stick-out, for a tool in a library holder.
//! Provenance: our own catalog and library controls; no HSMWorks material
//! involved.

use super::super::panes::kind_name;
use crate::{
    actions,
    state::{CommandKind, ErrorScope, State, ToolSource},
};
use camcraft_app::tools::{self, Tool, ToolKind};
use camcraft_app::{model, tool_body, tool_numbers, units};
use egui::{RichText, Ui};

/// The column heads of the Document list (#3663 section 1).
const DOCUMENT_HEADS: [&str; 11] = ["", "T", "Tool", "Kind", "Ø", "Flute length", "Flutes", "Holder", GAUGE, "Recipes", ""];
/// #3897: the Gauge column's head.
pub(crate) const GAUGE: &str = "Gauge length";
/// The width of the category column.
const CATEGORY_W: f32 = 170.0;

pub(super) fn draw(ui: &mut Ui, state: &mut State) {
    file_row(ui, state);
    ui.separator();
    let Some(source) = state.tool_library.as_ref().map(|v| v.source) else {
        return;
    };
    let mut picked = source;
    ui.horizontal(|ui| {
        ui.selectable_value(&mut picked, ToolSource::Document, "Document")
            .on_hover_text("the tools of this document");
        ui.selectable_value(&mut picked, ToolSource::Catalog, "Catalog")
            .on_hover_text("the built-in catalog: Add puts a tool into the document");
    });
    if picked != source
        && let Some(view) = state.tool_library.as_mut()
    {
        view.source = picked;
    }
    let rows = match picked {
        ToolSource::Document => document_tools(state),
        ToolSource::Catalog => catalog_tools(state),
    };
    filter_row(ui, state, &rows);
    let category = state.tool_library.as_ref().and_then(|v| v.category);
    ui.horizontal_top(|ui| {
        let pick = ui
            .vertical(|ui| {
                ui.set_width(CATEGORY_W);
                egui::ScrollArea::vertical()
                    .id_salt("tool_library_categories")
                    .auto_shrink([false, false])
                    .show(ui, |ui| super::categories::draw(ui, &rows, category))
                    .inner
            })
            .inner;
        if let Some(pick) = pick
            && let Some(view) = state.tool_library.as_mut()
        {
            view.category = pick;
        }
        ui.separator();
        egui::ScrollArea::both()
            .id_salt("tool_library_rows")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                let shown = filtered(state, rows);
                if shown.is_empty() {
                    ui.label("No tools match the filters.");
                    return;
                }
                match picked {
                    ToolSource::Document => document_list(ui, state, &shown),
                    ToolSource::Catalog => catalog_list(ui, state, &shown),
                }
            });
    });
}

/// New, Export, Import (with its mode) and the holders choice, then the
/// Tool pane's refusal, if any.
fn file_row(ui: &mut Ui, state: &mut State) {
    // Wrapped: a narrow window must not clip a button (a clipped button is a hidden one).
    let (new, export, import) = ui
        .horizontal_wrapped(|ui| {
            let new = ui.button("New tool…").clicked();
            let export = crate::browser_files::button(
                ui,
                state.in_browser(crate::browser_files::NO_EXPORT),
                "Export library…",
                "write the document's tools to a CamCraft (.camtools), PathPilot (.csv) or Hurco (.TCM) file",
            );
            let import = crate::browser_files::button(
                ui,
                state.in_browser(crate::browser_files::NEEDS_PICKER),
                "Import library…",
                "import tools from a CamCraft (.camtools/.json), PathPilot (.csv) or Hurco (.TCM) file (one undo step; the mode beside it says what happens to a name the document has)",
            );
            tool_import_mode_combo(ui, &mut state.tool_import_mode);
            (new, export, import)
        })
        .inner;
    // #889: whether an import takes each tool's holder body from the file.
    let mut holders = state.import_tool_holders;
    if ui
        .checkbox(&mut holders, "Import tool holders")
        .on_hover_text("take each tool's holder body from the library file; unticked, tools arrive with their holder cylinder only")
        .changed()
    {
        state.set_import_tool_holders(holders);
    }
    if new {
        actions::begin_new_tool(state);
    }
    if export {
        state.export_tool_library_dialog();
    }
    if import {
        state.import_tool_library_dialog();
    }
    if let Some(msg) = state.error_for(ErrorScope::Tool).map(str::to_owned) {
        super::super::error_text::error_text(ui, super::super::colorvision::text_inks(ui, &state.world).error, &msg);
    }
}

/// The document's tools, by name, in document order.
pub(super) fn document_tools(state: &State) -> Vec<(String, Tool)> {
    state
        .tool_names()
        .into_iter()
        .filter_map(|name| {
            let tool = model::find_by_name(&state.world, &name).and_then(|e| model::tool_of(&state.world, &e))?;
            Some((name, tool))
        })
        .collect()
}

/// The catalog tools of the kinds the machine can use.
fn catalog_tools(state: &State) -> Vec<(String, Tool)> {
    tools::catalog()
        .into_iter()
        .filter(|(_, t)| crate::machine_modes::tool_kind_offered(&state.world, t.kind))
        .map(|(n, t)| (n.to_owned(), t))
        .collect()
}

/// The Tool type choice lists the kinds of the shown source; a kind the
/// machine cannot use is dropped from the filter.
fn filter_row(ui: &mut Ui, state: &mut State, rows: &[(String, Tool)]) {
    if state
        .tool_catalog_kind
        .is_some_and(|kind| !crate::machine_modes::tool_kind_offered(&state.world, kind))
    {
        state.tool_catalog_kind = None;
    }
    let mut kinds: Vec<ToolKind> = Vec::new();
    for (_, t) in rows {
        if !kinds.contains(&t.kind) {
            kinds.push(t.kind);
        }
    }
    egui::ComboBox::from_id_salt("catalog_tool_type")
        .selected_text(state.tool_catalog_kind.map(kind_name).unwrap_or("All types"))
        .show_ui(ui, |ui| {
            ui.selectable_value(&mut state.tool_catalog_kind, None, "All types");
            for kind in kinds {
                ui.selectable_value(&mut state.tool_catalog_kind, Some(kind), kind_name(kind));
            }
        })
        .response
        .labelled_by(ui.label("Tool type").id);
    let name_label = ui.label("Name filter");
    ui.add(egui::TextEdit::singleline(&mut state.tool_catalog_name).desired_width(ui.available_width()))
        .labelled_by(name_label.id);
}

/// The rows that pass the category (#3897), the Tool type and the Name
/// filters (the name ignores case).
fn filtered(state: &State, rows: Vec<(String, Tool)>) -> Vec<(String, Tool)> {
    let name_filter = state.tool_catalog_name.to_lowercase();
    let category = state.tool_library.as_ref().and_then(|v| v.category);
    rows.into_iter()
        .filter(|(name, t)| {
            super::categories::passes(category, t)
                && state.tool_catalog_kind.is_none_or(|kind| t.kind == kind)
                && name.to_lowercase().contains(&name_filter)
        })
        .collect()
}

/// The Document list: one row for each tool with the columns of #3663.
/// #3893: "Edit" opens the tool in this window's editor (view state); the
/// Properties subject and the document selection do not change.
fn document_list(ui: &mut Ui, state: &mut State, rows: &[(String, Tool)]) {
    let numbers = tool_numbers::numbers(&state.world).unwrap_or_default();
    let u = state.units();
    let mut edit: Option<String> = None;
    egui::Grid::new("tool_library_document").striped(true).show(ui, |ui| {
        for h in DOCUMENT_HEADS {
            ui.label(RichText::new(h).strong());
        }
        ui.end_row();
        for (name, t) in rows {
            let number = numbers.iter().find(|(n, _)| n == name).map_or_else(|| "?".to_owned(), |(_, t)| t.to_string());
            super::thumbs::draw(ui, name, t);
            ui.label(number);
            ui.horizontal(|ui| {
                super::super::tool_icons::glyph(ui, super::super::tool_icons::catalog_row_icon(t));
                ui.label(name.as_str());
            });
            ui.label(kind_name(t.kind));
            ui.label(format!("{}", t.diameter));
            ui.label(format!("{}", t.flute_len));
            ui.label(t.flutes.map_or_else(|| "-".to_owned(), |z| z.to_string()));
            let (holder, gauge) = holder_of(state, name);
            ui.label(holder);
            ui.label(gauge.map_or_else(|| "-".to_owned(), |g| units::len(g, u, 2)))
                .on_hover_text("the holder's gauge length plus the stick-out (a tool in a library holder only)");
            ui.label(t.cutting.len().to_string());
            if ui.small_button("Edit").on_hover_text("edit this tool in this window").clicked() {
                edit = Some(name.clone());
            }
            ui.end_row();
        }
    });
    if let Some(name) = edit {
        state.open_tool_in_library(&name);
    }
}

/// The library holder on the tool ("-" when it has none) and, #3897, the
/// tool's gauge length in it: the holder's gauge length plus the stick-out.
fn holder_of(state: &State, name: &str) -> (String, Option<f64>) {
    let r = model::find_by_name(&state.world, name).and_then(|e| tool_body::body_of(&state.world, &e)).and_then(|b| b.library);
    match r {
        Some(r) => {
            let gauge = r.gauge_len + r.projection;
            (r.entry, gauge.is_finite().then_some(gauge))
        }
        None => ("-".to_owned(), None),
    }
}

/// The Catalog list: one row for each tool, with Add when the document
/// does not have a tool of that name yet. Add is the `AddTool` command.
fn catalog_list(ui: &mut Ui, state: &mut State, rows: &[(String, Tool)]) {
    for (cname, ct) in rows {
        let taken = model::find_by_name(&state.world, cname).is_some();
        let clicked = ui
            .horizontal(|ui| {
                super::super::tool_icons::glyph(ui, super::super::tool_icons::catalog_row_icon(ct));
                ui.label(format!("{}  Ø{} {}", cname, ct.diameter, kind_name(ct.kind)));
                if taken {
                    ui.label("(in document)");
                    false
                } else {
                    ui.button("Add").clicked()
                }
            })
            .inner;
        if clicked {
            state.pending.push_back(CommandKind::AddTool { name: cname.clone(), tool: ct.clone() });
        }
    }
}

/// #899: the Import library mode picker (view state; the import is the command).
fn tool_import_mode_combo(ui: &mut egui::Ui, mode: &mut camcraft_app::commands::ImportMode) {
    use camcraft_app::commands::ImportMode;
    egui::ComboBox::from_id_salt("tool_import_mode")
        .selected_text(match mode {
            ImportMode::Add => "Add (rename duplicates)",
            ImportMode::Update => "Update same-named",
            ImportMode::Replace => "Replace library",
        })
        .show_ui(ui, |ui| {
            ui.selectable_value(mode, ImportMode::Add, "Add (rename duplicates)")
                .on_hover_text("an identical tool is skipped, a different tool of a taken name comes in as \"Name (2)\"");
            ui.selectable_value(mode, ImportMode::Update, "Update same-named")
                .on_hover_text("a same-named tool takes the file's definition in place (number kept; operations using it go Stale); new names are added");
            ui.selectable_value(mode, ImportMode::Replace, "Replace library")
                .on_hover_text("update, and tools the file lacks are removed - refused if an operation uses one");
        });
}
