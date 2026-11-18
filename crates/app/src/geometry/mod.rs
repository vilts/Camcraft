//! Geometry: the camcraft-geometry crate re-exported whole (M4.1 R1,
//! docs/M4.1-CRATE-SPLIT.md), plus the truck-bound mesh importers, which
//! stay in app by design. Solidcraft domain import lives in geometry.
//! Every `crate::geometry::...` path in app and UI resolves through the
//! glob exactly as before the move.

pub use camcraft_geometry::geometry::*;
pub use camcraft_geometry::solidcraft;

pub mod import;

pub use import::{ImportError, load_model};
