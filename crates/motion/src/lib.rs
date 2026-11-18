//! camcraft-motion - the generator seam of the M4.1 crate split (R4,
//! docs/M4.1-CRATE-SPLIT.md). Model facets and tool records in, CL
//! out; the one legal direction to the sim is the one used here:
//! machining3d asks the dexel engine for rest material
//! (motion -> sim, one-way as designed). `rest.rs` stayed in app -
//! it is remachining ORCHESTRATION (who counts as prior, by setup
//! traversal), not a generator; the design table's "rest in motion"
//! entry was wrong and R4 records it. The islands law itself sank to
//! `geometry::slice`, so generator and verifier read one module, not
//! each other: the generators `material_overhead` (section plus the
//! shadow of what hangs above), the verifier `material_above` (the
//! plain section at a move's plane).
//!
//! Provenance: offset/contour pocketing (clipper-backed island
//! logic), zigzag and waterline slicing, drill-cycle families
//! (RS-274 G73/G81/G83/G84/G85/G86/G89 semantics), scallop-height
//! finishing - all from public machining literature and G-code
//! standards; no vendor strategy was inspected. No flecs, no world.

// Tests only (issue #1051): a unit suite reads the integration suites'
// fixtures (`tests/rotary_adaptive_fixture`), which name this crate.
#[cfg(test)]
extern crate self as camcraft_motion;

pub mod stock_top_frame;
pub mod approach;
pub mod drilling;
pub mod break_through;
pub mod entry;
pub mod entry_arc;
pub mod links;
// #1558: a zig-zag ramp flown as the ring's own helical arcs.
pub mod ring_zig;
#[cfg(test)]
mod ring_zig_tests;
pub mod load;
pub mod finishing3d;
pub mod machining2d;
pub mod machining3d;
pub mod plunge_law;
pub mod plasma;
pub mod wire_edm;
pub mod probing;
// #541: sinker EDM burn to depth.
pub mod sinker;
// #543: sinker rough and finish with undersize.
pub mod sinker_undersize;
// #547: a cavity read from picked faces, and the electrode it calls for.
pub mod sinker_cavity;
pub mod sinker_electrode;
// #1152: the body electrode of a non-rectangular cavity.
mod sinker_electrode_body;
#[cfg(test)]
mod sinker_electrode_body_tests;
#[cfg(test)]
mod sinker_electrode_tests;
#[cfg(test)]
mod sinker_electrode_fuzz;
// #546: sinker electrode wear compensation in Z.
pub mod sinker_wear;
// #544: sinker EDM orbit patterns as explicit moves.
pub mod sinker_orbit;
// #545: sinker EDM side burning by a lateral electrode move.
pub mod sinker_side;
pub mod quality;
pub mod multiaxis;
pub mod turning;

pub use camcraft_cl::cl;
pub use camcraft_geometry::{geo2d, geometry};
pub use camcraft_geometry::geometry::{stock, units};
pub use camcraft_sim::{fixture, stock_sim, verify};
pub use camcraft_tools::tools;
