//! The tool law's table half (M10.1 A): which cutter kinds each operation
//! kind can machine with, as named sets the `KindInfo` rows point at.
//!
//! Provenance: machine-tool handbook practice (flat-floor milling, ball
//! finishing, cone chamfering, drill/tap/bore cycle tooling, lathe insert
//! and blade families) read against our own generators' tool doors. No
//! vendor material consulted.
//!
//! The table gates the FAMILY match only. The generators' own shape
//! refusals (Finish3D's ball door, the chamfer's cone door, the lathe
//! kinds' blade doors, the thread mill's pitch door) stay exactly where
//! they are - this table names what may be offered, the generator still
//! judges the cutter it is handed. Where a row was ambiguous the set is
//! the narrower one: what the generator actually reads.

use crate::tools::ToolKind;

use super::KindInfo;

/// Flat-floor 2D kinds (Face, Contour, Pocket, Slot, 2D Adaptive, 2D
/// Offset, Circular, the milled Bore): the kernels read one diameter as
/// a flat-bottomed cylinder standing on the floor plane. A bull nose is
/// that cylinder with a corner radius (the everyday roughing form); a
/// ball would scallop the floor the kind promises flat.
pub const FLAT_MILLS: &[ToolKind] = &[ToolKind::EndMill, ToolKind::BullNose];

/// Centerline kinds (Trace, Engrave): the tool's center rides ON the
/// line at the authored depth and the tangency laws stand down, so any
/// mill form cuts what it is - the cone (V-bit) is the engraving tool.
pub const CENTERLINE: &[ToolKind] = &[
    ToolKind::EndMill,
    ToolKind::BallNose,
    ToolKind::BullNose,
    ToolKind::Taper,
];

/// Cone-on-the-edge kinds (2D Chamfer, Deburr): the generatrix lies on
/// the break, read off `tip_angle` - the chamfer mill and nothing else.
pub const CONE: &[ToolKind] = &[ToolKind::Taper];

/// The helical thread mill (#894): the multi-form and single-form
/// thread-mill kinds and nothing else. A Tap is a rigid tap - one
/// helical orbit of it is a crash - so the Tap-as-thread-mill reading
/// of M2.4 C is gone; an operation that still mounts one names it.
pub const THREAD_MILL: &[ToolKind] = &[ToolKind::ThreadMill, ToolKind::ThreadMillSingle];

/// The drilling cycles, Z or cross (live tooling on a mill-turn): the
/// cycle the tool expands into - a drill drills (plain/peck/deep and a
/// user-chosen spot), a tap taps, a boring bar bores. Spot is never
/// automatic, and no spot-specific kind exists to claim here.
pub const HOLE_CYCLES: &[ToolKind] = &[ToolKind::Drill, ToolKind::Tap, ToolKind::Bore];

/// The 3D rough ladder (3D Pocket): planes cut with a cylinder of the
/// tool's diameter. Every mill form at the same tip sits inside that
/// cylinder, so each leaves material rather than taking it - roughing
/// takes all three.
pub const ROUGH_MILLS: &[ToolKind] = &[ToolKind::EndMill, ToolKind::BallNose, ToolKind::BullNose];

/// The tangent-locus kinds (3D Finish and the whole multi-axis run): the
/// path is a sphere's support scan, and every one of these generators
/// refuses a non-ball at its door - the table says the same, earlier.
pub const BALL: &[ToolKind] = &[ToolKind::BallNose];

/// Lathe nose kinds (Facing, Profile Rough/Finish, Chamfering, Taper and
/// Multi-Pass Turning): the insert's nose circle is the cut.
pub const INSERT: &[ToolKind] = &[ToolKind::Turning];

/// Band kinds (Groove Rough/Finish, Parting): the blade's width is the
/// footprint.
pub const BLADE: &[ToolKind] = &[ToolKind::Groove];

/// Threading: the V-form insert.
pub const THREAD_INSERT: &[ToolKind] = &[ToolKind::Thread];

/// Form turning: the ground gauge.
pub const FORM: &[ToolKind] = &[ToolKind::Form];

/// Turned boring: the bar riding the hole's wall on its nose.
pub const BAR: &[ToolKind] = &[ToolKind::Bore];

impl KindInfo {
    /// Whether a tool of `kind` belongs to this operation kind's family
    /// (M10.1 A). The family match only - a `true` here is no promise the
    /// generator's shape door will pass the particular tool.
    #[must_use]
    pub fn accepts(&self, kind: ToolKind) -> bool {
        self.tool_kinds.contains(&kind)
    }

    /// Whether this kind cuts with a tool from the library at all. A
    /// wire EDM kind does not (#532, [`NO_TOOL`]): its operation names
    /// no tool, and every tool door reads this word first.
    #[must_use]
    pub fn mounts_tool(&self) -> bool {
        !self.tool_kinds.is_empty()
    }
}

/// The touch probe (#108): the Probe operation mounts nothing else, and
/// no cutting kind mounts a probe.
pub const PROBE: &[ToolKind] = &[ToolKind::Probe];

/// The plasma torch (#670): the plasma operations mount nothing else,
/// and no cutting kind mounts a torch.
pub const TORCH: &[ToolKind] = &[ToolKind::Torch];

/// No library tool (#532): a wire EDM kind mounts none - the wire's
/// diameter and spark gap are the machine's condition (#531), and a
/// tool beside it would be a second source for the same number. The
/// one empty set; [`KindInfo::mounts_tool`] is how the doors ask.
pub const NO_TOOL: &[ToolKind] = &[];

/// The radius a mill tool removes with certainty (#266): half its
/// diameter, except a Tap-kind tool (tap or thread mill), whose sure
/// removal is its thread's MINOR cylinder, `d / 2 - THREAD_DEPTH_PER_PITCH
/// x pitch` (ISO 724, the depth `machining2d::thread_mill` reads). CAD
/// models a tapped hole at that minor size and the thread band outside
/// it is meant to be cut, so judging a tap at its full diameter called
/// every such hole a gouge. Into solid metal the core still gouges.
/// The program sim sweeps this radius (`stock_sim::plan_program`), and
/// the generator's verify re-reads a Z tap cycle's `Gouge` convictions
/// at it (`tap_core`); its rapids keep the full diameter. A pitch too
/// coarse to leave a core is refused by name.
pub fn cutting_radius(tool: &crate::tools::Tool) -> Result<f64, String> {
    let radius = tool.diameter * 0.5;
    // A thread mill sweeps its tooth roots the same way (#894): the
    // teeth cut only the thread band, at its coarsest pitch.
    if !matches!(tool.kind, ToolKind::Tap | ToolKind::ThreadMill | ToolKind::ThreadMillSingle) {
        return Ok(radius);
    }
    let core = radius - crate::machining2d::THREAD_DEPTH_PER_PITCH * tool.pitch;
    if !(core.is_finite() && core > 0.0) {
        return Err(format!("{} leaves no thread core (diameter {}, pitch {})", tool.kind, tool.diameter, tool.pitch));
    }
    Ok(core)
}

/// The sinker electrode (#541): the sinker operations mount nothing
/// else, and no cutting kind mounts an electrode.
pub const ELECTRODE: &[ToolKind] = &[ToolKind::Electrode];
