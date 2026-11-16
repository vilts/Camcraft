//! Tool-body files (#361): the one door a holder or assembly file (STEP,
//! STL, OBJ) passes on its way to becoming a tool's body, whichever
//! command, pane or MCP tool asked. It reads through the model importer
//! and refuses, by name, a STEP that could only be meshed coarser than
//! the importer promises (#864's reduced fidelity). Specced from the
//! chord-and-sagitta fact `model_fidelity` states: a facet of a convex
//! wall stands inside the true surface, so a coarse holder reads smaller
//! than the holder is. Our own design; no vendor file or source consulted.
//!
//! The law here: **a body is judged from its facets, so its facets must
//! keep the importer's promise.** The collision laws read a body's
//! envelope with no allowance for a coarser mesh; until they grow one,
//! the only verdict that cannot be too generous is a refusal.

use std::path::Path;

use crate::geometry::import::{self, step, Graded};
use crate::geometry::Triangle;

/// A body file as read: its display name and its model-frame facets.
#[derive(Clone, Debug, PartialEq)]
pub struct BodyFile {
    pub source: String,
    pub facets: Vec<Triangle>,
}

/// The file name a body is called by (the whole path when it has none).
#[must_use]
pub fn source_name(path: &Path) -> String {
    path.file_name().map_or_else(|| path.display().to_string(), |f| f.to_string_lossy().into_owned())
}

/// Why a body meshed at `chord_mm` is not taken.
#[must_use]
pub fn too_coarse(source: &str, chord_mm: f64) -> String {
    format!(
        "{source} is too detailed to import at full fidelity (its curved faces would be off by up to {chord_mm:.3} mm, the importer promises {:.3} mm) - a tool body judged from that mesh would read smaller than the holder is; simplify the model (drop threads, lettering, small fillets) and import it again",
        step::CHORD_CAP
    )
}

/// Take an imported mesh as a tool body, or refuse it by name: reduced
/// fidelity, or no facets at all.
pub fn accept(path: &Path, graded: Graded) -> Result<BodyFile, String> {
    let source = source_name(path);
    if let Some(chord) = graded.coarse_mm {
        return Err(too_coarse(&source, chord));
    }
    if graded.mesh.facets.is_empty() {
        return Err(format!("{source} holds no facets"));
    }
    Ok(BodyFile { source, facets: graded.mesh.facets.to_vec() })
}

/// Read `path` through the model importer and [`accept`] it.
pub fn read(path: &Path) -> Result<BodyFile, String> {
    let graded = import::load_model_graded(path).map_err(|e| e.to_string())?;
    accept(path, graded)
}
