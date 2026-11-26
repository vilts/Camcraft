//! Bounded ASCII DXF import. Coordinates stay in drawing units; callers must resolve units.
//! Provenance: Autodesk's published DXF Reference; original implementation, no vendor code.
//!
//! Only planar, zero-width model-space geometry is accepted. Arcs remain analytic. SPLINE and
//! ELLIPSE are laid as tangent arcs within [`CURVE_TOLERANCE_MM`] (#1865).
//! This module performs no file I/O and has no dependency on the application or ECS.

mod biarc;
mod blocks;
mod curve_fit;
mod curves;
mod dictionaries;
mod document_writer;
mod entities;
mod nurbs;
mod pairs;
mod polyline;
mod reader;
mod types;
mod units;
mod writer;

pub use curves::{CURVE_MAX_RADIUS_MM, CURVE_TOLERANCE_MM};
pub use document_writer::{write, write_bytes};
pub use reader::{parse, parse_with_limits};
pub use types::*;
pub use units::{Unit, Units};
pub use writer::{ExportEntity, to_text};
