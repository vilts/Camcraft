//! Simultaneous-axis finishing kinds - the 5-axis freeform trio of
//! stage C (contour, parallel, swarf, all posing from the SURFACE
//! NORMAL through `tilt`), the ruled swarf reads of stage D
//! (`ruling`), and the rotary family of stage E (`axis` + `rotary`
//! for the finishing pair, `rotary_offset` for the rough ladder, all
//! posing from the RADIAL of a fitted axis of revolution), and the
//! stamped pocket ladder of stage E3 (`rough`: the prismatic rough in
//! freeform ground, posed by the same normal law as the finishing
//! trio), and the drive-curve contour of stage J (`curve`: the pass
//! that walks the caller's picked chains, resting the ball on the
//! vertex's own fan of planes).
//!
//! Where stage B kept the tool vertical and moved the PART (3+2
//! indexing through a tilted frame), these kinds keep the PART still
//! and move the TOOL AXIS: every CL move carries the direction the
//! spindle points, computed from the surface it is cutting. They
//! share the lean law in `tilt`, the pass that stamps an axis onto a
//! finished path in `stamp`, and the ball-end contact geometry
//! (`crate::finishing3d::support`); all ride stage A's axis-stamped
//! path builder. `linalg` holds the eigen and solve kernels the
//! fits are written from.
//!
//! Provenance: lead/lag/side-tilt angle definitions and level/parallel
//! 5-axis patterns from public multi-axis milling literature (ISO
//! 10791-10 test-part tradition, standard CAM manuals); rotary
//! axis-of-revolution detection from the published normal-concurrency
//! law; written from those sources and our own design notes only.

mod avoid;
mod curv;
mod curve_walk;
pub mod deburr_axis;
#[cfg(test)]
mod deburr_axis_tests;
mod deburr_pose;
mod deburr_clear;
mod entry_ramp;
mod linalg;
mod stamp;
mod stamp_pose;
mod stamp_rise;
#[cfg(test)]
mod stamp_tests;
pub mod axis;
mod axis_consensus;
pub mod axis_helix;
mod axis_point;
mod axis_poles;
mod axis_refine;
mod axis_robust;
pub mod axis_stated;
pub mod contour5d;
pub mod curve;
pub mod flow;
pub mod parallel5d;
pub mod rough;
pub mod rotary;
pub mod rotary_offset;
mod rotary_seam;
mod rotary_seam_angle;
mod rotary_sheet;
pub mod rotary_bar;
mod rotary_ends;
mod rotary_flow;
mod rotary_flow_emit;
mod rotary_flow_guard;
mod rotary_flow_pass;
mod rotary_flow_unwrap;
mod rotary_links;
pub mod rotary_pick;
mod wrap_exit;
mod retip;
pub mod ruling;
pub mod stamp_link;
pub mod swarf;
pub mod tilt;

pub use avoid::Shank;
pub use contour5d::{contour5d, contour5d_seamed};
pub use curve::{contour_curve, CurveParams};
pub use deburr_axis::{multiaxis_deburr, AxisDeburrParams, MAX_TILT_CAP_DEG};
pub use deburr_pose::DeburrAxisMode;
pub use flow::{flow3d, flow5d};
pub use parallel5d::parallel5d;
pub use rough::{multiaxis_rough, multiaxis_rough_prev};
pub use rotary::{rotary_contour, rotary_contour_stated, rotary_parallel, rotary_parallel_in, rotary_parallel_over, rotary_parallel_over_stated};
pub use rotary_bar::{revolved_bar, revolved_bar_stated};
pub use rotary_flow::{rotary_flow, RotaryFlowParams};
pub use rotary_offset::{rotary_offset, rotary_offset_developed, rotary_offset_in, rotary_offset_stated};
pub use stamp_link::{guard_turns, link_swings, LINK_TURN_DEG};
pub use swarf::{swarf5d, swarf5d_advanced};
pub use tilt::{command_axis, AxisControl};
