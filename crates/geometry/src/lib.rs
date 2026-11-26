//! camcraft-geometry - the first crate out of the M4.1 ladder (R1,
//! docs/M4.1-CRATE-SPLIT.md). The module tree mirrors the old
//! `camcraft_app::{geometry, geo2d}` paths on purpose: nothing inside
//! these files had to be re-pathed, and the app re-exports the whole
//! tree so UI and integration paths compile untouched.
//!
//! What is NOT here, and why: the STL/OBJ importers and the import facade
//! (the design note keeps that seam in app; since #917 the STEP and
//! Solidcraft meshing through the solidcore kernel is here, in `step` and
//! `solidcraft`) and the flecs singleton glue for units (crate::units in
//! app - the enum itself moved to geometry::units at R0).
//!
//! Provenance: our own domain model, specced from public computational-
//! geometry literature, RS-274 unit conventions and general CAD/CAM
//! practice; the 2D offsets/booleans ride clipper2-rust (BSL, notice in
//! assets/ui/LICENSES.md). Law: no ECS, no world, no schema - the
//! crate-boundary proof the R0 map was drawn to enable.

pub mod geometry;
pub mod geo2d;

pub mod boundary_chains;
pub mod dxf;
pub mod dxf_profile;
pub mod nesting;
pub mod profile_derive;
pub mod profile_mesh;
pub mod sketch_chains;
pub mod wire_round_profile;
pub mod die_opening;
pub mod solidcraft;
#[cfg(not(target_arch = "wasm32"))]
mod kernel_import;
pub mod step;
pub mod step_csys;
#[cfg(not(target_arch = "wasm32"))]
mod step_csys_read;
pub mod step_diag;
#[cfg(not(target_arch = "wasm32"))]
pub mod step_units;
pub mod sketch_data;
// #1880: SVG drawings read as the same 2D primitives as DXF ones.
pub mod svg;
// #2679: typed text as single-line engraving strokes.
pub mod stroke_text;
// #3072: typed text in an outline font as closed shapes.
pub mod outline_text;
