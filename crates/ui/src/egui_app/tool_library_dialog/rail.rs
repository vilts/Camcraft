//! #3893 (U2 of #3663): the rail - the tools list folded to one narrow
//! column while a tool is open in the window's editor. A row opens that
//! tool in the editor; "All tools" unfolds the full list again. A pick
//! here changes neither the document selection nor Properties.
//! Provenance: our own list-detail layout over egui selectable labels; no
//! HSMWorks material involved.

use egui::Ui;

use crate::state::State;
use camcraft_app::tool_numbers;

/// What the user picked in the rail this frame.
pub(super) enum Pick {
    /// Back to the full list.
    List,
    /// Open this tool in the editor.
    Tool(String),
}

/// The "All tools" button, then one row for each document tool, by tool
/// number and name. The open tool is the selected row.
pub(super) fn draw(ui: &mut Ui, state: &State) -> Option<Pick> {
    let open = state.tool_library.as_ref().and_then(|v| v.open_tool()).unwrap_or_default().to_owned();
    let mut pick = None;
    if ui.button("All tools").on_hover_text("close the editor and show the full list").clicked() {
        pick = Some(Pick::List);
    }
    ui.separator();
    let numbers = tool_numbers::numbers(&state.world).unwrap_or_default();
    egui::ScrollArea::vertical()
        .id_salt("tool_library_rail")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            for (name, tool) in super::list::document_tools(state) {
                let number = numbers.iter().find(|(n, _)| *n == name).map(|(_, t)| *t);
                ui.horizontal(|ui| {
                    super::super::tool_icons::glyph(ui, super::super::tool_icons::catalog_row_icon(&tool));
                    ui.label(number.map_or_else(|| "T?".to_owned(), |t| format!("T{t}")));
                    if ui.selectable_label(name == open, name.as_str()).clicked() && name != open {
                        pick = Some(Pick::Tool(name.clone()));
                    }
                });
            }
        });
    pick
}
