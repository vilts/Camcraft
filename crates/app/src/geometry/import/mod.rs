//! Mesh file importers (STL, OBJ, STEP). Public API: `load_model` / `parse_*`.
//!
//! The facet/vertex caps below are enforced before anything large is
//! allocated. STL (M5.3 E) streams from the file - the size law runs off
//! `metadata` before the facet buffer exists; OBJ and STEP stay buffered,
//! their parsers wanting the whole text in hand.

pub mod obj;
pub mod step;
pub mod step_budget;
pub mod step_csys;
mod step_diag;
pub mod step_depth;
mod step_refine;
mod step_units;
pub mod stl;

use std::path::{Path, PathBuf};

use crate::geometry::Mesh;

/// Hard cap on facets per model. 30M facets is the M5.3 E door: the
/// roadmap's 10M+ models walk through with room for the mesh to ride the
/// undo history as an Arc (one allocation, not one per revision) - at
/// 72-byte facets, 30M is a 2.16 GiB peak, under the 128 GiB machine's
/// comfort and under the file cap below (a 30M-facet binary STL is
/// ~1.5 GiB of disk).
pub const MAX_FACETS: usize = 30_000_000;
/// Hard cap on OBJ vertex definitions.
pub const MAX_VERTICES: usize = 5_000_000;
/// Cap on the size of any file we are willing to read into memory
/// (buffered formats) or declare from disk (the STL streaming law's
/// pre-allocation check).
pub const MAX_FILE_BYTES: u64 = 4 << 30;

#[derive(Debug, thiserror::Error)]
pub enum ImportError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("file is empty")]
    Empty,
    #[error("model exceeds the import limit")]
    TooLarge,
    #[error("corrupt STL: {0}")]
    Stl(String),
    #[error("corrupt OBJ: {0}")]
    Obj(String),
    #[error("corrupt STEP: {0}")]
    Step(String),
    #[error("imported mesh contains non-finite coordinates")]
    NonFinite,
    #[error("unsupported file type: {0}")]
    Unsupported(String),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Format {
    Stl,
    Obj,
    Step,
}

/// Load a model from disk, dispatching on extension. STL (M5.3 E)
/// streams: `metadata` feeds the size law, so a 10M-facet file costs its
/// facet buffer plus one window, never the whole disk image twice over.
pub fn load_model(path: &Path) -> Result<Mesh, ImportError> {
    load_model_graded(path).map(|m| m.mesh)
}

pub fn parse_model(data: &[u8], format: Format) -> Result<Mesh, ImportError> {
    parse_model_graded(data, format).map(|m| m.mesh)
}

/// An imported mesh and how faithfully it stands for the surfaces it was
/// made from (#864).
#[derive(Debug)]
pub struct Graded {
    pub mesh: Mesh,
    /// `Some(mm)` when the mesh approximates its curved surfaces more
    /// coarsely than the importer promises ([`step::CHORD_CAP`]): the
    /// chord error it was actually made at. `None` for a mesh that met
    /// the promise, and for STL and OBJ - those files ARE the mesh, with
    /// no surface behind them to measure against.
    pub coarse_mm: Option<f64>,
}

/// [`load_model`], with the fidelity the import achieved.
pub fn load_model_graded(path: &Path) -> Result<Graded, ImportError> {
    load_model_graded_within(path, MAX_FACETS)
}

/// [`load_model_graded`] with a STEP model tessellated inside `budget`
/// facets (#1054; STL and OBJ are the mesh itself and ignore it).
pub fn load_model_graded_within(path: &Path, budget: usize) -> Result<Graded, ImportError> {
    let format = format_from_path(path)?;
    let meta = std::fs::metadata(path)?;
    if meta.len() > MAX_FILE_BYTES {
        return Err(ImportError::TooLarge);
    }
    if format == Format::Stl {
        return Ok(Graded { mesh: gate(stl::parse_file(path)?)?, coarse_mm: None });
    }
    let data = std::fs::read(path)?;
    parse_model_graded_within(&data, format, budget)
}

/// [`parse_model`], with the fidelity the import achieved.
pub fn parse_model_graded(data: &[u8], format: Format) -> Result<Graded, ImportError> {
    parse_model_graded_within(data, format, MAX_FACETS)
}

fn parse_model_graded_within(data: &[u8], format: Format, budget: usize) -> Result<Graded, ImportError> {
    let (mesh, coarse_mm) = match format {
        Format::Stl => (stl::parse(data)?, None),
        Format::Obj => (obj::parse(data)?, None),
        Format::Step => {
            let (mesh, chord) = step::parse_with_chord_budget(data, budget)?;
            (mesh, step::coarse(chord))
        }
    };
    Ok(Graded { mesh: gate(mesh)?, coarse_mm })
}

/// Belt-and-gate: each importer checks finiteness at the parse site, and
/// nothing enters the world without passing here either.
fn gate(mesh: Mesh) -> Result<Mesh, ImportError> {
    if !mesh.all_finite() {
        return Err(ImportError::NonFinite);
    }
    Ok(mesh)
}

pub fn format_from_path(path: &Path) -> Result<Format, ImportError> {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .map(str::to_ascii_lowercase)
        .unwrap_or_default();
    match ext.as_str() {
        "stl" => Ok(Format::Stl),
        "obj" => Ok(Format::Obj),
        "step" | "stp" => Ok(Format::Step),
        _ => Err(ImportError::Unsupported(
            path.file_name()
                .map(PathBuf::from)
                .map(|p| p.display().to_string())
                .unwrap_or_else(|| "<unknown>".to_string()),
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unsupported_extension_is_an_error_not_a_panic() {
        let err = load_model(Path::new("thing.iges")).err();
        assert!(matches!(err, Some(ImportError::Unsupported(_))));
    }
}
