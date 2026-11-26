//! A `.dxf` dropped on the window (#1863): the drawing is placed as a
//! profile on the active setup's stock through the Stock page's own
//! `ImportDxf` path (the units prompt included), not loaded like a model.
//! With no stock the drop is refused by name and tells the user to make a
//! stock first; nothing changes in the document.
//!
//! Provenance: our own code over egui's public dropped-file fields and
//! this repo's #1862 commands; no vendor material consulted.

use std::path::Path;

use camcraft_app::{setup, sketch};

use crate::state::{DxfSource, State};

/// Is this a DXF drawing by its extension (any case)?
pub(super) fn is_dxf(path: &Path) -> bool {
    path.extension().and_then(|e| e.to_str()).is_some_and(|e| e.eq_ignore_ascii_case("dxf"))
}

fn label(path: &Path) -> String {
    path.file_name().map_or_else(|| path.display().to_string(), |n| n.to_string_lossy().into_owned())
}

/// Place the one dropped `.dxf`, or refuse it by name.
pub(super) fn take(state: &mut State, file: &std::sync::Arc<dyn egui::DroppedFile + Send + Sync>) {
    let path = file.path().to_path_buf();
    let name = label(&path);
    let active = setup::active(&state.world);
    if sketch::face_frame(&state.world, active.as_deref()).is_none() {
        state.refuse_drop(format!(
            "drop refused: {name} is a DXF profile - make a stock first (Setup > Stock), then drop it to place it on the stock top"
        ));
        return;
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        if path.is_file() {
            state.import_dxf(DxfSource::Path(path), None);
            return;
        }
        match file.bytes() {
            Ok(data) => state.import_dxf(DxfSource::Bytes { name, data: data.into() }, None),
            Err(why) => state.refuse_drop(format!("drop refused: {name} is not a readable file ({why})")),
        }
    }
    #[cfg(target_arch = "wasm32")]
    state.refuse_drop(format!(
        "drop refused: {name} - placing a dropped DXF profile is not supported in the browser yet; use Setup > Stock > Add DXF profile in the desktop app"
    ));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dxf_by_extension_in_any_case() {
        assert!(is_dxf(Path::new("a/plate.dxf")));
        assert!(is_dxf(Path::new("PLATE.DXF")));
        assert!(!is_dxf(Path::new("plate.stl")));
        assert!(!is_dxf(Path::new("dxf")));
    }
}
