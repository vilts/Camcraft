//! Split from `types.rs` by M5.0.1; structure only.

use crate::machining2d::Lead;

/// The legacy work-offset ENTITY names (#931): the implicit setup's WCS
/// is `G54`, and files before v54 named per-slot WCS entities
/// `<setup>/G54`..`<setup>/G59`. Reservations and migrations read them;
/// they are never a G-code word - the setup's post names its offsets
/// (`crate::setup_offsets`).
pub(crate) const LEGACY_WCS_NAMES: [&str; 6] = ["G54", "G55", "G56", "G57", "G58", "G59"];


/// Where the operation's boundary geometry comes from. Names, never
/// entity ids (AGENTS.md: identity by name keeps commands invertible).
#[derive(Clone, Debug, PartialEq)]
pub enum SrcRef {
    /// The setup's stock solid.
    Stock,
    /// Another named entity's geometry.
    Entity(String),
    /// A feature selection: faces and edge chains picked off named
    /// entities, resolved by their geometry-hash ids at generation
    /// (M1.5). An empty list is refused by the command layer.
    Selection(Vec<GeoSel>),
}

/// One selected feature of one named entity. Identity is the feature's
/// geometry hash, not an index or a facet count: re-imports, facet
/// reorder and rigid moves all keep a pick pointing at the same feature
/// (see `geometry::feature`'s stable-id design).
#[derive(Clone, Debug, PartialEq)]
pub struct GeoSel {
    pub entity: String,
    pub feature: FeatureSel,
}

/// What kind of feature was picked; the numbers are [`FaceId`] and
/// [`ChainId`] hashes, never positional.
///
/// [`FaceId`]: crate::geometry::feature::FaceId
/// [`ChainId`]: crate::geometry::feature::ChainId
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum FeatureSel {
    Face(crate::geometry::feature::FaceId),
    Chain(crate::geometry::feature::ChainId),
    /// One logical face of a smooth wall: every planar strip the wall was
    /// tessellated into, welded by their shared `facet_patch` root (see
    /// [`patch_rings`]). Only exists where that weld has more than one
    /// strip - a flat face is already whole, and stays a `Face`.
    ///
    /// [`patch_rings`]: crate::geometry::feature::patch_rings
    Patch(crate::geometry::feature::PatchId),
    /// One feature vertex - a corner where feature edges meet (M13.4),
    /// keyed by its welded position ([`VertexId`]). A point bounds no
    /// region: boundary consumers refuse it by name; heights and the WCS
    /// origin read its position.
    ///
    /// [`VertexId`]: crate::geometry::feature::VertexId
    Vertex(crate::geometry::feature::VertexId),
}

impl FeatureSel {
    /// The raw geometry hash, whatever the kind.
    #[must_use]
    pub fn id(self) -> u64 {
        match self {
            FeatureSel::Face(id) | FeatureSel::Chain(id) | FeatureSel::Patch(id) | FeatureSel::Vertex(id) => id,
        }
    }
}

impl core::fmt::Display for FeatureSel {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            FeatureSel::Face(id) => write!(f, "face {id:#x}"),
            FeatureSel::Chain(id) => write!(f, "edge chain {id:#x}"),
            FeatureSel::Patch(id) => write!(f, "welded face {id:#x}"),
            FeatureSel::Vertex(id) => write!(f, "vertex {id:#x}"),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OpType {
    Face,
    Contour,
    Pocket,
    Slot,
    Trace,
    /// 2D Adaptive roughing (M2.4 A): the pocket's constant-engagement
    /// ladder under its own catalog name — rings spaced by the
    /// engagement law instead of a authored stepover (see
    /// [`crate::machining2d::pocket`]). Declared where the registry
    /// rows it (the table's order IS the declaration order -
    /// `OpType as usize` is the table index everywhere).
    Adaptive2D,
    /// 2D Offset roughing, style A (M2.4 A): concentric rings from the
    /// boundary inward at the roughing stepover — the pocket's
    /// waterline tiling, named (see [`crate::machining2d::pocket`]).
    Offset2D,
    /// Engraving (M2.4 B): the trace's chain-follow at an engraver's
    /// depth under its own catalog name - the twin law again (the
    /// kernel is [`crate::machining2d::trace`], as it was for the
    /// roughing twins and the pocket). The V-bit tool family is
    /// M10.1's tool-law work; until it lands the name says what the
    /// path is, not what the tool must be.
    Engrave,
    /// 2D chamfer (M2.4 B): one ride of a taper tool's CONE lying ON
    /// the authored wall - apex at the top plane, offset solved so the
    /// cone coincides with the chamfer face (see
    /// [`crate::machining2d::chamfer`]). A tool whose cone cannot lie
    /// on the wall is refused by name, never approximated.
    Chamfer2D,
    /// Circular pocket / vault (M2.4 B): concentric rings about an
    /// AUTHORED center and radius - the geometry authored like
    /// [`Self::Slot`]'s endpoints, not picked; flat floors, or the
    /// vault's dome depth law under the same name (see
    /// [`crate::machining2d::circular`]).
    Circular,
    /// Helical thread milling (M2.4 C): 3-axis interpolation to an
    /// internal thread - pitch per turn down, one ISO 724 depth of
    /// radial infeed out, retract back up the ride (see
    /// [`crate::machining2d::thread_mill`]). Declared at the 2D mill's
    /// tail, where the registry rows it. The spindle-sync word a
    /// rigid tap wants rides the M3.3 machine definitions; a machine
    /// that declares no sync refuses the TAP cycle as M1.6
    /// established, and this kind needs nothing of it.
    Thread,
    Drill,
    /// Standalone boring (M2.4 C): a cutter SMALLER than the finished
    /// hole rides its center circle until the rim stands on the wall -
    /// circular interpolation, not the slide. Beside M1.6's boring
    /// cycles under its own catalog name (see
    /// [`crate::machining2d::bore`]); a counterbore-folded hole is
    /// declined for it by name, the cycle's own word.
    Bore,
    /// Mesh-driven Z-level pocketing: the model is the final part, every
    /// level's islands are computed from it (see [`crate::machining3d`]).
    Pocket3D,
    /// Mesh-driven 3D finishing: ball-nose waterline rings at the scallop
    /// stepdown (see [`crate::finishing3d`]).
    Finish3D,
    /// Deburring (M2.5 E B): the cone-ruled edge break in 3D — every
    /// EXPOSED edge of the model (an open mesh's rim, a convex crease)
    /// ridden once with the taper tool's axis standing OVER the edge,
    /// the generatrix making the break (see [`crate::finishing3d`]'s
    /// deburr). The break's width is the one authored number; the tip's
    /// depth follows from the tool's own cone, so no second promise
    /// exists to disagree. Edges nothing exposes answer Empty by name.
    Deburr,
    /// Simultaneous 5-axis level finishing: the waterline ring ladder
    /// with the TOOL AXIS moving per move - lead/lag lean against a
    /// tilt clamp, for shank clearance on steep walls (M5.1 stage C,
    /// see [`crate::multiaxis`]).
    Contour5D,
    /// Simultaneous 5-axis raster: the Z-following parallel pass with
    /// the same axis law (M5.1 stage C).
    Parallel5D,
    /// Ruled-surface (bladed) finishing: the raster run ALONG the
    /// surface's own straight direction, the tool lying with the
    /// flank instead of scalloping against it (M5.1 stage D, see
    /// [`crate::multiaxis`]).
    Swarf,
    /// The swarf family's tolerance ladder: the same path run
    /// coarse-first, finishing at the nominal tolerance (M5.1 stage D).
    AdvancedSwarf,
    /// Rotary 4-axis finishing of a turned work: the waterline ring
    /// ladder about the FITTED axis of revolution, the tool posed
    /// radially (M5.1 stage E, see [`crate::multiaxis::axis`]).
    RotaryContour,
    /// Rotary passes ALONG the fitted axis, stepping around the bar -
    /// the laid-down sibling of [`Self::RotaryContour`] (M5.1 stage E).
    RotaryParallel,
    /// The rotary family's ROUGH ladder (M5.1 stage E2): the pocket
    /// machinery run in wrap space about the fitted axis - the
    /// developed cylinder eroded level by level in radial infeed,
    /// every cut move standing RADIAL on the way back out.
    RotaryOffset,
    /// Multi-axis roughing (M5.1 stage E3): the 3-D pocket ladder,
    /// move for move, with the stage-C stamp read over it - floors
    /// and air keep the spindle vertical, steep flanks take it off
    /// vertical within the tilt cone. The part stands still and the
    /// TOOL AXIS moves; one 3+2 index cannot hold these poses (see
    /// [`crate::multiaxis`]).
    MultiAxisRough,
    /// Flowline finishing (M5.1 tail): the raster's laws with a
    /// direction that turns - each pass follows the surface's own
    /// least-curving direction field, so lines stay WITH a vane or
    /// channel instead of crossing it. Plain ground makes no
    /// directional claim and degrades to the parallel raster; the
    /// steep flanks are still the waterline rings' (see
    /// [`crate::multiaxis`]).
    Flow5D,
    /// Multi-axis contour on drive curves (M5.1 tail, stage J): the
    /// picked edge chains, machined WHERE THEY STAND - no pattern is
    /// asked of the model, the curve is the path. At every vertex the
    /// ball rests on the fan of planes the mesh shows there (the
    /// bisector of an edge, the seat of a corner) and the axis poses
    /// by the same lean law the family uses (see [`crate::multiaxis`]).
    MultiAxisContour,
    /// Turning facing (M5.2 stage A): the work spins, the insert
    /// travels - radial passes at Z stations stepping down the
    /// fitted axis toward the facing plane, the turned profile's
    /// own silhouette as the limit at every station (see
    /// [`crate::turning`]). The first kind of the TURN family: the
    /// family gate is what keeps it off a mill.
    Facing,
    /// Profile roughing (M5.2 stage B): the concentric rung ladder on
    /// the turned silhouette — the nose's ride at distance
    /// nose+stock+j·stepdown, rungs outermost first, each eating a
    /// radial shell down to the leave the finish takes off (see
    /// [`crate::turning`]).
    ProfileRough,
    /// Profile finishing (M5.2 stage B): one ride of the nose along
    /// the offset of the turned silhouette — the bar's own skin
    /// within the chord budget, lead in and slide out radially (see
    /// [`crate::turning`]).
    ProfileFinish,
    /// Grooving roughing (M5.2 stage C): the band ladder on a turned
    /// part — a full-width blade stepping down each center it tiles
    /// across the window, to the floor plus the leave the finish
    /// takes off (see [`crate::turning`]).
    GrooveRough,
    /// Grooving finishing (M5.2 stage C): one full-depth plunge per
    /// center to the band's floor — the model's own surface,
    /// tangentially, at every station the blade stands in.
    GrooveFinish,
    /// Parting (M5.2 stage C): the bar severed at one plane — a
    /// depth ladder at the parting station and the through-axis
    /// rung that lets the scrap go.
    Part,
    /// Threading (M5.2 stage D): the synchronized spindle walking a
    /// helix — a V-form insert's wave ladder in the meridian, one
    /// full turn per pitch of the modeled thread across the window
    /// of turns the helix imposes (see [`crate::turning`]). The
    /// verdict is [`Self::GrooveFinish`]'s meridian scan with the
    /// wave clause: the tool's V, not a boxcar, says what stands
    /// between the grooves.
    Threading,
    /// Chamfer turning (M5.2 stage E): one ride of the nose across
    /// a MODELED cone face — the authored angle is a promise the
    /// model must keep, and the ride chases the skin, not the
    /// number (see [`crate::turning`]). A face the model does not
    /// have is refused at the door by name.
    Chamfering,
    /// Taper turning (M5.2 stage E): the nose's ride of the cone
    /// the two authored diameters state — again a promise over a
    /// MODELED taper, so the pass and the verdict stand on the
    /// same skin. Equal diameters state the straight turn.
    TaperTurning,
    /// Multi-pass turning (M5.2 stage E): the roughing ladder down
    /// to the leave plus a sequence of finishing rides that share
    /// it out — the spring-pass law, each ride standing half the
    /// last one's shell in from it, the last on the profile
    /// itself. The moves are stages B's own, chained.
    MultiPassTurning,
    /// Form turning (M5.2 stage E2): a revolved crown stamped once
    /// per MODELED formed surface — a ground gauge plunges to the
    /// deepest position that cuts nothing the model keeps, and the
    /// crown's own arc says what stands between the band's edges
    /// (see [`crate::turning`]). The depth is not authored: the
    /// model states it.
    FormTurning,
    /// Turning boring (M5.2 stage F): the inside twin of the profile
    /// ride — the bar enters along the hole's axis, a radial
    /// lead-out sets the nose on the wall, and one ascending ride of
    /// the nose-center offset holds the hole to size (see
    /// [`crate::turning`]). The verdict is [`Self::TaperTurning`]'s
    /// envelope, rolled about the centers with the bar's own ground
    /// nose; a window that stands on no open hole is refused at the
    /// door by name.
    BoringTurning,
    /// Probing (#108): face / edge / corner / bore-boss centre finding
    /// with a touch probe (see [`crate::probing`]). Appended - the
    /// declaration order is the stored kind id - and listed in the
    /// Drilling section by display order only.
    Probe,
    /// Plasma profile (#670): the torch's 2D through-cut of picked
    /// contours - the plasma table's family (see [`crate::plasma`]).
    /// Appended: stored kind ids never move.
    PlasmaProfile,
    /// Trochoidal slotting (#220): a channel along a centre line cleared
    /// by engagement-controlled trochoid loops (see
    /// [`crate::machining2d::trochoidal_slot`]). Appended like the
    /// probe - the stored kind id never moves - and listed in the 2D
    /// mill section by display order.
    TrochoidalSlot,
    /// Wire EDM contour (#532): one closed profile cut through the
    /// workpiece by a vertical wire, offset to a stated punch / die
    /// side - the wire EDM's family (see [`crate::wire_edm`]).
    /// Appended: stored kind ids never move.
    WireContour,
    /// Sinker EDM burn to depth (#541): one electrode fed down Z to a
    /// depth read from a stated datum (see [`crate::sinker`]). Appended:
    /// stored kind ids never move.
    SinkerBurn,
    /// Sinker EDM side burn (#545): a lateral electrode move that burns
    /// one wall out by a stated amount (see [`crate::sinker_side`]).
    /// Appended: stored kind ids never move.
    SinkerSideBurn,
}

impl core::fmt::Display for OpType {
    /// The panel label, read from the kind registry (M5.0.2) - the one
    /// list the tree, sheet, combo and ribbon tips all quote.
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(self.info().label)
    }
}

pub(super) fn lead_sane(lead: &Lead) -> bool {
    match lead {
        Lead::None => true,
        Lead::Linear { length } => length.is_finite() && *length >= 0.0,
        Lead::Arc { radius } => radius.is_finite() && *radius > 0.0,
        Lead::Tangential { radius } => radius.is_finite() && *radius > 0.0,
        Lead::Perpendicular { length } => length.is_finite() && *length >= 0.0,
    }
}

/// The digest of everything a generated path depends on. Not a
/// cross-version identifier - the algorithm behind it is `DefaultHasher`,
/// stable within a build, and a spurious mismatch after a toolchain bump
/// only asks for a re-generate, never cuts anything wrong.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GenStamp(pub u64);

/// What the tree and properties panel show beside an operation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OpStatus {
    /// Never generated (or the cache is gone).
    Never,
    /// Cached, and every input still matches the stamp taken at generation.
    Ok,
    /// Cached, but some input moved. The path on screen may not match the
    /// model or tool behind it.
    Stale,
}
