//! The Tool library page's Export / Import library buttons (#300, #2566): a
//! file dialog, then `tool_table::export` to write (`.camtools`, PathPilot
//! `.csv`, or Hurco `.TCM`), or `tool_table::parse_import` + `ImportTools`
//! (one undo unit) to read. A refusal lands on the Tool pane by name, never
//! swallowed.
//! #889: the page's "Import tool holders" choice is the command's
//! `holders` parameter (`ImportTools::with_options`), remembered per user.
//! Provenance: our own `camcraft-tools/1` format (`app::tool_library`) and
//! public machine tool-table layouts (`app::tool_table`).

use std::path::{Path, PathBuf};

use camcraft_app::commands::ImportTools;
use camcraft_app::tool_library::{self, EXTENSION};
use camcraft_app::tool_table::{self, ToolTableFormat};

use super::{ErrorScope, State};

impl State {
    /// "Export library…": ask where, then write every document tool.
    pub(crate) fn export_tool_library_dialog(&mut self) {
        if self.refuse_in_browser(crate::browser_files::NO_EXPORT) {
            return;
        }
        let Some(path) = crate::file_dialog::FileDialog::new()
            .add_filter("CamCraft Tool Library", &[EXTENSION])
            .add_filter("Tormach PathPilot Tool Table", &["csv"])
            .add_filter("Hurco WinMax Tool Library", &["tcm", "TCM"])
            .set_file_name(format!("tools.{EXTENSION}"))
            .save_file()
        else {
            return;
        };
        self.export_tool_library_to(&tool_library::with_library_extension(&path));
    }

    /// "Import library…": pick a file, then import it.
    pub(crate) fn import_tool_library_dialog(&mut self) {
        if self.refuse_in_browser(crate::browser_files::NEEDS_PICKER) {
            return;
        }
        let picked: Option<PathBuf> = crate::file_dialog::FileDialog::new()
            .add_filter("Tool Libraries", &[EXTENSION, "json", "csv", "tcm", "TCM"])
            .add_filter("CamCraft Tool Library", &[EXTENSION, "json"])
            .add_filter("Tormach PathPilot Tool Table", &["csv"])
            .add_filter("Hurco WinMax Tool Library", &["tcm", "TCM"])
            .pick_file();
        if let Some(path) = picked {
            self.import_tool_library_from(&path);
        }
    }

    /// Tick or untick "Import tool holders": the person's choice, so it
    /// is written to their preferences, never to the document or history.
    pub(crate) fn set_import_tool_holders(&mut self, on: bool) {
        if self.import_tool_holders != on {
            self.import_tool_holders = on;
            self.persist_prefs();
        }
    }

    pub(crate) fn export_tool_library_to(&mut self, path: &Path) {
        let format = tool_table::detect_format(path, None).unwrap_or(ToolTableFormat::CamTools);
        let unit = match format {
            ToolTableFormat::CamTools => camcraft_app::units::Unit::Millimeters,
            ToolTableFormat::PathPilot | ToolTableFormat::Hurco => self.units(),
        };
        let written = tool_table::export(&self.world, format, unit)
            .and_then(|text| std::fs::write(path, text).map_err(|e| e.to_string()));
        match written {
            Ok(()) => self.status = format!("tool library written to {}", path.display()),
            Err(err) => self.refuse(ErrorScope::Tool, format!("tool library not exported: {err}")),
        }
    }

    pub(crate) fn import_tool_library_from(&mut self, path: &Path) {
        let unit = self.units();
        let planned = std::fs::read_to_string(path)
            .map_err(|e| e.to_string())
            .and_then(|text| tool_table::parse_import(&text, None, path, unit, false))
            .and_then(|(_, lib)| {
                ImportTools::with_options(&self.world, lib, self.tool_import_mode, self.import_tool_holders)
                    .map_err(|e| e.to_string())
            });
        match planned {
            Ok(command) => {
                let summary = command.report().summary();
                self.history.execute(&self.world, command);
                self.doc_version += 1;
                self.status = summary;
            }
            Err(err) => self.refuse(ErrorScope::Tool, format!("tool library not imported: {err}")),
        }
    }
}
