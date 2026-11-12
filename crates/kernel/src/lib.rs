//! camcraft-kernel - the B-rep kernel seam of M12.1, stages 1-3 (evaluation +
//! agreement bench). A thin [`Kernel`] trait carries only the operations both
//! candidate engines can be asked for: primitive construction (box, Z
//! cylinder), the three booleans (subtract, union, intersect), whole-solid
//! offset, constant-radius edge blends (round, chamfer), healing, and a [`Measure`]
//! (volume + bounding box) the bench compares. Nothing else in the workspace
//! depends on this crate yet; truck stays the live engine.
//!
//! Why `measure` and not `tessellate`: solidcore 1.0.1 does not port SMLib's
//! tessellation (its README lists it among the unported SM_API entry points),
//! so a triangle-mesh method would have no solidcore impl. The port measures
//! exactly through its mass-property integrator; the truck impl measures its
//! tessellation. The bench compares those numbers.
//!
//! Stage 4 (#916) adds a STEP importer on the solidcore side
//! ([`solidcore_step`]), bench only: the app still imports through truck.
//! #940 adds the machining mesh of such a solid ([`machining_mesh`]):
//! solidcore-tess at a chord in mm, every shortfall a named refusal, the
//! deviation from the exact surfaces measured ([`mesh_deviation`]).
//!
//! Provenance: our own seam design; solidcore (tangency-labs, Apache-2.0
//! port of NVIDIA's SMLib) and its STEP reader are linked at one pinned
//! commit of the owner's fork (Cargo.toml), truck (ricosjp, Apache-2.0) as
//! published crates; the mesh volume is the divergence theorem (any
//! computational-geometry text). Notices in assets/ui/LICENSES.md.

#![forbid(unsafe_code)]

mod error;
pub mod machining_mesh;
mod measure;
pub mod mesh_deviation;
mod solidcore_blend;
pub mod solidcore_impl;
pub mod solidcore_import;
pub mod solidcore_step;
#[cfg(feature = "truck")]
pub mod truck_impl;

pub use error::KernelError;
pub use measure::{Measure, mesh_signed_volume};

/// A point or extent in model units, `[x, y, z]`.
pub type P3 = [f64; 3];

/// The kernel seam. `Solid` is the engine's own solid type; the seam never
/// looks inside it. Every method validates its numeric input before the
/// engine sees it (Prime Directive: degenerate input is a clean `Err`).
pub trait Kernel {
    /// The engine's solid representation.
    type Solid;

    /// Short engine name for diagnostics (`"solidcore"`, `"truck"`).
    fn name(&self) -> &'static str;

    /// Axis-aligned box with its minimum corner at `origin`.
    fn make_box(&mut self, origin: P3, size: P3) -> Result<Self::Solid, KernelError>;

    /// Cylinder along +Z, base circle centered at `base`.
    fn make_cylinder(
        &mut self,
        base: P3,
        radius: f64,
        height: f64,
    ) -> Result<Self::Solid, KernelError>;

    /// `a` minus `b`. An empty result is an `Err`, not an empty solid.
    fn boolean_subtract(
        &mut self,
        a: Self::Solid,
        b: Self::Solid,
    ) -> Result<Self::Solid, KernelError>;

    /// `a` united with `b`.
    fn boolean_union(&mut self, a: Self::Solid, b: Self::Solid) -> Result<Self::Solid, KernelError>;

    /// The common part of `a` and `b`. An empty result is an `Err`.
    fn boolean_intersect(
        &mut self,
        a: Self::Solid,
        b: Self::Solid,
    ) -> Result<Self::Solid, KernelError>;

    /// Whole-solid offset by `distance` (negative = inset), convex edges
    /// rounded. Engines without an offset return [`KernelError::Unsupported`].
    fn offset(&mut self, s: &mut Self::Solid, distance: f64) -> Result<Self::Solid, KernelError>;

    /// Constant-radius blend of the edges nearest each of `picks` (every
    /// pick must lie on an edge, within [`PICK_TOL`]; two picks on one edge
    /// are refused). `Round` is a circular section of `radius`; `Chamfer` a
    /// flat one with setback `radius` on both faces. Engines without edge
    /// blends return [`KernelError::Unsupported`].
    fn blend_edges(
        &mut self,
        s: Self::Solid,
        picks: &[P3],
        radius: f64,
        kind: Blend,
    ) -> Result<Self::Solid, KernelError>;

    /// Repairs gaps between a solid's vertices, edges and faces (the fix-up
    /// an imported model needs) and hands it back only if it is a closed
    /// solid afterwards. Engines without healing return
    /// [`KernelError::Unsupported`].
    fn heal(&mut self, s: Self::Solid) -> Result<Self::Solid, KernelError>;

    /// Volume and bounding box of a closed solid.
    fn measure(&mut self, s: &mut Self::Solid) -> Result<Measure, KernelError>;
}

/// The edge-blend cross-section.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Blend {
    /// Circular section (a fillet on a concave edge, a round on a convex one).
    Round,
    /// Flat section, equal setback on both faces.
    Chamfer,
}

/// How far a blend pick may lie from the edge it names (model units).
pub const PICK_TOL: f64 = 1e-3;

/// Rejects non-finite coordinates (shared input guard of both impls).
pub(crate) fn finite_point(op: &'static str, p: P3) -> Result<(), KernelError> {
    if p.iter().all(|c| c.is_finite()) {
        Ok(())
    } else {
        Err(KernelError::InvalidInput {
            op,
            detail: format!("non-finite coordinate {p:?}"),
        })
    }
}

/// Rejects a length that is not finite and strictly positive.
pub(crate) fn positive_len(op: &'static str, what: &str, v: f64) -> Result<(), KernelError> {
    if v.is_finite() && v > 0.0 {
        Ok(())
    } else {
        Err(KernelError::InvalidInput {
            op,
            detail: format!("{what} must be finite and > 0, got {v}"),
        })
    }
}

/// Rejects an offset distance that is not finite or is zero.
pub(crate) fn offset_distance(v: f64) -> Result<(), KernelError> {
    if v.is_finite() && v != 0.0 {
        Ok(())
    } else {
        Err(KernelError::InvalidInput {
            op: "offset",
            detail: format!("distance must be finite and non-zero, got {v}"),
        })
    }
}
