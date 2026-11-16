//! v58 -> v59 (#1056): a project saved before #911 recorded no mesh
//! fidelity, so a model its STEP import kept coarser than promised loads
//! looking exactly like one that met the promise. Every mesh such a file
//! holds is marked "fidelity unknown" (`model_fidelity::UNKNOWN_MM`); the
//! gate then refuses its operations by name until it is re-imported.
//!
//! Provenance: our own persistence contract (the load-note channel of
//! #342, the fidelity flag of #864); no vendor material consulted.
//!
//! ## The law here
//!
//! - Re-measuring on load is not possible: a chord error is measured
//!   against the B-rep, and the file keeps only the facets.
//! - A v58 file cannot tell a pre-#911 save from a later one whose
//!   models all met the promise (#911 did not bump the version), nor a
//!   STEP mesh from an STL one. So every model with facets and every
//!   from-model stock copy without a recorded chord is marked; a
//!   recorded chord (a #911-era save) is known and kept as it is.
//! - The mark is saved (`fidelity_unknown`), so a re-save never turns it
//!   into "fine"; only a fresh import of the model clears it.

use super::{EntityDto, EntityKind, StockDto};

/// Mark every mesh of a file older than v59 whose fidelity the file does
/// not record; one load note per marked entity.
pub(super) fn settle(entities: &mut [EntityDto], from: u32) -> Vec<String> {
    if from >= 59 {
        return Vec::new();
    }
    let mut notes = Vec::new();
    for dto in entities.iter_mut() {
        let mesh = match dto.kind.unwrap_or(EntityKind::Model) {
            EntityKind::Model => dto.facets.is_some(),
            EntityKind::Stock => matches!(dto.stock, Some(StockDto::FromModel { .. })),
            _ => false,
        };
        if mesh && dto.coarse_mm.is_none() && !dto.fidelity_unknown {
            dto.fidelity_unknown = true;
            notes.push(format!("'{}': {}", dto.name, crate::model_fidelity::UNKNOWN_NOTE));
        }
    }
    notes
}
