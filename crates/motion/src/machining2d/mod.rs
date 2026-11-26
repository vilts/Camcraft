//! 2D machining generators: region + parameters in, CL path out.
//!
//! Provenance: serpentine facing, offset contouring, zigzag and waterline
//! pocketing, stadium slotting rows and engraved trace paths are textbook
//! CAM strategies as described in machine-tool handbooks and published CAM
//! literature; the arc fitter is the classic three-point circle with a
//! two-sided sagitta bound (geometry handbooks, e.g. Farin's curves and
//! surfaces primer); turning-point blends are RS-274 / ISO 6983 G2/G3
//! path filleting (general machining knowledge), and the contour /
//! pass-link corner fillets the classical circle tangent to two lines. Every number (parameter model, entry/exit overruns,
//! snap rules) is ours. Pure module: no flecs, no
//! world access - the command layer resolves geometry into regions, this
//! module only cuts them.
//!
//! Prime Directive: parameters are validated before any geometry is
//! touched; impossible shapes return `GenError` (or a clean empty result
//! where the plan says collapse means empty), never a partially linked
//! path. Every returned path is chained, finite, and posts real motion.




mod arc_biarc;
mod arc_cross;
mod arc_proof;
mod arc_side;
mod arc_touch;
mod arc_leads;
mod arcs;
mod blend;
mod blend_cross;
mod bore;
mod bore_core;
mod chamfer;
mod chamfer_ride;
mod chamfer_side;
mod circular;
mod circular_helix;
mod thread_mill;
mod thread_passes;
mod thread_ride;
mod trochoid_air;
mod trochoid_bore;
mod trochoid_level;
mod trochoid_loops;
mod trochoid_slot;
mod contour;
mod contour_region;
mod contour_auto;
mod contour_bore;
mod contour_chain;
mod contour_comp;
mod contour_comp_pick;
mod contour_comp_room;
mod contour_depth;
mod contour_entry;
mod contour_link;
mod contour_params;
mod contour_peck;
mod contour_run_side;
mod contour_run_stock;
mod contour_start;
mod contour_step;
mod contour_wall_ramp;
mod contour_whole;
mod contour_whole_fence;
mod contour_whole_reach;
mod contour_whole_slice;
mod air_entry;
mod descent;
mod descent_dive;
mod descent_xy;
mod entry_warn;
mod equal_steps;
mod face;
mod face_entry;
mod face_holes;
mod face_keep;
mod face_raster;
mod fillet;
mod gen_error;
mod ghost;
mod fillet_arc;
mod fillet_tally;
mod lead_ext;
mod lead_fit;
mod lead_seam;
mod ladder_links;
mod level_flanks;
mod level_order;
mod level_rings;
mod levels;
mod microlift;
mod pass_start;
mod ring_fill;
mod ring_links;
mod ring_side;
mod pocket;
mod pocket_plan;
mod pocket_emit;
mod pocket_zigzag;
mod pocket_hsm;
mod pocket_hsm_start;
mod pocket_strategy;
mod pocket_troch;
mod predrilled;
mod adaptive;
mod adaptive_method;
mod adaptive_troch;
mod adaptive_avoid;
mod adaptive_troch_clean;
mod adaptive_troch_lanes;
mod adaptive_troch_level;
mod adaptive_troch_slice;
mod rest2d;
mod rest_prior;
mod rest_prior_adaptive;
mod rest_anchor;
mod rest_rings;
mod chord_clear;
mod ring;
mod tab_bridge;
mod tab_clear;
mod tab_kerf;
mod sketch_trace;
mod slots;
mod slot_band;
mod trace_avoid;
mod slot_ends;
mod slot_lap;
mod slot_picks;
mod slot_room;
mod slot_turns;
mod undercut;
mod unreached;
mod vcarve;
mod vcarve_axis;
mod vcarve_walk;
mod waterscan;
mod pocket_avoid;
mod pocket_avoid_level;
mod corner_relief;
mod corner_relief_spur;

#[cfg(test)]
#[allow(unused_imports)]
use {
    arcs::*, bore::*, chamfer::*, circular::*, contour::*, face::*, levels::*, pocket::*,
    ring::*, slots::*, thread_mill::*, waterscan::*,
};
#[cfg(test)]
mod tests;
#[cfg(test)]
mod bore_core_tests;
#[cfg(test)]
mod thread_tests;
#[cfg(test)]
mod thread_pass_tests;

pub use arcs::{fit_arcs, fit_arcs_sided, Seg, ARC_FIT_TOL};
pub use blend::MIN_BLEND_RADIUS;
pub use bore::{bore, bore_started, BoreHole, BoreParams};
pub use bore_core::{bore_core_gap, bore_floor_reach, bore_leaves_core, cutting_radius_at, max_bore_diameter};
pub use chamfer::{chamfer, chamfer_open_runs, ChamferParams};
pub use chamfer_ride::{ride_loops as chamfer_ride_loops, wall_lean as chamfer_wall_lean};
pub use circular::{circular, CircularParams};
pub use contour::{contour, contour_on_bodies, contour_on_part, contour_on_picked, ContourParams, Lead, Side};
pub use contour_whole::{contour_whole_bodies, contour_whole_bodies_with};
pub use contour_auto::{auto_side, AutoSide, AUTO_BAND, AUTO_PROBE_DZ};
pub use contour_chain::contour_along_chain;
pub use contour_run_side::{contour_open_run, offset_run_side, SectionAt};
pub use contour_run_stock::contour_open_run_sided;
pub use contour_link::DEFAULT_LEVEL_RAMP_ANGLE;
pub use crate::links::{Link, Links};
pub use equal_steps::{cut_levels_for, equal_stepdown, ladder_stepdown};
pub use face::{face, face_led, face_over, face_over_kept, face_picked, FaceKeep, FaceParams, GenError, MAX_PASSES};
pub use face_holes::FaceHoles;
pub use ghost::Ghost;
pub use face_entry::{air_lead, AIR_MARGIN};
pub use fillet::{fillet_corners, Legal};
pub use fillet_tally::{corner_arc_radius, fillet_floor, FilletTally};
pub use levels::{cut_levels, cut_levels_under};
pub use corner_relief::{
    CornerRelief, CornerReliefChoice, CornerReliefKind, CornerReliefOverride, CornerReliefPlan,
    InsideCorner, ResolvedRelief, TBoneDirection, capsule_region, inside_corners, plan_corner_reliefs,
};
pub use corner_relief_spur::pocket_relief_plan;
pub use level_rings::level_ring_plan;
pub use crate::approach::OUTSIDE_AIR_REFUSAL;
pub use entry_warn::{ENTRY_STAND_IN_NOTE, PLUNGE_INTO_STOCK_NOTE, entry_stand_ins, entry_warnings, is_entry_stand_in};
pub use pass_start::{
    PASS_START_NOTE, PASS_START_REST_NOTE, PASS_START_ZIGZAG_NOTE, PassStart, StartSide, advised, entry_advice, pass_start_fallbacks,
};
pub use pocket::{
    pocket, pocket_entered, pocket_entered_with_relief, pocket_started, pocket_started_on, PocketParams, PocketStrategy,
};
pub use pocket_avoid::{
    pocket_avoid, pocket_avoid_entered_started, pocket_avoid_entered_with_relief, pocket_avoid_started,
    pocket_avoid_started_on, pocket_avoid_with_relief,
};
pub use predrilled::PreHole;
pub use rest2d::{
    rest_plan, rest_pocket, rest_pocket_avoid_with_relief, rest_pocket_with_relief, rest_strategy, RestParams,
    RestPlan, RestSource, REST_FILM, REST_MARGIN, REST_NEEDS_STOCK, REST_REACH,
};
pub use rest_prior::{
    rest_plan_stock, rest_pocket_previous, rest_pocket_previous_avoid_with_relief, rest_pocket_previous_with_relief,
    rest_stock_from_columns, PRIOR_Z_TOL,
};
pub use rest_prior_adaptive::{rest_adaptive_previous, REST_ADAPTIVE_ENTERED, REST_ADAPTIVE_NO_STOCK};
pub use ring::{on_boundary, ring_is_circular};
pub use sketch_trace::TraceStrokes;
pub use slot_band::slot_centre_band;
pub use pocket_plan::pocket_tool_fits;
pub use slots::{chord_clears, slot, trace, SlotParams, TraceParams};
pub use trace_avoid::trace_avoid;
pub use slot_ends::extended_ends as slot_extended_ends;
pub use slot_picks::picked_ends as slot_picked_ends;
pub use slot_room::{SlotBound, SlotFit, FIT_TOL as SLOT_FIT_TOL};
pub use undercut::{undercut, undercut_band, undercut_station, UndercutParams, MAX_BULGE as UNDERCUT_MAX_BULGE};
pub use vcarve::{v_carve, VCarveParams};
pub use unreached::{unreached_corner_note, unreached_corners, unreached_note, unreached_pieces};
pub use adaptive::{rest_adaptive, rest_levels, RestStock, StockFloor, REST_STOCK_FILM};
pub use adaptive_method::AdaptiveMethod;
pub use pocket_hsm_start::trochoidal_pass_start_refusal;
pub use adaptive_troch::{AdaptiveTrochParams, adaptive_trochoidal, adaptive_trochoidal_with};
pub use adaptive_avoid::{adaptive_spiral_avoid, adaptive_spiral_avoid_on, adaptive_trochoidal_avoid};
pub use trochoid_air::lifted_cut;
pub use trochoid_slot::{
    plan_trochoidal_slot, trochoidal_slot, TrochoidParams, TrochoidPlan, TrochoidReturn, DEFAULT_MAX_ENGAGEMENT,
    DEFAULT_MICROLIFT,
};
pub use thread_mill::{thread_mill, PassSchedule, ThreadHole, ThreadParams, ThreadPasses, MAX_SPRING_PASSES, EXTERNAL_THREAD_DEPTH_PER_PITCH, THREAD_DEPTH_PER_PITCH};
pub(crate) use pocket::plan_passes;
pub(crate) use pocket_zigzag::{SeamWrap, ringed_passes as zigzag_ringed_passes};
pub use pocket_zigzag::boundary_rings_probe;
pub(crate) use adaptive_troch_level::{TrochLevel, WHO_3D, plan_troch_level, refusal_text as troch_refusal_text};
pub(crate) use adaptive::PLAN_SAG;
// The HSM planner's door, its arc proofs and its microlift link, read
// by the 3-D ladder's per-level port (`machining3d::hsm`, M11.4).
pub(crate) use adaptive::Return;
pub(crate) use arc_cross::{crossing_free, moves_cross_prior, seg_crosses_prior};
pub(crate) use arc_proof::{ProofTally, fit_proved};
pub(crate) use arcs::push_segs;
pub(crate) use microlift::link_at as microlift_link;
pub(crate) use pocket_hsm::{adaptive_passes, choose_entry, in_air};
pub use pocket_hsm::{RINGS_FALLBACK, rings_fallbacks};
pub(crate) use waterscan::chord_crosses_path;
