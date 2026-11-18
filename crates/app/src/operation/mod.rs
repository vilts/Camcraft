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
mod hole_digest;
mod slot_fit;
mod trochoid_op;
mod stamp;
mod stamp_tool;
pub use stamp::*;
mod stamp_stock;
pub use stamp_stock::reads_setup_stock;
pub(crate) use stamp_stock::write_setup_stock;
mod material;
pub use material::*;
mod material_region;
pub(crate) use material_region::selection_drives_region;
mod circle_fit;
// #1504: the plain Slot's room read off two picked edges or a face.
mod slot_pick;
pub use slot_pick::{slot_bound_of, slot_fit_of, slot_from_picks};
pub use circle_fit::{CIRCLE_FIT_TOL, circle_of_region, circular_fit_of};
mod material_source;
// #1071: the thread mill's major-circle rims and solid-law bores.
mod thread_rims;
mod footprint;
pub use footprint::machined_footprint;
mod probe;
// #108: the Probe operation's generation and protected-move law.
pub mod probe_op;
// #670: the Plasma Profile's seed and generation glue.
pub mod plasma_op;
// #894: the thread-milling operation's door, kernel plan and its two verdict readings.
pub mod thread_op;
mod thread_entry;
mod thread_r_plane;
// #985: external threads - the boss door, the crest region, the neighbour law.
mod thread_boss;
pub mod wire_op;
mod wire_ruled;
// #534: the wire laws' inputs, read live at the post.
pub mod wire_verify;
// #541: the sinker EDM burn to depth.
pub mod sinker_op;
// #543: the sinker's rough and finish passes.
pub mod sinker_pass;
// #547: picked cavity faces read as a sinker electrode.
pub mod sinker_faces;
// #546: the sinker burn's electrode wear compensation.
pub mod sinker_wear_op;
// #545: the Sinker Side Burn's seed, laws and generation.
pub mod sinker_side_op;
// #171: the Probe op seeded from one picked face, edge, vertex or round wall.
mod probe_pick;
mod probe_pick_round;
pub use probe_pick::{PROBE_AXIS_TOL_DEG, ProbeSeed, probe_account, probe_from_picks, probe_seed, seeded_params};
pub(crate) use probe::*;
mod selection;
mod contour_auto_pick;
pub use selection::*;
mod picks;
pub use picks::*;
mod stitch;
use stitch::*;
mod stitch_cross;
use stitch_cross::*;
mod open_band;
mod run_out;
use run_out::*;
mod area;
#[cfg(test)]
mod stock_top_frame;
use camcraft_motion::stock_top_frame::stock_top_in_frame;
use area::*;
mod pocket_memo;
pub use pocket_memo::pocket_slices;
mod selection_memo;
pub use selection_memo::{contour_auto_side_fresh, selection_account_fresh, selection_walks};
mod boundary;
pub use boundary::AreaCtx;
use boundary::AreaApplied;
mod boundary_turn;
use camcraft_geometry::boundary_chains;

fn boundary_chain_error(error: boundary_chains::BoundaryError) -> GenError {
    match error {
        boundary_chains::BoundaryError::Invalid(s) => GenError::Invalid(s),
        boundary_chains::BoundaryError::Empty(s) => GenError::Empty(s),
        boundary_chains::BoundaryError::Geometry(e) => e.into(),
    }
}
mod contour_open;
mod finish_clip;
mod finish_entry;
mod boundary_ctx;
mod boundary_picks;
pub use boundary_picks::{
    PickShape, area_pick_issue, area_refusal, boundary_pick_shape, extend_to_loop,
    extend_to_loop_in, run_pick, toggle_run,
};
pub use boundary_ctx::op_stamp;
use boundary_ctx::*;
// #763: the one machining-frame seam — the composed door for an
// operation's world reads and CL wrap (part_alignment x Tilt).
mod frame;
pub use frame::OpFrame;
mod wcs_yaw;
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
mod bore_reach;
use camcraft_motion::break_through;
pub use bore_reach::DetectedHoles;
pub use break_through::BELOW_STOCK_NOTE;
mod drill_prior;
mod carried_stock;
pub use carried_stock::{CARRIED_OUTSIDE_DECLARED, CARRIED_OUTSIDE_NOTED, carried_problems};
pub(crate) use drill_prior::reads_prior;
mod plunge_solid;
mod predrilled_entry;
pub mod rest_op;
pub use rest_op::REST_PREDRILLED;
#[cfg(test)]
mod rest_op_tests;
#[cfg(test)]
mod predrilled_entry_tests;
#[cfg(test)]
mod authored_law_tests;
mod rotary_law;
pub(crate) mod spindle_law;
pub use spindle_law::{spindle_reading, SpindleReading, SPINDLE_TOL};
mod rotary_gate;
mod air_law;
// #1595: the depth law an Engrave or Trace on the part carries itself.
pub mod mark_law;
// #1603: the part a pocket-family op keeps, judged on the op.
pub mod keep_law;
// #1749: the 3D Pocket's footprint past its open edges.
mod open_side;
// #1628: the post's live re-judge of the parts `keep_law` keeps.
pub mod kept_rejudge;
// #864: what a kind's gouge laws need of its model's fidelity.
pub(crate) mod fidelity_law;
mod clearance_stock;
mod top_stock;
mod stock_top_law;
// #266: a tap judged at its thread's core.
mod tap_core;
pub use clearance_stock::{CLEARANCE_STOCK_WARNING, creation_advice as clearance_in_stock_advice};
pub use top_stock::{CARRIED_STOCK_NOTE, TOP_STOCK_WARNING};
mod turning_credit;
mod turning_axis;
mod turning_feeds;
mod turning_holder;
pub(crate) use turning_holder::body_axes as lathe_body_axes;
mod turning_holder_scene;
mod turning_holder_side;
mod turning_holder_steel;
pub use turning_holder_steel::STEEL_UNREAD_NOTE;
pub(crate) use turning_holder_steel::lathe_steel_hits;
mod turning_outline;
mod turning_rapids;
mod turning_stock;
pub use air_law::AIR_NOTE;
pub use generate::*;
#[cfg(test)]
mod tests;
#[cfg(test)]
mod tests_thread_stamp;

pub mod types;
pub use types::*;
