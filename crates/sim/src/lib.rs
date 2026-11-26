//! camcraft-sim - the sim/verify seam of the M4.1 crate split (R4,
//! docs/M4.1-CRATE-SPLIT.md). Everything here consumes DATA: facets,
//! stock grids, tool records, paths - never entities. The
//! document-facing halves (`stock_sim::stock_grid`, the app's
//! `stock_sim`/`fixture` modules that walk setups) stayed in app and
//! re-export this crate whole: the model/setup knot the design note
//! feared was never cut - it dissolved, because the engine already
//! spoke in tuples and only the world half knew it.
//!
//! Provenance: dexel stock simulation and swept-envelope verification
//! are general CAM practice, specced from machine-tool handbooks and
//! published verification literature; every algorithm here is ours.
//! No flecs, no schema.

// #1442: lathe chucks generated from their figures.
pub mod chuck;
pub mod fixture;
// #339: a fixture's closed mesh as the narrow phase of the steel laws.
pub mod fixture_mesh;
// #2584: a generated machine model (blocks and cylinders), posed by axis values.
pub mod machine_model;
// #2181: the playback keyframes and the calculation before playback.
pub mod play_keys;
pub mod play_precalc;
pub mod play_precalc_mailbox;
pub mod sim;
// #2608: the operation steps of the simulation timeline.
mod sim_steps;
pub mod solid_rest;
pub mod stock_sim;
pub mod verify;

pub use camcraft_cl::cl;
pub use camcraft_geometry::{geo2d, geometry};
pub use camcraft_geometry::geometry::{stock, units};
pub use camcraft_tools::tools;

/// Parametric fixture solids and profile interchange.
pub mod fixture_profile;
