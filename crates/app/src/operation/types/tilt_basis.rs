//! The tilt basis step (#2599): an edit that gives a tilt new angles
//! takes the current frame basis, so the plane X of a new index is the
//! same in a loaded old project as in a new one.
//!
//! Provenance: our own frame-basis contract (#2133 / PR #2157,
//! `crate::cl::TiltedFrame`); no vendor material.

use super::params::{OpParams, Tilt};

/// The basis version that new tilts use (`crate::cl::TiltedFrame::new`).
pub const CURRENT_TILT_BASIS: u8 = 1;

impl OpParams {
    /// The tilt field of the planar kinds that carry one, as a slot the
    /// caller can write; None for the kinds that carry no 3+2 index
    /// (the same set that [`OpParams::tilt`] answers None for).
    pub fn tilt_slot(&mut self) -> Option<&mut Option<Tilt>> {
        match self {
            OpParams::Face { tilt, .. }
            | OpParams::Contour { tilt, .. }
            | OpParams::Pocket { tilt, .. }
            | OpParams::Adaptive2D { tilt, .. }
            | OpParams::Offset2D { tilt, .. }
            | OpParams::Engrave { tilt, .. }
            | OpParams::Chamfer2D { tilt, .. }
            | OpParams::Circular { tilt, .. }
            | OpParams::Slot { tilt, .. }
            | OpParams::Trace { tilt, .. }
            | OpParams::VCarve { tilt, .. } => Some(tilt),
            _ => None,
        }
    }

    /// The edit door's basis law (#2599). A legacy basis (version 0,
    /// a tilt from a file written before #2133) keeps its saved plane
    /// coordinates only while its angles stay as saved. When an edit
    /// gives the tilt new angles, the user sets a new index, and it
    /// takes [`CURRENT_TILT_BASIS`]: else an A0/B180 index set today on
    /// an old operation turns its plane XY 90 degrees from the same
    /// index on a new operation. An origin-only edit keeps the basis.
    #[must_use]
    pub fn with_stepped_tilt_basis(mut self, prev: &OpParams) -> OpParams {
        let before = prev.tilt();
        if let Some(Some(t)) = self.tilt_slot() {
            let same_angles = before.is_some_and(|b| b.a_deg == t.a_deg && b.b_deg == t.b_deg);
            if t.basis_version < CURRENT_TILT_BASIS && !same_angles {
                t.basis_version = CURRENT_TILT_BASIS;
            }
        }
        self
    }
}

#[cfg(test)]
mod tests {
    use crate::operation::{KIND_COUNT, OpType, seed_params};
    use crate::project::{params_from_json, params_to_json};

    use super::*;

    fn legacy(a_deg: f64, b_deg: f64) -> Option<Tilt> {
        Some(Tilt { basis_version: 0, a_deg, b_deg, origin: [0.0; 3] })
    }

    /// The slot set is the set of kinds whose file shape has a tilt:
    /// a kind that gains a tilt field and is missing here fails.
    #[test]
    fn the_tilt_slot_covers_each_kind_whose_file_shape_has_a_tilt() {
        for kind in 0..KIND_COUNT {
            let mut seed = seed_params(kind);
            let mut file = params_to_json(&seed);
            if let Some(fields) = file.as_object_mut().and_then(|m| m.values_mut().next()) {
                fields["tilt"] = serde_json::json!({ "a_deg": 10.0, "b_deg": 0.0, "origin": [0.0, 0.0, 0.0] });
            }
            let carries = params_from_json(&file).is_ok_and(|p| p.tilt().is_some());
            assert_eq!(seed.tilt_slot().is_some(), carries, "{:?}", seed.kind());
        }
    }

    #[test]
    fn new_angles_step_a_legacy_basis_and_kept_angles_do_not() {
        let mut prev = seed_params(OpType::Slot.ordinal());
        assert!(prev.tilt_slot().is_some(), "a slot carries a tilt");
        if let Some(slot) = prev.tilt_slot() {
            *slot = legacy(90.0, 0.0);
        }
        let with = |t: Option<Tilt>| {
            let mut p = prev.clone();
            if let Some(slot) = p.tilt_slot() {
                *slot = t;
            }
            p.with_stepped_tilt_basis(&prev).tilt()
        };
        assert_eq!(with(legacy(0.0, 180.0)).map(|t| t.basis_version), Some(1));
        assert_eq!(with(legacy(90.0, 0.0)).map(|t| t.basis_version), Some(0));
        let moved = Tilt { basis_version: 0, a_deg: 90.0, b_deg: 0.0, origin: [1.0, 2.0, 3.0] };
        assert_eq!(with(Some(moved)).map(|t| t.basis_version), Some(0));
        assert_eq!(with(None), None);
    }
}
