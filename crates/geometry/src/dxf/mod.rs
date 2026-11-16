//! Bounded ASCII DXF import. Coordinates stay in drawing units; callers must resolve units.
//! Provenance: Autodesk's published DXF Reference; original implementation, no vendor code.
//!
//! Only planar, zero-width model-space geometry is accepted. Arcs remain analytic.
//! This module performs no file I/O and has no dependency on the application or ECS.

mod entities;
mod pairs;
mod polyline;
mod reader;
mod types;
mod units;

pub use reader::{parse, parse_with_limits};
pub use types::*;
pub use units::{Unit, Units};
