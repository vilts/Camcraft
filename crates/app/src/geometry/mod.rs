//! Geometry: the camcraft-geometry crate re-exported whole (M4.1 R1,
//! docs/M4.1-CRATE-SPLIT.md), plus the mesh import facade, which stays in
//! app by design. Solidcraft and STEP meshing (solidcore) live in geometry.
//! Every `crate::geometry::...` path in app and UI resolves through the
//! glob exactly as before the move.

pub use camcraft_geometry::geometry::*;
pub use camcraft_geometry::solidcraft;
pub use camcraft_geometry::{dxf, geo2d, sketch_chains, sketch_data};

pub mod import;

pub use import::{ImportError, load_model};
