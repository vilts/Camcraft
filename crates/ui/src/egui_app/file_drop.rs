//! Drag and drop a model file onto the window (issue #1450). While a file
//! is dragged over the window the viewport wears a plain "Drop to load
//! model" hint; a single drop of a mesh the Model dialog reads (`stl`,
//! `obj`, `step`, `stp`) queues `CommandKind::ImportFile` - the Model
//! dialog's own import path, so the worker, the update-or-spawn decision
//! and the one undoable command are shared, not copied. Several files, an
//! unsupported extension, a folder or a path that is no file (the
//! browser hands over a bare name) is refused by name on the status line and
//! leaves the project unchanged. The hint and the drop are view state;
//! the document only changes through the import command.
//! Provenance: our own code over egui's public `RawInput` drop fields; no
//! HSMWorks material.

use std::path::{Path, PathBuf};

use egui::{Align2, Color32, FontId, Rect, Stroke, Ui};

use crate::state::{CommandKind, State};

/// The hint painted over the viewport while a file hovers (also what the
/// structural test reads).
pub(super) const HINT: &str = "Drop to load model";

/// The extensions the Model dialog's filter offers, lower case.
const MESH_EXTENSIONS: &[&str] = &["stl", "obj", "step", "stp"];

fn label(path: &Path) -> String {
    path.file_name().map_or_else(|| path.display().to_string(), |n| n.to_string_lossy().into_owned())
}

/// The one file to import, or the refusal that names why not.
pub(super) fn classify(paths: &[PathBuf]) -> Result<PathBuf, String> {
    let [path] = paths else {
        return match paths.len() {
            0 => Err(String::from("drop refused: no file was dropped")),
            n => Err(format!("drop refused: {n} files were dropped; drop one model file at a time")),
        };
    };
    let ext = path.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
    if !MESH_EXTENSIONS.contains(&ext.as_str()) {
        return Err(format!(
            "drop refused: {} is not a model file (read: .stl, .obj, .step, .stp)",
            label(path)
        ));
    }
    if !path.is_file() {
        return Err(format!("drop refused: {} is not a readable file", label(path)));
    }
    Ok(path.clone())
}

/// Once a frame: take a finished drop (queue the import or refuse it).
pub(super) fn take_drop(ui: &Ui, state: &mut State) {
    let files: Vec<PathBuf> = ui.ctx().input(|i| i.raw.dropped_files.iter().map(|f| f.path().to_path_buf()).collect());
    if files.is_empty() {
        return;
    }
    match classify(&files) {
        Ok(path) => state.pending.push_back(CommandKind::ImportFile(path)),
        Err(msg) => state.refuse_drop(msg),
    }
}

/// The hint over the viewport's rect while a file hovers the window.
pub(super) fn paint_hint(ui: &Ui, rect: Rect) {
    if !ui.ctx().input(|i| !i.raw.hovered_files.is_empty()) {
        return;
    }
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 0.0, Color32::from_black_alpha(140));
    painter.rect_stroke(rect.shrink(6.0), 6.0, Stroke::new(2.0, Color32::WHITE), egui::StrokeKind::Inside);
    painter.text(rect.center(), Align2::CENTER_CENTER, HINT, FontId::proportional(22.0), Color32::WHITE);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(path: &str) -> PathBuf {
        PathBuf::from(path)
    }

    #[test]
    fn refusals_name_the_file_and_the_reason() {
        assert!(classify(&[]).is_err());
        assert!(classify(&[d("a.stl"), d("b.stl")]).expect_err("two").contains("2 files"));
        assert!(classify(&[d("notes.txt")]).expect_err("ext").contains("notes.txt is not a model file"));
        assert!(classify(&[d("missing.STEP")]).expect_err("gone").contains("missing.STEP is not a readable file"));
    }
}
