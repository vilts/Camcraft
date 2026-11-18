//! Drag and drop a model file onto the window (issue #1450). While a file
//! is dragged over the window the viewport wears a plain "Drop to load
//! model" hint; a single drop of a mesh the Model dialog reads (`stl`,
//! `obj`, `step`, `stp`, and a Solidcraft `sc`, #1456) queues `CommandKind::ImportFile` - the Model
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
use crate::state::MODEL_EXTENSIONS as MESH_EXTENSIONS;

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
            "drop refused: {} is not a model file (read: .{})",
            label(path),
            MESH_EXTENSIONS.join(", .")
        ));
    }
    if !path.is_file() {
        return Err(format!("drop refused: {} is not a readable file", label(path)));
    }
    Ok(path.clone())
}

/// Once a frame: take a finished drop (queue the import or refuse it).
/// Disk paths run through `classify` -> `CommandKind::ImportFile`;
/// bare-name drops with bytes (browser drops, or in-memory drops) route
/// through `file_pick`.
pub(super) fn take_drop(ui: &Ui, state: &mut State) {
    let files = ui.ctx().input(|i| i.raw.dropped_files.clone());
    if files.is_empty() {
        return;
    }
    #[cfg(not(target_arch = "wasm32"))]
    if files.len() != 1 || files[0].path().is_file() {
        let paths: Vec<PathBuf> = files.iter().map(|f| f.path().to_path_buf()).collect();
        match classify(&paths) {
            Ok(path) => state.pending.push_back(CommandKind::ImportFile(path)),
            Err(msg) => state.refuse_drop(msg),
        }
        return;
    }
    let [file] = files.as_slice() else {
        state.refuse_drop(format!(
            "drop refused: {} files were dropped; drop one model file at a time",
            files.len()
        ));
        return;
    };
    let path = file.path().to_path_buf();
    let name = label(&path);
    let ext = path.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
    if !MESH_EXTENSIONS.contains(&ext.as_str()) {
        state.refuse_drop(format!(
            "drop refused: {name} is not a model file (read: .{})",
            MESH_EXTENSIONS.join(", .")
        ));
        return;
    }
    #[cfg(not(target_arch = "wasm32"))]
    match file.bytes() {
        Ok(bytes) => match check_drop_bytes(&name, Ok(bytes)) {
            Ok(data) => deliver_model_bytes(ui.ctx(), name, Ok(data)),
            Err(why) => state.refuse_drop(format!("drop refused: {why}")),
        },
        Err(why) => state.refuse_drop(format!("drop refused: {name} is not a readable file ({why})")),
    }
    #[cfg(target_arch = "wasm32")]
    {
        if let Some(wf) = file.web_file()
            && let Some(why) = crate::file_pick::too_big(&name, wf.size())
        {
            state.refuse_drop(format!("drop refused: {why}"));
            return;
        }
        state.status = format!("reading {name}…");
        let file = std::sync::Arc::clone(file);
        let ctx = ui.ctx().clone();
        wasm_bindgen_futures::spawn_local(async move {
            let bytes = check_drop_bytes(&name, file.bytes_async().await);
            if let Err(why) = &bytes {
                diag_warn!("file drop: {name}: {why}");
            }
            deliver_model_bytes(&ctx, name, bytes);
        });
    }
}

fn check_drop_bytes(name: &str, bytes: Result<Vec<u8>, String>) -> Result<Vec<u8>, String> {
    bytes.and_then(|data| match crate::file_pick::too_big(name, data.len() as f64) {
        Some(why) => Err(why),
        None => Ok(data),
    })
}

fn deliver_model_bytes(ctx: &egui::Context, name: String, bytes: Result<Vec<u8>, String>) {
    crate::file_pick::deliver(crate::file_pick::PickedFile {
        purpose: crate::file_pick::PickFor::Model,
        name,
        bytes,
    });
    ctx.request_repaint();
}

/// The hint over the viewport's rect while a file hovers the window.
pub(super) fn paint_hint(ui: &Ui, rect: Rect, pal: &super::theme::Tokens) {
    if !ui.ctx().input(|i| !i.raw.hovered_files.is_empty()) {
        return;
    }
    let painter = ui.painter_at(rect);
    let inset = pal.window_margin.clamp(0.0, (rect.width().min(rect.height()) * 0.4).max(0.0));
    let radius = f32::from(pal.radius_window);
    let stroke_w = (pal.font_label / 5.5).clamp(1.0, 6.0);
    let font_px = (pal.font_label * 2.0).max(12.0);
    painter.rect_filled(rect, 0.0, Color32::from_black_alpha(140));
    painter.rect_stroke(
        rect.shrink(inset),
        radius,
        Stroke::new(stroke_w, Color32::WHITE),
        egui::StrokeKind::Inside,
    );
    painter.text(rect.center(), Align2::CENTER_CENTER, HINT, FontId::proportional(font_px), Color32::WHITE);
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

    #[test]
    fn a_solidcraft_design_drop_queues_the_import() {
        // #1456: `.sc` is a model file; the import refuses a damaged one by name.
        let sample = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../testdata/models/solidcraft_for_beginners.sc");
        assert_eq!(classify(std::slice::from_ref(&sample)), Ok(sample.clone()));
        assert!(classify(&[d("gone.SC")]).expect_err("gone").contains("gone.SC is not a readable file"));
        assert!(classify(&[d("notes.txt")]).expect_err("ext").contains(".sc)"));
    }
}
