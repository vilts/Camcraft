//! camcraft-geometry - the first crate out of the M4.1 ladder (R1,
//! docs/M4.1-CRATE-SPLIT.md). The module tree mirrors the old
//! `camcraft_app::{geometry, geo2d}` paths on purpose: nothing inside
//! these files had to be re-pathed, and the app re-exports the whole
//! tree so UI and integration paths compile untouched.
//!
//! What is NOT here, and why: the STL/OBJ/STEP importers (truck-bound,
//! the design note keeps that seam in app) and the flecs singleton glue
//! for units (crate::units in app - the enum itself moved to
//! geometry::units at R0).
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
