//! The independent docked tool catalog (#2002).
//! Provenance: our own catalog and library controls extracted from pages.rs.

use super::panes::kind_name;
use crate::{
    actions,
    state::{CommandKind, ErrorScope, State},
};
use camcraft_app::{model, tools};
use egui::Ui;

pub(crate) fn draw(ui: &mut Ui, state: &mut State) {
    egui::ScrollArea::vertical().show(ui, |ui| {
        ui.heading("Tool library");
    // Wrapped: the pane is narrow, and a clipped button is a hidden one.
    let (new, export, import) = ui
        .horizontal_wrapped(|ui| {
            let new = ui.button("New tool…").clicked();
            let export = crate::browser_files::button(
                ui,
                state.in_browser(crate::browser_files::NO_EXPORT),
                "Export library…",
                "write the document's tools to a CamCraft tool library file",
            );
            let import = crate::browser_files::button(
                ui,
                state.in_browser(crate::browser_files::NEEDS_PICKER),
                "Import library…",
                "import the tools of a CamCraft tool library file (one undo step; the mode beside it says what happens to a name the document has)",
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
        actions::open_tool_editor(state, "");
    }
    if export {
        state.export_tool_library_dialog();
    }
    if import {
        state.import_tool_library_dialog();
    }
    if let Some(msg) = state.error_for(ErrorScope::Tool).map(str::to_owned) {
        super::error_text::error_text(ui, super::colorvision::text_inks(ui, &state.world).error, &msg);
    }
    ui.separator();
    ui.heading("Catalog");
    let catalog = tools::catalog();
    if state
        .tool_catalog_kind
        .is_some_and(|kind| !crate::machine_modes::tool_kind_offered(&state.world, kind))
    {
        state.tool_catalog_kind = None;
    }
    egui::ComboBox::from_id_salt("catalog_tool_type")
        .selected_text(state.tool_catalog_kind.map(kind_name).unwrap_or("All types"))
        .show_ui(ui, |ui| {
            ui.selectable_value(&mut state.tool_catalog_kind, None, "All types");
            let mut kinds = Vec::new();
            for (_, tool) in &catalog {
                if !kinds.contains(&tool.kind) && crate::machine_modes::tool_kind_offered(&state.world, tool.kind) {
                    kinds.push(tool.kind);
                    ui.selectable_value(&mut state.tool_catalog_kind, Some(tool.kind), kind_name(tool.kind));
                }
            }
        }).response.labelled_by(ui.label("Tool type").id);
    let name_label = ui.label("Name filter");
    ui.add(egui::TextEdit::singleline(&mut state.tool_catalog_name)
        .desired_width(ui.available_width()))
        .labelled_by(name_label.id);
    let name_filter = state.tool_catalog_name.to_lowercase();
    let mut matches = 0;
    for (cname, ct) in catalog {
        if !crate::machine_modes::tool_kind_offered(&state.world, ct.kind)
            || state.tool_catalog_kind.is_some_and(|kind| ct.kind != kind)
            || !cname.to_lowercase().contains(&name_filter)
        {
            continue;
        }
        matches += 1;
        let taken = model::find_by_name(&state.world, cname).is_some();
        let clicked = ui
            .horizontal(|ui| {
                super::tool_icons::glyph(ui, super::tool_icons::catalog_row_icon(&ct));
                ui.label(format!(
                    "{}  Ø{} {}",
                    cname,
                    ct.diameter,
                    kind_name(ct.kind)
                ));
                if taken {
                    ui.label("(in document)");
                    false
                } else {
                    ui.button("Add").clicked()
                }
            })
            .inner;
        if clicked {
            state.pending.push_back(CommandKind::AddTool {
                name: cname.to_owned(),
                tool: ct,
            });
        }
    }
    if matches == 0 {
        ui.label("No tools match the filters.");
    }

    });
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
