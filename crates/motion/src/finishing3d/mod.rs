//! finishing3d.rs - 3D finishing: the waterline (Z-level ball path)
//! and the parallel (Z-following raster) strategies.
//!
//! Provenance: Z-level contouring with a ball-nose cutter is textbook
//! CAM (RS-274-level vocabulary; the rolling-ball/tangent-locus model
//! is published literature). The parallel raster's cusp law - a
//! Z-following stepover of `√(8·r·tol)·cos α` between ball paths
//! offset `s·sec α` apart, the Huang-Oliver scallop formula specialised
//! to a tilted plane - and the point-to-triangle distance test are
//! published geometry (Ericson, Real-Time Collision Detection, for the
//! region test). The construction below - forbidden centers as a
//! facetwise support-function scan, the tangent-support center law
//! `c = q + r·n̂`, and the adaptive chord march - is our own derivation,
//! written from geometry, not from any vendor's code, output forensics,
//! or documentation text.
//!
//! ## The laws here
//!
//! - **Forbidden centers.** The finished surface is the model itself,
//!   so a ball center may sit exactly where the ball touches the solid
//!   and nowhere it cuts in. The path at a center plane `cz` is
//!   `boundary(S ⊕ ball ∩ plane)`: the union of per-facet "where may
//!   the center be to touch this facet" sets. Walking that boundary
//!   with the ball is waterline contouring.
//! - **Only outer loops are cut paths.** A hole of the forbidden
//!   region is a set of centers farther than `r` from every facet; its
//!   boundary grazes the surface from the side buried inside the
//!   solid's own material - a center no tool can occupy (the cone's
//!   inner wavefront at `R = (base − cz) − r√2` was a 8.58 mm gouge
//!   when walked). The facet-shell fusion is a material union
//!   (`Region::union_all`, not sketch parity), so genuine islands of
//!   the model emerge as separate CCW components and CW loops are
//!   exactly the unreachable wavefronts: skip them. Interior-feature
//!   waterlines (tool inside a slot or bore) need a reachability test
//!   on hole interiors and are deferred - logged in RECOMMENDED.md.
//! - **Facet contributions are exactly convex.** A triangle plus a
//!   ball is convex, and its section at height `cz` is the convex hull
//!   of the vertex-ball circles: a per-direction support scan
//!   suffices. For direction `u` the support is `max over facet points
//!   q` of `q_xy·u + ρ(q_z)`, `ρ = √(r² − (cz − q_z)²)`; that
//!   function is concave on the triangle, so its maximum is at a
//!   vertex, or on an edge at the one point where
//!   `w·E_z = −(E_xy·u)·ρ` - and that stationary point always lies
//!   inside the feasible band `|w| ≤ r`, so when it falls outside the
//!   edge segment the maximum is at that end's vertex, already a
//!   candidate. (A face-interior stationary point would need `u` to
//!   track the facet normal's XY direction, and the contact line's
//!   edge endpoints carry the same support value - the edge and
//!   vertex candidates cover the face exactly.) Six candidates per
//!   facet per direction, no sampling.
//! - **Stepdown from the scallop, not from habit.** Successive rings
//!   leave a cusp on a wall tilted `α` from vertical of about
//!   `Δz² / (8 r cos²α)`, so `Δz = 2√(r·tol)` holds the cusp inside
//!   `tol` for every surface at `|α| ≤ 45°`. Steeper flanks and flat
//!   shelves are the scallop and parallel strategies' jurisdiction
//!   (M2.2b/M2.2c), and the envelope verifier (`verify.rs`,
//!   `Mode::Envelope`) measures - never assumes - what this stepdown
//!   leaves everywhere else.
//! - **Flat shelves are never waterline rest.** A horizontal facet
//!   only enters the forbidden region through radius-0 silhouette
//!   tangents; its interior is reachable by no ring. That is not a
//!   hole in this law, it is the parallel strategy's job description.
//! - **The entry is the tangent point.** A vertical plunge at a
//!   tangent-ring entry never cuts above the ring's level on any
//!   locally convex surface (distance from the lifted center to the
//!   tangent point grows monotonically); overhangs violate the
//!   premise, and the envelope measurement reports that, loudly, as
//!   a gouge. No lead-in arcs for M2.2a - the ring is smooth at its
//!   own entry by construction. M8.2 B's `entry_ramp` is the opt-in
//!   that says when the plunge, though innocent, is inhospitable: on
//!   flanks past the 45° split the ball meets the wall at full
//!   contact, so the entry may instead slide in along its contact
//!   normal - one radius out, down a certified column, in along a
//!   certified diagonal, certification spending the SAME currency as
//!   this law's own probe (centers never bite past `tol`). Where the
//!   normal cannot be read, lies under the horizon, or bites, the
//!   plunge stands and the path carries a note saying so.
//! - **Level changes climb to `clearance_z`; rings within one level
//!   hop at `retract_z` clamped to clearance** - the shared ladder
//!   law of every depth-stepping operation in this codebase.

//!
//! The machinery lives in the per-domain files below; the M5.0.1 split is
//! structure only - behaviour is byte-identical to the flat file it replaces.

mod center_cut;
mod crease;
mod deburr;
mod drape;
mod edge;
mod fill;
mod flat;
mod geodesic;
mod ladder;
mod morph;
mod pencil;
mod flow;
mod march;
mod params;
mod polar;
mod raster;
mod rim;
mod rings;
mod scan;
mod shadow;
mod support;

pub use params::{
    scallop_stepdown, scallop_stepover, Finish3DParams, FinishPattern,
};
pub use center_cut::guard as center_cut_guard;
pub use center_cut::guard_as as center_cut_guard_as;
pub use center_cut::guard_shaped as center_cut_guard_shaped;
pub use deburr::{deburr, DeburrParams};
pub use fill::{fill_openings, Filled};
pub use flat::flat;
pub use geodesic::geodesic;
pub use pencil::{corner, pencil};
pub use flow::flow;
// The 5-axis stamp's contact question (M5.1 C): the same Voronoi
// closest-point law the raster clearance uses, crate-visible.
pub(crate) use support::closest_pt_tri;
pub use polar::{morphed_spiral, radial, spiral};
pub use raster::{blend, parallel, swarf, swarf_multi};
pub use rings::waterline;
pub use morph::morph;
pub use shadow::{contour, project};
