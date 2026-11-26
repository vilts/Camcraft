//! The STEP importers over [`read_step`] (#916, #917): a file whole, or
//! refused by name. [`import_step`] hands out solids only;
//! [`import_step_model`] also hands out surface models, to be meshed with
//! open sides.
//! Provenance: our own seam over solidcore-step's public API (see
//! `solidcore_step`); no HSMWorks material consulted.

use solidcore::kernel::CancelToken;

use crate::KernelError;
use crate::solidcore_impl::SolidcoreKernel;
use crate::solidcore_step::{NAME, SHEET, StepRead, StepRefusal, StepSolid, read_step, read_step_assuming};
use crate::step_edge_gap::EdgeGap;

/// A surface model of a STEP file (`SHELL_BASED_SURFACE_MODEL` of open
/// shells), in millimetres: a record to mesh with open sides
/// ([`crate::machining_mesh_open::sheet_mesh`]), never a solid.
#[derive(Debug)]
pub struct StepSheet {
    /// The file's entity id of the item (`#id`).
    pub item: u64,
    /// The item's own name, else its product's (may be empty).
    pub name: String,
    /// The reader's B-rep record (mm).
    pub record: solidcore_record::brep::Brep,
    /// What the reader repaired or noted on it.
    pub notes: Vec<String>,
}

/// A STEP file imported whole: every solid item of the file is a solid.
#[derive(Debug)]
pub struct StepImport {
    /// The file's solids, in millimetres.
    pub solids: Vec<StepSolid>,
    /// See [`StepRead::length_units_mm`].
    pub length_units_mm: Vec<f64>,
    /// File-level diagnostics.
    pub notes: Vec<String>,
    /// See [`StepRead::edge_gap`].
    pub edge_gap: EdgeGap,
}

/// A STEP model imported whole: its solids and its surface models.
#[derive(Debug)]
pub struct StepModel {
    /// The file's solids, in millimetres.
    pub solids: Vec<StepSolid>,
    /// The file's surface models, in millimetres.
    pub sheets: Vec<StepSheet>,
    /// See [`StepRead::length_units_mm`].
    pub length_units_mm: Vec<f64>,
    /// See [`StepRead::assumed_mm`].
    pub assumed_mm: Option<f64>,
    /// File-level diagnostics.
    pub notes: Vec<String>,
    /// See [`StepRead::edge_gap`].
    pub edge_gap: EdgeGap,
}

fn refused(op: &'static str, read: &StepRead, kept: usize) -> Option<KernelError> {
    let first = read.refused.first()?;
    Some(KernelError::InvalidResult {
        kernel: NAME,
        op,
        detail: format!(
            "{} of {} solid item(s) refused; first #{} at {}: {}",
            read.refused.len(),
            read.refused.len().saturating_add(kept),
            first.item,
            first.stage,
            first.reason
        ),
    })
}

/// The importer: [`read_step`], and a model only when EVERY solid item of
/// the file came through. A file that reads only in part is refused by
/// name, never handed out with a body missing - a toolpath planned against
/// a part with a solid left out cuts through it. A surface model is
/// refused as no solid.
pub fn import_step(
    k: &mut SolidcoreKernel,
    bytes: &[u8],
    stop: &CancelToken,
) -> Result<StepImport, KernelError> {
    let mut read = read_step(k, bytes, stop)?;
    for s in read.sheets.drain(..) {
        read.refused.push(StepRefusal { item: s.item, stage: "read", reason: SHEET.into() });
    }
    read.refused.sort_by_key(|r| r.item);
    if let Some(e) = refused("import_step", &read, read.solids.len()) {
        return Err(e);
    }
    if read.solids.is_empty() {
        return Err(KernelError::InvalidResult {
            kernel: NAME,
            op: "import_step",
            detail: "the file holds no solid".into(),
        });
    }
    Ok(StepImport {
        solids: read.solids,
        length_units_mm: read.length_units_mm,
        notes: read.notes,
        edge_gap: read.edge_gap,
    })
}

/// [`import_step`] that keeps the surface models too: the app's model
/// importer. Every solid and sheet item came through, or the file is
/// refused by name.
pub fn import_step_model(
    k: &mut SolidcoreKernel,
    bytes: &[u8],
    stop: &CancelToken,
) -> Result<StepModel, KernelError> {
    import_step_model_assuming(k, bytes, stop, None)
}

/// [`import_step_model`] with the caller's length unit for a file that
/// states none (#1874, [`read_step_assuming`]).
pub fn import_step_model_assuming(
    k: &mut SolidcoreKernel,
    bytes: &[u8],
    stop: &CancelToken,
    assume_mm: Option<f64>,
) -> Result<StepModel, KernelError> {
    let read = read_step_assuming(k, bytes, stop, assume_mm)?;
    let kept = read.solids.len().saturating_add(read.sheets.len());
    if let Some(e) = refused("import_step", &read, kept) {
        return Err(e);
    }
    if kept == 0 {
        return Err(KernelError::InvalidResult {
            kernel: NAME,
            op: "import_step",
            detail: "the file holds no solid".into(),
        });
    }
    Ok(StepModel {
        solids: read.solids,
        sheets: read.sheets,
        length_units_mm: read.length_units_mm,
        assumed_mm: read.assumed_mm,
        notes: read.notes,
        edge_gap: read.edge_gap,
    })
}
