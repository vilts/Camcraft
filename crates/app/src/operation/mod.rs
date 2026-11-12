//! Machining operations: what to cut, from where, and the generation
//! orchestration that turns an operation into a verified toolpath.
//!
//! Provenance: our own domain model - operation types, source geometry and
//! staleness tracking are specced from public documentation of commercial
//! CAM workflows; the algorithms invoked here live in `machining2d` and
//! `verify` with their own provenance. No vendor source was consulted.
//!
//! The operation is data (`SrcRef`, `OpParams`, `GenStamp`); the behavior
//! is the two free functions at the bottom:
//!
//! - [`generate`] resolves tool and source geometry in the current world,
//!   builds the cutting region, runs the matching `machining2d` generator
//!   with the tool's diameter injected, and verifies the result. Pure read:
//!   it never touches the world, so commands can run it at construction
//!   and replay its stored output byte-identically through undo/redo.
//! - [`current_stamp`] digests everything the cached path depends on.
//!   Staleness is derived from a stamp mismatch (AGENTS.md: no stored
//!   `Stale` flag), so an edit anywhere the digest reads - tool geometry,
//!   stock size and placement, model move, parameters - turns the cache
//!   grey without any command writing a status.
//!
//! Region rule per operation, at the level the paths live in:
//! - Face: the XY silhouette of the source (`slice::project`), cut plane
//!   from `cut_z`.
//! - Contour / Pocket / Trace: the source's cross-section at `top_z`
//!   (`slice::slice`, half-open; a grazing slice is retried one micron in).
//! - Slot: the closed-form stadium between the two endpoints.
//! - Drill: the XY silhouette again, plus one rim loop per hole whose rim
//!   the projection did not already carry - the rim is what makes the
//!   gouge law's tangency test mean something at a plunge.
//!
//! Meshes are sliced because STEP import is tessellate-only (no curve data
//! survives), which is also what makes STL, OBJ and STEP sources take the
//! identical path.
//!
//! A `SrcRef::Selection` builds its region from the picked features
//! instead: faces contribute their boundary loops projected to XY, closed
//! edge chains their ring projected to XY, coincident rims dedupe. Face
//! ops demand a near-horizontal face, Pocket demands a closed boundary,
//! and an open run is either a Contour centerline pass along its points
//! or, for a facing op, a limit extended out to the stock (M1.5.1) -
//! never a silent drop. Preview and generation read the same walk, so
//! the two cannot disagree about where that pass or that boundary runs.

use std::collections::HashMap;
use std::hash::Hasher;

use flecs_ecs::prelude::World;

use crate::cl::Toolpath;
use crate::drilling::{self, CycleChoice, Hole};
use crate::finishing3d::{self, DeburrParams, Finish3DParams};
use crate::geo2d::{Loop, Point2, Region};
use crate::geometry::Triangle;
use crate::geometry::feature::{self, LoopKind};
use crate::geometry::revision;
use crate::geometry::slice::{self, SliceError};
use crate::geometry::{Point3, transform_point};
use crate::machining2d::{
    self, ChamferParams, CircularParams, ContourParams, FaceParams, GenError, Lead, PocketParams,
    PocketStrategy, Side, SlotParams, TraceParams,
};
use crate::machining3d::{self, Entry3D, Pocket3DParams};
use crate::model;
use crate::schema::Operation;
use crate::verify::{self, VerifyContext, Warning};

mod heights;
pub use heights::*;
mod heights_sel;
pub use heights_sel::*;
mod kinds;
pub use kinds::*;
mod seed;
pub use seed::seed_params;
pub mod tool_law;
pub mod tool_gate;
mod params;
mod trochoid_op;
mod stamp;
pub use stamp::*;
mod stamp_stock;
pub use stamp_stock::reads_setup_stock;
pub(crate) use stamp_stock::write_setup_stock;
mod material;
pub use material::*;
mod material_region;
pub(crate) use material_region::selection_drives_region;
mod circle_fit;
pub use circle_fit::{CIRCLE_FIT_TOL, circle_of_region, circular_fit_of};
mod material_source;
mod footprint;
pub use footprint::machined_footprint;
mod probe;
// #108: the Probe operation's generation and protected-move law.
pub mod probe_op;
// #670: the Plasma Profile's seed and generation glue.
pub mod plasma_op;
// #171: the Probe op seeded from one picked face, edge, vertex or round wall.
mod probe_pick;
mod probe_pick_round;
pub use probe_pick::{PROBE_AXIS_TOL_DEG, ProbeSeed, probe_account, probe_from_picks, probe_seed, seeded_params};
pub(crate) use probe::*;
mod selection;
pub use selection::*;
mod picks;
pub use picks::*;
mod stitch;
use stitch::*;
mod stitch_cross;
use stitch_cross::*;
mod run_out;
use run_out::*;
mod area;
use area::*;
mod boundary;
pub use boundary::AreaCtx;
use boundary::AreaApplied;
mod boundary_turn;
mod contour_open;
mod finish_clip;
mod finish_entry;
mod boundary_ctx;
mod boundary_picks;
pub use boundary_picks::{
    PickShape, area_pick_issue, area_refusal, boundary_pick_shape, extend_to_loop, run_pick, toggle_run,
};
pub use boundary_ctx::op_stamp;
use boundary_ctx::*;
mod job;
pub use job::*;
mod generate;
pub mod body_judged;
/// #339 slice 5: links routed over the setup's fixtures.
pub mod fixture_route;
mod cross_entry;
mod cross_sag;
mod cross_part;
mod cross_spindle;
mod cross_stock;
mod drill_reach;
mod drill_prior;
mod carried_stock;
pub use carried_stock::{CARRIED_OUTSIDE_DECLARED, CARRIED_OUTSIDE_NOTED, carried_problems};
pub(crate) use drill_prior::reads_prior;
mod plunge_solid;
mod rotary_law;
pub(crate) mod spindle_law;
pub use spindle_law::{spindle_reading, SpindleReading, SPINDLE_TOL};
mod rotary_gate;
mod air_law;
// #864: what a kind's gouge laws need of its model's fidelity.
pub(crate) mod fidelity_law;
mod top_stock;
mod stock_top_law;
pub use top_stock::TOP_STOCK_WARNING;
mod turning_credit;
mod turning_feeds;
mod turning_holder;
pub(crate) use turning_holder::body_axes as lathe_body_axes;
mod turning_holder_scene;
mod turning_outline;
mod turning_rapids;
mod turning_stock;
pub use air_law::AIR_NOTE;
pub use generate::*;
#[cfg(test)]
mod tests;

pub mod types;
pub use types::*;
