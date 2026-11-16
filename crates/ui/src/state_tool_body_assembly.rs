//! The tool editor's modular piece-assembly actions (#361 / #1040):
//! attach an adapter, extension or clamping unit onto a landed tool
//! body by ISO 13399 CSYS (auto or named) or remove a piece, always as
//! one undoable `SetToolBody` command. Provenance: our own pane state
//! over `app::tool_body_assembly`; no vendor material consulted.

use std::path::{Path, PathBuf};

use camcraft_app::commands::SetToolBody;
use camcraft_app::tool_body_assembly::PiecePlace;

use super::{CommandKind, ErrorScope, State};

/// Ephemeral UI fields for attaching the next modular piece.
#[derive(Clone, Debug, Default)]
pub struct AssemblyUi {
    /// Existing piece to mate against (None = search the whole assembly).
    pub target_piece: Option<usize>,
    /// Optional target CSYS name (blank = auto-mate).
    pub target_csys: String,
    /// Optional incoming piece CSYS name (blank = auto-mate).
    pub piece_csys: String,
}

fn non_empty(s: &str) -> Option<String> {
    let t = s.trim();
    (!t.is_empty()).then(|| t.to_owned())
}

impl State {
    /// Selected placement rule for attaching a piece from the editor.
    #[must_use]
    pub(crate) fn editor_piece_place(&self) -> PiecePlace {
        PiecePlace::Csys {
            target_piece: self.tool_body_ui.assembly.target_piece,
            target_csys: non_empty(&self.tool_body_ui.assembly.target_csys),
            piece_csys: non_empty(&self.tool_body_ui.assembly.piece_csys),
        }
    }

    /// "Attach piece…": pick a STEP / STL / OBJ file and queue `SetToolBody::attach_piece`.
    pub(crate) fn attach_tool_body_piece_dialog(&mut self) {
        if self.refuse_in_browser(crate::browser_files::NEEDS_PICKER) {
            return;
        }
        if let Some(path) = super::pick_mesh_file() {
            self.attach_tool_body_piece_from(&path);
        }
    }

    /// Queue attaching `path` onto the editor's current tool using the UI's CSYS fields.
    pub(crate) fn attach_tool_body_piece_from(&mut self, path: &Path) {
        let place = self.editor_piece_place();
        self.queue_attach_tool_body_piece(path.to_path_buf(), place);
    }

    pub(crate) fn queue_attach_tool_body_piece(&mut self, path: PathBuf, place: PiecePlace) {
        self.pending.push_back(CommandKind::AttachToolBodyPiece {
            name: self.tool_ui.name.clone(),
            path,
            place,
        });
    }

    pub(crate) fn queue_remove_tool_body_piece(&mut self, index: usize) {
        self.pending.push_back(CommandKind::RemoveToolBodyPiece {
            name: self.tool_ui.name.clone(),
            index,
        });
    }

    pub(crate) fn run_attach_tool_body_piece(&mut self, name: &str, path: &Path, place: PiecePlace) {
        match SetToolBody::attach_piece(&self.world, name, path, place) {
            Ok(command) => {
                self.history.execute(&self.world, command);
                self.doc_version += 1;
                self.status = format!("tool {name}: attached piece");
            }
            Err(err) => self.refuse(ErrorScope::Tool, format!("piece not attached: {err}")),
        }
    }

    pub(crate) fn run_remove_tool_body_piece(&mut self, name: &str, index: usize) {
        match SetToolBody::remove_piece(&self.world, name, index) {
            Ok(command) => {
                self.history.execute(&self.world, command);
                self.doc_version += 1;
                self.status = format!("tool {name}: removed piece {index}");
            }
            Err(err) => self.refuse(ErrorScope::Tool, format!("piece not removed: {err}")),
        }
    }
}
