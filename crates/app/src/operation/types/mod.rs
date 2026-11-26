//! Plain operation data: what an operation stores. The component
//! wrapper lives in [`crate::schema::Operation`]; nothing in this module
//! touches the world, an entity, or the schema - which is exactly the
//! law M4.1 R0 draws (docs/M4.1-CRATE-SPLIT.md): the plain types' home
//! imports no schema, so the wrappers and the data can part companies.
//! The world-facing methods of these same types (`resolve_height`,
//! `z`, `stamp_with`, `current_stamp`) stay in the parent module as
//! separate impl blocks.
//!
//! Provenance: our own domain model; the From-reference/offset height
//! expression is general CAM practice (public documentation of
//! heights semantics), no vendor internals.


mod area;
mod avoid;
mod dwell;
mod fine_bore;
mod height;
mod links;
mod params;
mod plasma;
mod sinker;
mod sane;
pub use sane::POCKET_ADAPTIVE_RETIRED;
mod sel;
mod start_side_entry;
mod tilt_basis;
pub use tilt_basis::CURRENT_TILT_BASIS;
// #1444: the Slot Undercut's stored parameters.
mod undercut;
pub use undercut::SlotUndercutOp;
mod waterjet;
mod wire_edm;

pub use area::{
    AREA_DISTANCE_MAX, AreaControls, BoundaryMode, BoundaryShape, ChainSide, Containment, CornerStyle,
    ExtraBoundary, OpenBoundary, area_applies, area_unsupported,
};
pub use crate::machining2d::{
    CornerRelief, CornerReliefChoice, CornerReliefKind, CornerReliefOverride, TBoneDirection,
};
pub use avoid::{
    DoNotTouch, describe_sel, do_not_touch_applies, do_not_touch_overlap_issue,
    do_not_touch_unsupported,
};
/// The coolant choice is the stream's own type (#112): one enum from the
/// op field to the post's `PARAM Coolant`, no mapping to drift.
pub use camcraft_post::post::CoolantMode;
/// The after-operation action is the stream's own type too (#110).
pub use camcraft_post::post::AfterOperation;
pub use dwell::MAX_DWELL_S;
pub use height::{Height, HeightReference};
pub use params::{OpParams, Tilt};
pub use plasma::{PlasmaComp, PlasmaOp};
pub use waterjet::{JetLead, JetSide, JetTabsOp, WaterjetOp};
pub use sinker::{
    SinkerBurnOp, SinkerDatum, SinkerOrbit, SinkerOrbitKind, SinkerPass, SinkerSide,
    SinkerSideBurnOp, SinkerSideDirection,
};
pub use wire_edm::{
    WireContourOp, WireFourAxis, WirePairingRule, WireSide, WireSkim, WireSyncPair, WireTabs, WireWidening,
    CONDITION_ID_MAX, DEFAULT_CLEARANCE, DEFAULT_LEAD_ARC_RADIUS, FOUR_AXIS_TOLERANCE, SKIMS_MAX, SYNC_PAIRS_MAX, VERIFY_TOLERANCE_MAX,
};
pub use sel::{ChainOverride, FeatureSel, GenStamp, GeoSel, OpStatus, OpType, SrcRef};
pub(crate) use sel::LEGACY_WCS_NAMES;
