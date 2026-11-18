//! Split from `types.rs` by M5.0.1; structure only.

use crate::drilling::{CycleChoice, Hole};
use crate::finishing3d;
use crate::geo2d::Point2;
use crate::links::Links;
use crate::machining2d::{FaceHoles, Lead, PocketStrategy, RestParams, Side};
use crate::machining3d::Entry3D;

use super::height::Height;
use super::sel::OpType;

/// The 3+2 index of a planar operation (M5.1 stage B): the tool holds
/// one pose for the whole path - commanded by the RS-274 letters A and
/// B (degrees, [`crate::cl::pose`]'s convention, C left to the machine's
/// table spin), with the cut plane anchored at `origin` in world mm.
/// Stored, stamped and posted, never normalized: the letters ARE the
/// claim, and [`Tilt::frame`] is the one reading of them. `None` in a
/// parameter field is no index - the ordinary prismatic op.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Tilt {
    /// 0 preserves the pre-#2133 frame; 1 ignores trigonometric dust.
    pub basis_version: u8,
    pub a_deg: f64,
    pub b_deg: f64,
    pub origin: [f64; 3],
}

impl Tilt {
    /// The frame this index commands under its persisted basis version.
    /// Non-finite angles or an unsupported version return None; the
    /// parameter door refuses these before generation.
    #[must_use]
    pub fn frame(&self) -> Option<crate::cl::TiltedFrame> {
        crate::cl::TiltedFrame::from_angles_versioned(
            crate::geometry::Point3::new(self.origin[0], self.origin[1], self.origin[2]),
            self.a_deg,
            self.b_deg,
            self.basis_version,
        )
    }

    /// By-name refusal behind [`Tilt::is_sane`] (#2133): names the out-of-range
    /// field, gives the canonical bounds (A in -90..90, B in -180..180), and
    /// explains that the face opposite the top is A 0 B 180.
    #[must_use]
    pub fn refusal(&self) -> Option<String> {
        if self.basis_version > 1 {
            return Some("Tilted plane: unsupported frame basis version".into());
        }
        if !self.a_deg.is_finite() || !self.b_deg.is_finite() {
            return Some(
                "Tilted plane (3+2): Tool axis A and Tool axis B must be finite numbers (A -90..90 deg, B -180..180 deg)"
                    .into(),
            );
        }
        if !(-90.0..=90.0).contains(&self.a_deg) {
            return Some(format!(
                "Tilted plane (3+2): Tool axis A ({} deg) must be in -90..90 deg (Tool axis B -180..180 deg) - for the face opposite the top, use Tool axis A 0 and Tool axis B 180",
                self.a_deg
            ));
        }
        if !(-180.0..=180.0).contains(&self.b_deg) {
            return Some(format!(
                "Tilted plane (3+2): Tool axis B ({} deg) must be in -180..180 deg (Tool axis A -90..90 deg) - for the face opposite the top, use Tool axis A 0 and Tool axis B 180",
                self.b_deg
            ));
        }
        if !self.origin.iter().all(|v| v.is_finite() && v.abs() < 1e9) {
            return Some(
                "Tilted plane (3+2): Plane origin X, Y and Z must be finite numbers within 1e9 mm"
                    .into(),
            );
        }
        None
    }

    /// Door sanity (M5.1 stage B): every number finite, the anchor
    /// stably small, and the letters canonical - A in [-90, 90] and B
    /// in [-180, 180] - because [`crate::cl`]’s pose convention maps any OTHER
    /// letter pair is already the axis of one of these, and storing a
    /// non-canonical pose would give the same machine position two
    /// file identities (a stamp that changes for no motion).
    #[must_use]
    pub fn is_sane(&self) -> bool {
        self.refusal().is_none()
    }
}

/// Everything an operation needs beyond its tool and source. The tool
/// diameter is deliberately absent - it lives in the tool definition and
/// is injected at generation, so a tool edit cannot disagree with a
/// parameter field. Heights are [`Height`] expressions (From + offset);
/// a file's pre-reference absolute numbers arrive as `Height::fixed`
/// through the project migration, so their behavior is unchanged.
#[derive(Clone, Debug, PartialEq)]
pub enum OpParams {
    Face {
        stepover: f64,
        clearance_z: Height,
        cut_z: Height,
        /// Staged facing: the first cut level. `None` faces in one level
        /// at `cut_z` - the legacy default every pre-stepdown file
        /// carries. `Some(t)` descends `t` down to `cut_z` by `stepdown`.
        top_z: Option<Height>,
        /// Level spacing; positive when `top_z` is set, ignored when it
        /// is not.
        stepdown: Option<f64>,
        feed: f64,
        plunge_feed: f64,
        climb: bool,
        /// Turning-point blends (M11.1 A): row-end reversals ride one
        /// tangent G2/G3 semicircle where the law proves it legal
        /// (`machining2d::face`); `false` is the sharp serpentine every
        /// pre-M11.1 file holds.
        blend_turns: bool,
        /// How the rows meet a hole in the region (M13.11): lifted over
        /// at the clearance plane (the default - safe for a bore and a
        /// boss alike), cut straight across where the hole is air with
        /// islands lifted, or ignored (`Through`: the M1.3 band
        /// serpentine every earlier file holds). A region with no holes
        /// faces the same under all three.
        holes: FaceHoles,
        /// 3+2 index (M5.1 stage B): the plane this face cuts, tilted;
        /// `None` is the world XY the kind has always meant.
        tilt: Option<Tilt>,
    },
    Contour {
        stock_to_leave: f64,
        clearance_z: Height,
        retract_z: Height,
        top_z: Height,
        bottom_z: Height,
        stepdown: f64,
        feed: f64,
        plunge_feed: f64,
        climb: bool,
        /// Which side of the boundary the cutter rides; see
        /// [`machining2d::Side`]. Auto is answered by the material-fill
        /// check in the selection walk, never stored as a verdict.
        side: Side,
        lead: Lead,
        tolerance: f64,
        tab_count: u32,
        tab_width: f64,
        tab_height: f64,
        /// How a level crossing engages; the family
        /// [`crate::machining3d::Entry3D`] names, plumbed to the wall
        /// ladder in M8.2 A. `Plunge` is every pre-M8.2 file - and
        /// stays the law on the Outside side, where no bounded air
        /// certifies a descent; the other arms speak their refusal in
        /// the path's notes there.
        entry: Entry3D,
        /// Helix centerline radius, mm; read only when `entry` is
        /// Helix, and certified against the bore's wall per level by
        /// the generator's dive room, not by the roughing overlap law.
        helix_radius: f64,
        /// Turning-point blends (M11.1 B): air-side corners (line or
        /// arc on either side) ride one tangent G2/G3 fillet where the law proves
        /// it (`machining2d::fillet_corners`); `false` is the sharp
        /// walk every pre-M11.1 file holds.
        blend_turns: bool,
        /// Level transitions and links (M11.2 C/D, the Lead in / Lead
        /// out choice): the descent between levels, and clearance /
        /// retract / keep-down between levels and between rings. The
        /// pre-M11.2 `level_link` stay-down ramp is level Keep down with
        /// a Ramp transition (`crate::links::Links`).
        links: Links,
        /// 3+2 index; see [`OpParams::Face`]'s field.
        tilt: Option<Tilt>,
    },
    Pocket {
        stepover: f64,
        stock_to_leave: f64,
        clearance_z: Height,
        /// Hop plane between passes of one level; None hops to
        /// clearance - the legacy shape.
        retract_z: Option<Height>,
        top_z: Height,
        bottom_z: Height,
        stepdown: f64,
        feed: f64,
        plunge_feed: f64,
        climb: bool,
        strategy: PocketStrategy,
        /// How the cutter engages each level; the family
        /// [`crate::machining3d::Entry3D`] names, lifted to the planar
        /// pocket's ladder in M5.1 H. `Plunge` is every pre-H file.
        entry: Entry3D,
        /// Helix centerline radius, mm; read only when `entry` is
        /// Helix, validated there against the tool radius at generate.
        helix_radius: f64,
        links: Links,
        /// The `Adaptive` strategy's optimal load (M11.4 B): radial width
        /// of cut as a fraction of the tool diameter (ae/D), the ring
        /// step `optimal_load x D` and the contact angle derived
        /// (`crate::load`); read only when `strategy` is Adaptive,
        /// bounds checked there at generate (M5.1 H made the planar
        /// pocket read the law instead of riding Waterline as an alias).
        optimal_load: f64,
        /// Turning-point blends (M11.1 C): pass corners and the
        /// zigzag's row-to-link jogs ride one tangent G2/G3 fillet
        /// inside the cutter-center area where the law proves it;
        /// `false` is the sharp walk every pre-M11.1 file holds.
        blend_turns: bool,
        /// The corner arc's radius, mm (#1733), read only with
        /// `blend_turns`; `None` is the tool radius every earlier file
        /// blends at.
        blend_radius: Option<f64>,
        /// Rest machining (#1489): cut only what the larger previous tool
        /// described here could not reach (`machining2d::rest_pocket`).
        /// `None` is off - every earlier file, and the plain pocket.
        rest: Option<RestParams>,
        /// 3+2 index; see [`OpParams::Face`]'s field.
        tilt: Option<Tilt>,
    },
    /// 2D Adaptive roughing (M2.4 A): the pocket's constant-engagement
    /// ladder as its own catalog name. The rings are the `Adaptive`
    /// strategy's - the optimal load spaces them, so `stepover` rides
    /// the retype law inert, exactly as it does on a Pocket set to
    /// Adaptive. `machining2d::pocket` does the work; the kind is the
    /// name, the seed, and the golden.
    Adaptive2D {
        stepover: f64,
        stock_to_leave: f64,
        clearance_z: Height,
        /// Hop plane between passes of one level; None hops to
        /// clearance - the pocket family's shape.
        retract_z: Option<Height>,
        top_z: Height,
        bottom_z: Height,
        stepdown: f64,
        feed: f64,
        plunge_feed: f64,
        climb: bool,
        /// How the cutter engages each level; see [`OpParams::Pocket`].
        entry: Entry3D,
        /// Helix centerline radius, mm; read only when `entry` is Helix.
        helix_radius: f64,
        links: Links,
        /// Optimal load, ae/D (M11.4 B): the ring step the ladder keeps;
        /// the law `PocketStrategy::Adaptive` reads. Under the Trochoidal
        /// method it is the most the loops advance a turn.
        optimal_load: f64,
        /// How the region is cleared (#1509): the HSM spiral every 2D
        /// Adaptive cut before the choice existed (the default), or
        /// trochoidal loops along the region's medial line
        /// (`machining2d::adaptive_trochoidal`).
        method: crate::machining2d::AdaptiveMethod,
        /// Trochoidal controls (#1588): read only when `method` is
        /// `Trochoidal` (engagement limit, return mode, return feed,
        /// loop width).
        troch: crate::machining2d::AdaptiveTrochParams,
        /// Arc turns at the spiral's corners (#1733), see
        /// [`OpParams::Pocket`]; `false` is every earlier file. The
        /// Trochoidal method's loops are arcs already: it refuses the
        /// option by name rather than ignore it.
        blend_turns: bool,
        /// The corner arc's radius, mm; `None` is the tool radius.
        blend_radius: Option<f64>,
        /// 3+2 index; see [`OpParams::Face`]'s field.
        tilt: Option<Tilt>,
    },
    /// 2D Offset pocket roughing, style A (M2.4 A): concentric rings
    /// from the boundary inward at the roughing stepover - the pocket's
    /// `Waterline` tiling under a roughing schedule, the same island
    /// and clearance math the staircase uses. No engagement angle: the
    /// stated `stepover` spaces the rings here.
    Offset2D {
        stepover: f64,
        stock_to_leave: f64,
        clearance_z: Height,
        /// Hop plane between passes of one level; None hops to
        /// clearance - the pocket family's shape.
        retract_z: Option<Height>,
        top_z: Height,
        bottom_z: Height,
        stepdown: f64,
        feed: f64,
        plunge_feed: f64,
        climb: bool,
        /// How the cutter engages each level; see [`OpParams::Pocket`].
        entry: Entry3D,
        /// Helix centerline radius, mm; read only when `entry` is Helix.
        helix_radius: f64,
        links: Links,
        /// Arc turns at the rings' corners (#1733), see
        /// [`OpParams::Pocket`]; `false` is every earlier file.
        blend_turns: bool,
        /// The corner arc's radius, mm; `None` is the tool radius.
        blend_radius: Option<f64>,
        /// 3+2 index; see [`OpParams::Face`]'s field.
        tilt: Option<Tilt>,
    },
    /// Engraving (M2.4 B): the trace's numbers under the catalog name -
    /// the twin law again (as `Adaptive2D` is the pocket's). The chain
    /// ride, the stepdown ladder and the heights are Trace's verbatim;
    /// `machining2d::trace` does the work, the kind is the name, the
    /// seed and the equivalence.
    Engrave {
        clearance_z: Height,
        /// Hop plane between chains; None hops to clearance - the
        /// trace's shape.
        retract_z: Option<Height>,
        top_z: Height,
        bottom_z: Height,
        stepdown: f64,
        feed: f64,
        plunge_feed: f64,
        climb: bool,
        /// 3+2 index; see [`OpParams::Face`]'s field.
        tilt: Option<Tilt>,
    },
    /// 2D chamfer (M2.4 B): one ride whose center line stands so the
    /// taper tool's CONE lies on the chamfer face the two authored
    /// numbers state - `width` along the top plane, `angle_deg` off it.
    /// There is no `bottom_z` to author: the face's depth is
    /// `width·tan(angle)`, and a second number for the same line could
    /// only disagree with the first. The cut edge is picked like
    /// [`Self::Trace`]'s chains; `side` says which wall the cone lies
    /// on, exactly as contour's does.
    Chamfer2D {
        /// The chamfer face's width, measured in the top plane.
        width: f64,
        /// The face's slope off the top plane, degrees, in (0, 90).
        angle_deg: f64,
        clearance_z: Height,
        /// Hop plane between chains; None hops to clearance - the
        /// trace's shape.
        retract_z: Option<Height>,
        /// The plane the chamfer starts FROM - the top edge's own Z.
        top_z: Height,
        feed: f64,
        plunge_feed: f64,
        climb: bool,
        /// Which side of the boundary the cone rides; see
        /// [`machining2d::Side`].
        side: Side,
        /// Chord budget of the offset center line, mm - the contour's
        /// arc-fit law.
        tolerance: f64,
        /// 3+2 index; see [`OpParams::Face`]'s field.
        tilt: Option<Tilt>,
    },
    /// Circular pocket / vault (M2.4 B): concentric rings about an
    /// AUTHORED center at an AUTHORED radius - the geometry authored
    /// like [`Self::Slot`]'s endpoints, no pick to disagree with the
    /// circles. Flat floors stand at `bottom_z` like the pocket's;
    /// `vault` instead sinks each ring on the dome law
    /// `bottom_z + (top_z − bottom_z)·(r/R)²` - the parabolic
    /// reflector, every ring a circle the spindle simply holds.
    Circular {
        /// The pattern's center, work coordinates.
        center: Point2,
        /// The disk's radius, mm; > 0 at the door.
        radius: f64,
        /// The dome law instead of the flat floors (variant docs).
        vault: bool,
        stepover: f64,
        stock_to_leave: f64,
        clearance_z: Height,
        /// Hop plane between rings of one level; None hops to
        /// clearance - the pocket family's shape.
        retract_z: Option<Height>,
        top_z: Height,
        bottom_z: Height,
        stepdown: f64,
        feed: f64,
        plunge_feed: f64,
        climb: bool,
        /// How the cutter engages each level; see [`OpParams::Pocket`].
        entry: Entry3D,
        /// Helix centerline radius, mm; read only when `entry` is Helix.
        helix_radius: f64,
        links: Links,
        /// 3+2 index; see [`OpParams::Face`]'s field.
        tilt: Option<Tilt>,
    },
    Slot {
        a: Point2,
        b: Point2,
        width: f64,
        stock_to_leave: f64,
        clearance_z: Height,
        retract_z: Option<Height>,
        top_z: Height,
        bottom_z: Height,
        stepdown: f64,
        stepover: f64,
        feed: f64,
        plunge_feed: f64,
        climb: bool,
        /// How a level crossing engages (M8.2 C); the
        /// [`crate::machining3d::Entry3D`] family the pocket and
        /// contour ladders read. A slot's pass start is a
        /// stadium-boundary point - a wall - so `Plunge` keeps the
        /// everyday shape (and the goldens), while the ramped arms
        /// certify against the closed-form cutter-center stadium.
        entry: Entry3D,
        /// Helix centerline radius, mm; read only when `entry` is
        /// Helix, certified per level against the stadium (a
        /// full-width channel has no room for an orbit and says so).
        helix_radius: f64,
        links: Links,
        /// 3+2 index; see [`OpParams::Face`]'s field.
        tilt: Option<Tilt>,
        /// How far the pass runs past `a`, back along `b -> a`, mm
        /// (#1731). Zero is the authored end. The extended pass answers
        /// to the same gouge laws as the rest of the slot: an extension
        /// into a part the job keeps is a blocking Gouge.
        extend_start: f64,
        /// How far the pass runs past `b`, on along `a -> b`, mm (#1731).
        extend_end: f64,
    },
    /// Trochoidal slotting (#220): the channel of `width` along `a -> b`
    /// cleared by trochoid loops whose engagement the planner measures
    /// (see [`crate::machining2d::TrochoidParams`] for every number).
    TrochoidalSlot {
        a: Point2,
        b: Point2,
        width: f64,
        /// Band the loop centres sweep; `None` is all the channel leaves.
        loop_width: Option<f64>,
        /// Forward step per loop, mm (the most the planner may use).
        step: f64,
        /// Engagement limit, degrees.
        max_engagement: f64,
        ret: crate::machining2d::TrochoidReturn,
        /// Fillet from the wall lead into each loop, mm.
        smoothing: f64,
        stock_to_leave: f64,
        clearance_z: Height,
        retract_z: Option<Height>,
        top_z: Height,
        bottom_z: Height,
        stepdown: f64,
        feed: f64,
        /// Feed on the lifted return.
        return_feed: f64,
        plunge_feed: f64,
        climb: bool,
    },
    Trace {
        clearance_z: Height,
        retract_z: Option<Height>,
        top_z: Height,
        bottom_z: Height,
        stepdown: f64,
        feed: f64,
        plunge_feed: f64,
        climb: bool,
        /// 3+2 index; see [`OpParams::Face`]'s field.
        tilt: Option<Tilt>,
    },
    /// Hole machining. The hole list is params data - recognized from the
    /// model by an action in the panel, or typed - so it is undoable,
    /// visible, and part of the stamp; a model edit that moves the holes
    /// turns the op Stale through the source digest, never by re-running
    /// recognition behind the user's back. `bottom_z` overrides the depth
    /// of every hole at once; `None` (the normal shape) machines each
    /// hole to its own floor, plus the through-hole exit allowance.
    Drill {
        clearance_z: Height,
        /// Hop plane between holes; `None` hops to clearance, like the
        /// pocket family.
        retract_z: Option<Height>,
        /// The cycle's R plane: where the feed move starts in each hole.
        top_z: Height,
        bottom_z: Option<Height>,
        cycle: CycleChoice,
        /// Depth per peck; `0.0` lets the Auto law pick.
        peck: f64,
        /// Seconds at the bottom; posted as the dwell word on the cycles
        /// that honor one.
        dwell: f64,
        feed: f64,
        plunge_feed: f64,
        holes: Vec<Hole>,
        /// Radial holes in the turned solid, plunged by live tooling
        /// from a turret station (M5.2 F2). A document carries one
        /// kind or the other - the mill's Z holes or the turret's
        /// cross holes - and the generator refuses a mix by name.
        cross: Vec<camcraft_motion::drilling::CrossHole>,
        /// How far past a through hole's floor the tip runs (#1557), mm.
        /// `None` is the kind's own default - 1.0 mm, the drill's full diameter clean through - which every file
        /// before the option posted; checked at generation against the
        /// stock bottom, refused by name below it.
        break_through: Option<f64>,
    },
    /// Mesh-driven 3D pocketing. The model is the final part: every
    /// level cuts the footprint minus the model's section strictly above
    /// it - islands are computed, never picked (the level law,
    /// [`crate::machining3d`]). The region is the model's silhouette,
    /// not a section: unlike the 2.5-D kinds, the cut area changes with
    /// Z, so a selection could only disagree with the mesh.
    Pocket3D {
        stepover: f64,
        stock_to_leave: f64,
        clearance_z: Height,
        /// Hop plane between rings of one level; `None` hops to
        /// clearance, like the pocket family.
        retract_z: Option<Height>,
        /// The first level's plane - the stock surface the ladder
        /// descends from.
        top_z: Height,
        /// The last level snaps here: the model's lowest floor.
        bottom_z: Height,
        stepdown: f64,
        feed: f64,
        plunge_feed: f64,
        climb: bool,
        strategy: PocketStrategy,
        /// How the cutter engages each level; see
        /// [`crate::machining3d::Entry3D`].
        entry: Entry3D,
        /// Helix centerline radius, mm; read only when `entry` is
        /// Helix, validated there against the tool radius at generate.
        helix_radius: f64,
        links: Links,
        /// Close the level ladder's staircase with a rest-material
        /// pass aimed at what the dexel sim says still stands
        /// ([`crate::machining3d::pocket3d_with_rest`], M2.1b).
        rest_rough: bool,
        /// The `Adaptive` and `OffsetRings` strategies' optimal load,
        /// ae/D (M11.4 B; the M2.1c engagement law with the angle
        /// derived - `crate::load`): the HSM planner's load on every
        /// level under Adaptive (M11.4's Pocket3D port, the 2D
        /// planner level by level, refused levels cut by rings and
        /// said), the ring step under OffsetRings; bounds checked at
        /// generate.
        optimal_load: f64,
        /// Modulate every planar feed's rate by the engagement the
        /// dexel sim says the move actually meets, median-anchored to
        /// the programmed feed ([`crate::machining3d::optimize_feed`],
        /// M2.1d). Geometry is untouched; only feed words move.
        feed_opt: bool,
        /// Rough by OFFSET SHELLS of the model
        /// ([`crate::machining3d::pocket3d_offset`], M2.5 F): the
        /// level ladder keeps its XY plan and its dive superset law,
        /// but each cut vertex rides the model's own relief between
        /// the planes instead of the plane itself - the staircase's
        /// pyramids come off in the roughing pass, and the rest
        /// ladder (if any) owes the closing half-step less. Off is
        /// the level ladder exactly as it always generated.
        shell: bool,
    },
    /// Ball-nose surface finishing. The model is the finished
    /// surface. `Waterline`: each ring is the ball-center tangent
    /// locus of the forbidden region at its center plane, levels
    /// stepped by the scallop law - `Δz = 2√(r·tol)` - so there is no
    /// stepdown field: the tolerance IS the depth increment's author
    /// (see [`crate::finishing3d`]). `Parallel`/`ParallelMulti`/
    /// `Scallop`: a Z-following raster over the shallow terrain (once
    /// along X, crossed along the axes, crossed along the diagonals),
    /// then the same scallop ladder's rings on the steep flanks - the
    /// 45° auto-split, one jurisdiction line for both halves. Region,
    /// heights and refusals follow the 3-D pocket's laws exactly:
    /// silhouette footprint, resolved top/bottom, mesh-computed
    /// everything.
    Finish3D {
        /// Which kinematic generates the path. Waterline is the
        /// M2.2a default; Parallel adds the raster (M2.2b),
        /// ParallelMulti and Scallop cross it (M2.2c).
        pattern: finishing3d::FinishPattern,
        /// Raster line spacing, mm; `None` (always for Waterline)
        /// follows the stepover cos-law from the tolerance instead -
        /// same "the tolerance is the author" law as the stepdown.
        stepover: Option<f64>,
        /// Surface deviation budget, mm, both directions: it sets the
        /// stepdown, the ring's scan resolution and the arc-fit chord
        /// together, and it is what the envelope verdict measures
        /// against.
        tolerance: f64,
        clearance_z: Height,
        /// Hop plane between rings of one level; `None` hops to
        /// clearance, like the pocket family.
        retract_z: Option<Height>,
        /// Tip-Z of the first (highest) ring.
        top_z: Height,
        /// Tip-Z of the last ring - the model's lowest point the ball
        /// can touch (the generator's levels snap here).
        bottom_z: Height,
        feed: f64,
        plunge_feed: f64,
        climb: bool,
        /// Chain the lines of one family with diagonal feeds instead
        /// of hops where the ground allows (M2.2e). `false` is every
        /// pre-ramp file: hop-to-clearance, unchanged.
        ramp: bool,
        /// Floor for the march's refinement, mm (M2.2e surface
        /// finish). `None` rides the march's own floor - what every
        /// pre-v18 file ran.
        min_step: Option<f64>,
        /// The steepest ramp descent, degrees (M2.2e surface
        /// finish). 45.0 is the law's own gift - shallow ground is
        /// exactly 1:1 - so the default certifies exactly what
        /// v17's chain certified. Inert while `ramp` is false.
        ramp_angle: f64,
        /// Enter the ring's first contact along its contact normal
        /// where the tangent is steep (M8.2 B). `false` is every
        /// pre-0.5 file: the vertical plunge, byte for byte.
        entry_ramp: bool,
        /// The ring ladder's spacing, mm (M2.5 A): the flank's own
        /// number where the Steep-and-shallow partition draws its
        /// border. `None` - and every pattern but SteepShallow, and
        /// every pre-v46 file - rides the scallop law `2√(r·tol)`,
        /// byte for byte, as if this field had never arrived; the
        /// same "a stated number outranks a derived one, the verdict
        /// keeps the promise" grammar `stepover` keeps for the raster.
        steep_stepover: Option<f64>,
        /// The reference ball's diameter, mm (M2.5 B): Pencil keeps
        /// the corner bands THIS tool cannot reach but that bigger
        /// ball leaves standing. `None` says no bigger ball was ever
        /// named - every concave corner is fair game. Must exceed
        /// the op's tool diameter or the pencil refuses by name;
        /// every other pattern ignores it as if it never arrived.
        pencil_reference: Option<f64>,
        /// The land model (#1196): lid every CLOSED opening (hole or
        /// pocket) a flat disk of this diameter, mm, cannot enter, on
        /// the surface the path is PLANNED on - the finish flies over
        /// it at its rim instead of diving down its walls. The verdict
        /// still judges the real model. `None` (every older file) is
        /// the model itself, byte for byte. See
        /// [`crate::finishing3d::fill_openings`] for the rule.
        fill_openings: Option<f64>,
    },
    /// Deburr (M2.5 E B): the cone-ruled edge break in 3D. The ONE
    /// authored number is `width` - the break's width in the edge's
    /// own plane - and the tip's depth follows from the tool's cone:
    /// `drop = width / tan(half-angle)`, stated once in
    /// [`crate::finishing3d::DeburrParams::drop`] so the door, the
    /// pane and the path cannot disagree (chamfer's derived-floor
    /// grammar, read on a real dihedral). No `bottom_z` to author
    /// the ride stops at: the loci are the model's own exposed edges,
    /// and `top_z`/`bottom_z` bound the WINDOW those edges must lie
    /// in - a rim running off the programmed floor is clipped by the
    /// edge's own line, never dropped whole. The hop between runs is
    /// the kernel's own: every run starts and ends at `clearance_z`
    /// (no retract field exists to author, and none is needed).
    Deburr {
        /// Break width at the edge plane, mm; > 0 at the door, and
        /// small enough for the tool's flute is the OTHER door.
        width: f64,
        clearance_z: Height,
        /// Tip ceiling of the ride: edges whose TIP stands above it
        /// are clipped - the window's top, as every finishing op's.
        top_z: Height,
        /// Tip floor of the ride: edges running below it are clipped.
        bottom_z: Height,
        feed: f64,
        plunge_feed: f64,
    },
    /// Simultaneous 5-axis level finishing (M5.1 stage C): the waterline
    /// ring ladder a [`Self::Finish3D`] Waterline cuts, with a tool-axis
    /// pose stamped on every cutting move. The positions are the
    /// ball-center paths - a sphere's envelope is rotation-invariant;
    /// what makes the path 5-axis is where the spindle points: the
    /// contact normal, leaned `lead_deg` forward along the feed and
    /// `lag_deg` sideways across it, folded onto the cone of
    /// `max_tilt_deg` wherever that lean would pass the machine's
    /// reach ([`crate::multiaxis`]'s law). The angles exist to keep the
    /// shank clear of steep walls; the envelope verifier measures what
    /// the ball leaves, and the stage-A leaning band law judges the
    /// steel. Region, heights, levels and refusals follow the finish
    /// op's laws exactly.
    Contour5D {
        /// Surface deviation budget, mm - the same author as the
        /// finish op's: it sets the scallop stepdown, the scan
        /// resolution and the arc-fit chord, and it is what the
        /// envelope verdict measures against.
        tolerance: f64,
        clearance_z: Height,
        /// Hop plane between rings of one level; `None` hops to
        /// clearance, like the pocket family.
        retract_z: Option<Height>,
        /// Tip-Z of the first (highest) ring.
        top_z: Height,
        /// Tip-Z of the last ring.
        bottom_z: Height,
        feed: f64,
        plunge_feed: f64,
        climb: bool,
        /// Forward lean off the contact normal, degrees, in the
        /// direction of travel (the cutting literature's lead angle).
        lead_deg: f64,
        /// Sideways lean off the contact normal, degrees, away from
        /// the uncut wall (the literature's tilt/side-lean angle).
        lag_deg: f64,
        /// The cone half-angle the pose may never leave, degrees -
        /// the machine's tilt reach stated by the operation.
        max_tilt_deg: f64,
        /// Collide-aware orientation (M5.1 stage F): where the
        /// commanded pose would drive the shank into the part, tilt
        /// to the nearest clear angle inside the cone. The cut is the
        /// same path either way - a sphere has no opinion about its
        /// own axis - so this is safety, never shape.
        shank_avoid: bool,
    },
    /// The raster sibling of [`Self::Contour5D`]: the Z-following
    /// parallel pass with the same axis law. No `pattern` field - the
    /// kind IS the choice of kinematic (the two-kinds-not-one-table
    /// decision, RECOMMENDED.md).
    Parallel5D {
        tolerance: f64,
        /// Raster line spacing, mm; `None` follows the stepover
        /// cos-law from the tolerance instead.
        stepover: Option<f64>,
        clearance_z: Height,
        retract_z: Option<Height>,
        top_z: Height,
        bottom_z: Height,
        feed: f64,
        plunge_feed: f64,
        climb: bool,
        lead_deg: f64,
        lag_deg: f64,
        max_tilt_deg: f64,
        /// See [`Self::Contour5D`]: the shank dodges within the cone,
        /// the raster's positions standing still.
        shank_avoid: bool,
    },
    /// Ruled-surface (bladed) finishing (M5.1 stage D): the raster run
    /// ALONG the direction [`crate::multiaxis::ruling`] reads off the
    /// mesh, stepping across it at the same scallop law - and the tool
    /// LYING WITH the flank (axis tangent along the ruling, the
    /// side-machining contact) wherever a facet holds one. Where the
    /// surface says nothing of a ruling - floors, rims, domes between
    /// blades - the [`Self::Parallel5D`] lean law poses the move
    /// instead. A doubly-curved model, or one ruled only along Z,
    /// refuses by name and points at the kind that speaks for it.
    Swarf {
        tolerance: f64,
        /// Spacing ACROSS the ruling, mm; `None` follows the stepover
        /// cos-law from the tolerance instead.
        stepover: Option<f64>,
        clearance_z: Height,
        retract_z: Option<Height>,
        top_z: Height,
        bottom_z: Height,
        feed: f64,
        plunge_feed: f64,
        climb: bool,
        lead_deg: f64,
        lag_deg: f64,
        max_tilt_deg: f64,
    },
    /// The swarf family's tolerance ladder (M5.1 stage D): the same
    /// path run coarse-first - pass i from the top at `tol·4^i`, every
    /// scallop law tolerance-authored, so each pass doubles the
    /// spacing and the stepdown - finishing at the nominal tolerance.
    /// Every pass's centers stand tangent to the MODEL, never to the
    /// previous pass, so a wrong pass count cannot hide a gouge.
    AdvancedSwarf {
        tolerance: f64,
        stepover: Option<f64>,
        clearance_z: Height,
        retract_z: Option<Height>,
        top_z: Height,
        bottom_z: Height,
        feed: f64,
        plunge_feed: f64,
        climb: bool,
        lead_deg: f64,
        lag_deg: f64,
        max_tilt_deg: f64,
        /// Coarse-to-fine passes, 1..4; 1 is the plain ladder alone.
        passes: u32,
    },
    /// Rotary 4-axis finishing of a turned work (M5.1 stage E): the
    /// waterline ring ladder about the axis [`crate::multiaxis::axis`]
    /// reads off the mesh, with the tool posed RADIAL - standing
    /// square to the turned wall, its axis meeting the rotation axis,
    /// the defining posture of rotary work. The heights read as axial
    /// STATIONS, so the part may stand or lean on the table; the lean
    /// trio rotates the pose off the radial along the travel, exactly
    /// as the 5-axis kinds lean off the normal. A mesh with no axis of
    /// revolution refuses by name and points at the 5-Axis kinds; a
    /// part lying flat on its axis points at Rotary Parallel.
    RotaryContour {
        tolerance: f64,
        clearance_z: Height,
        retract_z: Option<Height>,
        top_z: Height,
        bottom_z: Height,
        feed: f64,
        plunge_feed: f64,
        climb: bool,
        /// Lean off the radial pose, degrees along the travel (the
        /// same author as the 5-axis kinds': [`crate::multiaxis`]).
        lead_deg: f64,
        lag_deg: f64,
        max_tilt_deg: f64,
    },
    /// The laid-down sibling of [`Self::RotaryContour`]: generatrix
    /// passes ALONG the fitted axis, stepping around the bar - the
    /// swarf raster in a frame that lays the axis down. On a bar
    /// lying on its side this is the ladder whose rings the contour
    /// kind cannot read; a part standing on its axis points back at
    /// Rotary Contour by name.
    RotaryParallel {
        tolerance: f64,
        /// Spacing around the bar, mm; `None` follows the stepover
        /// cos-law from the tolerance instead.
        stepover: Option<f64>,
        clearance_z: Height,
        retract_z: Option<Height>,
        top_z: Height,
        bottom_z: Height,
        feed: f64,
        plunge_feed: f64,
        climb: bool,
        lead_deg: f64,
        lag_deg: f64,
        max_tilt_deg: f64,
    },
    /// The rotary family's rough ladder (M5.1 stage E2): the pocket
    /// machinery run in WRAP SPACE - the developed skin of the fitted
    /// axis eroded level by level, the ladder peeling rho_max to
    /// rho_min - and mapped back out standing RADIAL. `stepdown` is
    /// the RADIAL infeed, `top_z`/`bottom_z` window the STATIONS along
    /// the axis, and `tolerance` is the verify budget alone: the wrap
    /// kernel chords by its own arc-fit law. There is no retract
    /// because the kernel names its own rapids plane - one infeed and
    /// one tool radius outside the skin - and showing a field that
    /// gets overwritten would lie.
    RotaryOffset {
        tolerance: f64,
        /// Spacing around the bar in developed arc length, mm - a
        /// plain number, not the twins' Option: the ladder has no
        /// scallop cos-law to fall back on.
        stepover: f64,
        stock_to_leave: f64,
        clearance_z: Height,
        top_z: Height,
        bottom_z: Height,
        /// Radial infeed per wrap level, mm.
        stepdown: f64,
        feed: f64,
        plunge_feed: f64,
        climb: bool,
        strategy: PocketStrategy,
        entry: Entry3D,
        helix_radius: f64,
        links: Links,
        /// Optimal load, ae/D (M11.4 B), read under Adaptive.
        optimal_load: f64,
        lead_deg: f64,
        lag_deg: f64,
        max_tilt_deg: f64,
    },
    /// Multi-axis roughing (M5.1 stage E3): the [`Self::Pocket3D`]
    /// ladder, positions bit-identical, with a tool-axis pose stamped
    /// on every move the tangency law finds a surface to lean off -
    /// steep flanks take the spindle off vertical within the cone,
    /// floors and air keep it honest. `tolerance` is the VERIFY
    /// budget alone (the envelope law's, like the wrap ladder's).
    /// Deliberately absent next to the plain pocket: `rest_rough`
    /// and `feed_opt`, whose sim passes the stamp has never been
    /// shown to commute with (deferred - RECOMMENDED.md).
    MultiAxisRough {
        tolerance: f64,
        stepover: f64,
        stock_to_leave: f64,
        clearance_z: Height,
        /// Hop plane between rings of one level; `None` hops to
        /// clearance, like the pocket family.
        retract_z: Option<Height>,
        top_z: Height,
        bottom_z: Height,
        stepdown: f64,
        feed: f64,
        plunge_feed: f64,
        climb: bool,
        strategy: PocketStrategy,
        entry: Entry3D,
        helix_radius: f64,
        links: Links,
        /// Optimal load, ae/D (M11.4 B), read under Adaptive.
        optimal_load: f64,
        lead_deg: f64,
        lag_deg: f64,
        max_tilt_deg: f64,
        /// See [`Self::Contour5D`]: the stamped ladder dodges with its
        /// shank too - and the floors it left unposed take a pose when
        /// (only when) the shank speaks (M5.1 stage F).
        shank_avoid: bool,
    },
    /// Flowline finishing (M5.1 tail): [`Self::Parallel5D`]'s numbers
    /// run on a direction that TURNS - each pass follows the
    /// surface's own least-curving direction (the same curvature fit
    /// the swarf reading uses, per facet instead of one global
    /// ruling), seeded so no ground stands farther from a traced
    /// line than half a stepover. Facets that make no directional
    /// claim - flat shelves, isotropic domes - take the region's
    /// dominant fallback direction, so plain ground flows exactly
    /// like the parallel raster and nothing is ever left uncovered.
    Flow5D {
        tolerance: f64,
        /// Line spacing across the field, mm; `None` follows the
        /// stepover cos-law from the tolerance, the raster's law.
        stepover: Option<f64>,
        clearance_z: Height,
        retract_z: Option<Height>,
        top_z: Height,
        bottom_z: Height,
        feed: f64,
        plunge_feed: f64,
        climb: bool,
        lead_deg: f64,
        lag_deg: f64,
        max_tilt_deg: f64,
        /// See [`Self::Contour5D`]: the dodge, on curved lines.
        shank_avoid: bool,
    },
    /// Multi-axis contour on drive curves (M5.1 tail, stage J): the
    /// edge chains picked off the model in `op.src`, machined WHERE
    /// THEY STAND - no pattern is asked of the model, the curve IS
    /// the path, so there is no stepover and no ladder to author
    /// spacing with. `tolerance` keeps the meaning the other kinds
    /// have NOT needed here: it is the verify budget, the envelope
    /// scan's chord allowance, nothing more (as in
    /// [`Self::RotaryOffset`] and [`Self::MultiAxisRough`]). The ball
    /// still rests on the model's facets - the selection's geometry,
    /// the union of the entities named - while the picked chains
    /// tell it where to go (see [`crate::multiaxis`]).
    MultiAxisContour {
        tolerance: f64,
        clearance_z: Height,
        retract_z: Option<Height>,
        top_z: Height,
        bottom_z: Height,
        feed: f64,
        plunge_feed: f64,
        climb: bool,
        lead_deg: f64,
        lag_deg: f64,
        max_tilt_deg: f64,
        /// See [`Self::Contour5D`]: the dodge, on the chain's own
        /// vertices.
        shank_avoid: bool,
    },
    /// Turning facing (M5.2 stage A): the lathe's answer to
    /// [`Self::Face`]. The stations ride the FITTED axis (same
    /// revolution read as the rotary kinds): `top_z`/`bottom_z`
    /// bound the station ladder along it, `stepdown` is the axial
    /// infeed, and at each station one radial pass runs from clear
    /// of the turned profile straight THROUGH the axis - the sweep
    /// that leaves the flat. No stepover is authored because none
    /// is missed: the pass covers every radius in one line.
    /// `tolerance` is the verify budget alone (the envelope law's,
    /// as in [`Self::RotaryOffset`]) - the kernel itself chords the
    /// profile by that law's own tol/4 promise.
    Facing {
        tolerance: f64,
        clearance_z: Height,
        top_z: Height,
        bottom_z: Height,
        stepdown: f64,
        feed: f64,
        plunge_feed: f64,
    },
    /// Profile roughing (M5.2 stage B): the RADIAL ladder on the
    /// turned silhouette. `top_z`/`bottom_z` bound the station
    /// window the rungs ride, `stepdown` is the shell width one rung
    /// eats (bounded by twice the nose — the same ridge law facing
    /// states axially), and `stock_to_leave` is what the last rung
    /// stands off the skin for the finish to take off. `tolerance`
    /// is both the verify budget and the chord budget the silhouette
    /// and its ride settle at (unlike [`Self::Facing`], this kernel
    /// READS it — the ride is the pattern).
    ProfileRough {
        tolerance: f64,
        clearance_z: Height,
        top_z: Height,
        bottom_z: Height,
        stepdown: f64,
        stock_to_leave: f64,
        feed: f64,
        plunge_feed: f64,
    },
    /// Profile finishing (M5.2 stage B): one ride of the nose along
    /// the offset of the silhouette, between the two heights. No
    /// stepdown exists to author — the whole skin is one pass; the
    /// window IS the ladder. `tolerance` again the kernel's own
    /// chord budget and the verify budget at once.
    ProfileFinish {
        tolerance: f64,
        clearance_z: Height,
        top_z: Height,
        bottom_z: Height,
        feed: f64,
        plunge_feed: f64,
    },
    /// Grooving roughing (M5.2 stage C): the blade window on the
    /// bar. `top_z`/`bottom_z` bound the station range the band
    /// owns, `stepdown` is the radial infeed of each rung, and
    /// `stock_to_leave` the skin above the floor the finish takes
    /// off. `tolerance` is the verify budget and the silhouette's
    /// chord budget at once - the twins' law, now with edges
    /// ([`Self::ProfileRough`]).
    GrooveRough {
        tolerance: f64,
        clearance_z: Height,
        top_z: Height,
        bottom_z: Height,
        stepdown: f64,
        stock_to_leave: f64,
        feed: f64,
        plunge_feed: f64,
    },
    /// Grooving finishing (M5.2 stage C): one full-depth plunge per
    /// center - the floor is the model's own, so no infeed exists
    /// to author and no leave to stand off.
    GrooveFinish {
        tolerance: f64,
        clearance_z: Height,
        top_z: Height,
        bottom_z: Height,
        feed: f64,
        plunge_feed: f64,
    },
    /// Parting (M5.2 stage C): `bottom_z` is the PARTING PLANE -
    /// the station the blade sinks through - and `top_z` names the
    /// bar's top for the panel. The cut itself is the bar's whole
    /// column at the plane: the model, not these numbers, sets the
    /// ladder's depth, and the last rung passes the axis to sever.
    /// The bar feeder (M5.2 G.2): `bar_load` stands fresh bar out
    /// `bar_advance` mm BEFORE the section's moves (0 signals the
    /// act alone - a controller that indexes by its own stops), and
    /// `bar_unload` asks the catcher for the severed part AFTER
    /// them. A program describes ONE part: load the bar it is cut
    /// from, machine it, take the part away. The signals ride the
    /// stamp (they change the POSTED program) but no geometry -
    /// generate, simulate and verify never read them. `bar_transfer`
    /// (M5.3 C) adds the handoff step the feeder pair was the special
    /// case of: after the last cut the parting op makes, the part
    /// moves to the machine's second spindle - a `PART TRANSFER 1 2`
    /// intent on the stream, and the program gate (not the post)
    /// says whether the machine states a second spindle. Same law as
    /// its siblings: a signal, never geometry. `inspect_tol` (M5.3 D)
    /// is the family's measurement ask: > 0 asks the program to
    /// probe the finished part against that deviation budget (mm)
    /// - a `PROBE` intent on the stream - and asks the POST to
    ///   judge it: the program's blade legs compose into the final
    ///   cut and one meridian scan measures it under this bar
    ///   (`operation::probe_problems` - the part is what ALL the
    ///   blades leave, so a rough's promise is only silent once a
    ///   finish answers for it). The verdict as a re-run decision,
    ///   not a report tail. 0 is silence, like every signal's off
    ///   state.
    Part {
        tolerance: f64,
        clearance_z: Height,
        top_z: Height,
        bottom_z: Height,
        stepdown: f64,
        feed: f64,
        plunge_feed: f64,
        bar_load: bool,
        bar_advance: f64,
        bar_unload: bool,
        bar_transfer: bool,
        inspect_tol: f64,
    },
    /// Threading (M5.2 stage D): the helical ladder on a MODELED
    /// thread. `pitch` is that thread's lead - the lattice of V's
    /// stands pitched by it across the window `top_z`/`bottom_z`
    /// bound - `depth` the radial drop from crest to root, and
    /// `infeed` the rung of the ladder that grows one groove from
    /// the blank. The V itself (crest width, flank angle) is TOOL
    /// geometry, the blade's width law again; the feeds are mm/min
    /// with the spindle turning one rev per pitch of the
    /// synchronized pass. `infeed` is also the FIRST pass's depth
    /// when `infeed_plan` ladders by constant area (issue #416), and
    /// `infeed_plan` says how the passes enter (radial, flank,
    /// alternating), how deep each goes, the finish allowance and the
    /// spring passes - its default is the radial constant-depth ladder.
    /// `hand` (#629) is the way the thread winds -
    /// right-hand unless the drawing says LH; the passes run toward
    /// the chuck either way and the hand sets the helix's sense.
    Threading {
        tolerance: f64,
        clearance_z: Height,
        top_z: Height,
        bottom_z: Height,
        pitch: f64,
        depth: f64,
        infeed: f64,
        feed: f64,
        plunge_feed: f64,
        infeed_plan: crate::turning::ThreadInfeed,
        hand: crate::cl::ThreadHand,
    },
    /// Chamfer turning (M5.2 stage E): one ride across the cone
    /// face the window `top_z`/`bottom_z` bounds. `angle_deg` is
    /// the cone's slope off the cylinder - a PROMISE the model
    /// must keep, spent at the door, never a surface to chase:
    /// the ride follows the modeled skin and refuses by name when
    /// the skin stands further off the authored line than
    /// `tolerance`. The direction the face falls comes from the
    /// model, not these numbers.
    Chamfering {
        tolerance: f64,
        clearance_z: Height,
        top_z: Height,
        bottom_z: Height,
        angle_deg: f64,
        feed: f64,
        plunge_feed: f64,
    },
    /// Taper turning (M5.2 stage E): the ride of the cone the two
    /// diameters state - `bottom_dia` at the window's low station,
    /// `top_dia` at its high one. The same promise law as
    /// [`Self::Chamfering`]: the model is the surface the ride
    /// chases and the verdict measures; equal diameters state the
    /// straight turn of a cylinder.
    TaperTurning {
        tolerance: f64,
        clearance_z: Height,
        top_z: Height,
        bottom_z: Height,
        bottom_dia: f64,
        top_dia: f64,
        feed: f64,
        plunge_feed: f64,
    },
    /// Multi-pass turning (M5.2 stage E): stage B's concentric
    /// ladder down to `stock_to_leave`, then `passes` finishing
    /// rides sharing the leave out by the spring-pass midpoint
    /// law - ride k stands a shell `stock * (n - k) / n` off the
    /// skin, so the last ride IS the profile. `passes` is a count
    /// the ladder spends, not a depth: 1 makes the classic rough-
    /// plus-finish pair, and the kernel caps the sequence at 64.
    MultiPassTurning {
        tolerance: f64,
        clearance_z: Height,
        top_z: Height,
        bottom_z: Height,
        stepdown: f64,
        stock_to_leave: f64,
        passes: u32,
        feed: f64,
        plunge_feed: f64,
    },
    /// Form turning (M5.2 stage E2): one plunge of a crown gauge per
    /// MODELED formed surface in the window `top_z`/`bottom_z`
    /// bound. The plunge depth is not authored — it is the deepest
    /// position whose floor stands at or above the material across
    /// the band, gouge-free by construction. The crown's radius and
    /// the band's width are TOOL geometry, the blade's width law
    /// again, read off the mounted gauge; the feeds are mm/min with
    /// the spindle turning steady.
    FormTurning {
        tolerance: f64,
        clearance_z: Height,
        top_z: Height,
        bottom_z: Height,
        feed: f64,
        plunge_feed: f64,
    },
    /// Turning boring (M5.2 stage F): one ascending ride of the
    /// nose offset INSIDE a MODELED hole in the window
    /// `top_z`/`bottom_z` bound - the bar runs in on the axis,
    /// leads out radially at `plunge_feed`, and holds the wall to
    /// size at `feed`, the contour twins' numbers read inward.
    /// The nose radius is TOOL steel (the bar's ground tip, the
    /// field every turning kind carries there); the hole itself is
    /// the model's, and a window with none in it refuses by name.
    BoringTurning {
        tolerance: f64,
        clearance_z: Height,
        top_z: Height,
        bottom_z: Height,
        feed: f64,
        plunge_feed: f64,
    },
    /// Standalone boring (M2.4 C). The hole list is params data, the
    /// drill's own law: recognized by an action, undoable, stamped -
    /// a model edit turns the op Stale through the source digest, it
    /// never re-runs recognition behind the user's back. `bottom_z`
    /// overrides every floor at once; `None` machines each hole to
    /// its own. The ride is circular interpolation, so the wall must
    /// be a circle the cutter can stand inside: a hole no wider than
    /// the tool is refused by name at generate.
    Bore {
        clearance_z: Height,
        retract_z: Option<Height>,
        /// Where the feed move opens in each hole (the cycles' R
        /// plane law).
        top_z: Height,
        bottom_z: Option<Height>,
        /// The rim stands this far short of the model's wall; `0.0`
        /// finishes to nominal.
        stock_to_leave: f64,
        stepdown: f64,
        feed: f64,
        plunge_feed: f64,
        /// CCW on the wall (climb) when true.
        climb: bool,
        /// Sagitta budget for the wall circle's arc fit.
        tolerance: f64,
        holes: Vec<Hole>,
        links: Links,
        /// How far past a through hole's floor the tip runs (#1557), mm.
        /// `None` is the kind's own default - 1.0 mm, as the drill - which every file
        /// before the option posted; checked at generation against the
        /// stock bottom, refused by name below it.
        break_through: Option<f64>,
    },
    /// Helical thread milling (M2.4 C, reworked by #894): internal
    /// threads in the listed holes - or, `external`, on the listed
    /// bosses (#985) - with a thread mill
    /// ([`super::super::thread_op`]). `infeed` is the radial depth per
    /// pass and must stay inside the standard's depth `0.5413 x
    /// pitch`; the last pass lands on the major circle whatever it is.
    Thread {
        clearance_z: Height,
        retract_z: Option<Height>,
        top_z: Height,
        bottom_z: Option<Height>,
        infeed: f64,
        feed: f64,
        plunge_feed: f64,
        /// Climb milling: under a clockwise (M3) spindle the orbit runs
        /// counter-clockwise inside a hole; an M4 spindle flips it (#988).
        climb: bool,
        holes: Vec<Hole>,
        /// The thread's pitch, mm (#894). `None` reads a multi-form
        /// mill's own pitch; a single-form mill needs it stated, inside
        /// its range.
        pitch: Option<f64>,
        /// Right-hand unless the drawing says LH (#894).
        hand: crate::cl::ThreadHand,
        /// External threads (#985): the listed features are BOSSES -
        /// `radius` is each one's major radius, `floor_z` its shoulder -
        /// and the mill orbits beside them, cutting `0.6134 x pitch`
        /// deep ([`super::super::thread_boss`]). `false` is every
        /// thread made before the flag existed: internal, in holes.
        external: bool,
        /// Pass control (#988): the radial schedule (equal or constant
        /// chip area), a finishing pass, spring passes, a fit allowance
        /// and the spindle direction with the cutter's hand
        /// ([`crate::machining2d::ThreadPasses`]). The default is the
        /// #894 ride every older file meant.
        passes: crate::machining2d::ThreadPasses,
        /// How far past a through hole's floor the tip runs (#1557), mm.
        /// `None` is the kind's own default - one pitch, so the thread is whole at the exit - which every file
        /// before the option posted; checked at generation against the
        /// stock bottom, refused by name below it.
        break_through: Option<f64>,
    },
    /// Probing (#108): a spindle touch probe finds a face, an edge, an
    /// outside corner or a bore / boss centre, then sets a work offset
    /// or stops for the operator (see [`crate::probing`]). It cuts
    /// nothing: every move is a protected probe move. `target` is the
    /// expected contact (face), the picked edge point / vertex (edge /
    /// corner) or the circle centre at its rim (bore / boss).
    Probe {
        clearance_z: Height,
        mode: crate::probing::ProbeMode,
        dir: crate::probing::ProbeDir,
        dir_y: crate::probing::ProbeDir,
        target: [f64; 3],
        radius: f64,
        approach: f64,
        overtravel: f64,
        backoff: f64,
        fast_feed: f64,
        slow_feed: f64,
        depth: f64,
        inset: f64,
        /// Some(slot 0..=5): set G54..G59 from the found position;
        /// None: report the deviation and stop for the operator.
        set_wcs: Option<u8>,
        /// The found position may stray this far (mm) from the model's
        /// before the program stops with a message.
        tolerance: f64,
    },
    /// Plasma profile (#670): 2D through-cuts of the picked contours
    /// with a torch - kerf offset, cut direction, leads, the small-hole
    /// rule and inner-before-outer order (see [`crate::plasma`]). The
    /// plasma table's only kind (#669's family seam).
    PlasmaProfile(super::plasma::PlasmaOp),
    /// Wire EDM contour (#532): one closed profile cut by a vertical
    /// wire at the kerf offset of the machine's condition, on a stated
    /// punch / die side, from a stated start hole (see
    /// [`crate::wire_edm`]). The wire EDM's first kind (#290 D1).
    WireContour(super::wire_edm::WireContourOp),
    /// Sinker EDM burn to depth (#541): one electrode fed down Z to a
    /// depth from a stated datum (see [`crate::sinker`]). The sinker
    /// EDM's first kind.
    SinkerBurn(super::sinker::SinkerBurnOp),
    /// Sinker EDM side burn (#545): the burn's plunge, then one lateral
    /// move along an axis that burns the faced wall out by a stated
    /// amount (see [`crate::sinker_side`]).
    SinkerSideBurn(super::sinker::SinkerSideBurnOp),
}

impl OpParams {
    #[must_use]
    pub fn kind(&self) -> OpType {
        match self {
            OpParams::Face { .. } => OpType::Face,
            OpParams::Contour { .. } => OpType::Contour,
            OpParams::Pocket { .. } => OpType::Pocket,
            OpParams::Adaptive2D { .. } => OpType::Adaptive2D,
            OpParams::Offset2D { .. } => OpType::Offset2D,
            OpParams::Engrave { .. } => OpType::Engrave,
            OpParams::Chamfer2D { .. } => OpType::Chamfer2D,
            OpParams::Circular { .. } => OpType::Circular,
            OpParams::Slot { .. } => OpType::Slot,
            OpParams::Trace { .. } => OpType::Trace,
            OpParams::Drill { .. } => OpType::Drill,
            OpParams::Pocket3D { .. } => OpType::Pocket3D,
            OpParams::Finish3D { .. } => OpType::Finish3D,
            OpParams::Deburr { .. } => OpType::Deburr,
            OpParams::Contour5D { .. } => OpType::Contour5D,
            OpParams::Parallel5D { .. } => OpType::Parallel5D,
            OpParams::Swarf { .. } => OpType::Swarf,
            OpParams::AdvancedSwarf { .. } => OpType::AdvancedSwarf,
            OpParams::RotaryContour { .. } => OpType::RotaryContour,
            OpParams::RotaryParallel { .. } => OpType::RotaryParallel,
            OpParams::RotaryOffset { .. } => OpType::RotaryOffset,
            OpParams::MultiAxisRough { .. } => OpType::MultiAxisRough,
            OpParams::Flow5D { .. } => OpType::Flow5D,
            OpParams::MultiAxisContour { .. } => OpType::MultiAxisContour,
            OpParams::Facing { .. } => OpType::Facing,
            OpParams::ProfileRough { .. } => OpType::ProfileRough,
            OpParams::ProfileFinish { .. } => OpType::ProfileFinish,
            OpParams::GrooveRough { .. } => OpType::GrooveRough,
            OpParams::GrooveFinish { .. } => OpType::GrooveFinish,
            OpParams::Part { .. } => OpType::Part,
            OpParams::Threading { .. } => OpType::Threading,
            OpParams::Chamfering { .. } => OpType::Chamfering,
            OpParams::TaperTurning { .. } => OpType::TaperTurning,
            OpParams::MultiPassTurning { .. } => OpType::MultiPassTurning,
            OpParams::FormTurning { .. } => OpType::FormTurning,
            OpParams::BoringTurning { .. } => OpType::BoringTurning,
            OpParams::Bore { .. } => OpType::Bore,
            OpParams::Thread { .. } => OpType::Thread,
            OpParams::Probe { .. } => OpType::Probe,
            OpParams::PlasmaProfile(_) => OpType::PlasmaProfile,
            OpParams::WireContour(_) => OpType::WireContour,
            OpParams::SinkerBurn(_) => OpType::SinkerBurn,
            OpParams::SinkerSideBurn(_) => OpType::SinkerSideBurn,
            OpParams::TrochoidalSlot { .. } => OpType::TrochoidalSlot,
        }
    }

    /// The 3+2 index, if this kind carries one (M5.1 stage B): the
    /// five planar kinds do, the rest answer None - an exhaustive
    /// match, so giving tilt to another kind means answering here
    /// with the compiler witnessing it.
    #[must_use]
    pub fn tilt(&self) -> Option<Tilt> {
        match self {
            OpParams::Face { tilt, .. }
            | OpParams::Contour { tilt, .. }
            | OpParams::Pocket { tilt, .. }
            | OpParams::Adaptive2D { tilt, .. }
            | OpParams::Offset2D { tilt, .. }
            // The M2.4 B trio are planar rides like the five above -
            // the index reads their plane the same way.
            | OpParams::Engrave { tilt, .. }
            | OpParams::Chamfer2D { tilt, .. }
            | OpParams::Circular { tilt, .. }
            | OpParams::Slot { tilt, .. }
            | OpParams::Trace { tilt, .. } => *tilt,
            // The probe touches along the machine's axes (#108); the
            // torch stands vertical over a flat plate (#670); the
            // trochoid carries no index (#220 slice 1).
            OpParams::Probe { .. } | OpParams::PlasmaProfile(_) | OpParams::TrochoidalSlot { .. } => None,
            // The wire stands vertical (#532): a taper is a pose per
            // move (#290 D2, #536), never a 3+2 index.
            OpParams::WireContour(_) => None,
            // The electrode burns straight down Z (#541, #545).
            OpParams::SinkerBurn(_) | OpParams::SinkerSideBurn(_) => None,
            // The 5-axis kinds (M5.1 C) are the third answer, not the
            // second: the PART stands still and the TOOL AXIS moves -
            // a 3+2 index inside them would command the machine to
            // hold a pose the stamp is already writing per move.
            OpParams::Drill { .. }
            | OpParams::Pocket3D { .. }
            | OpParams::Finish3D { .. }
            // The deburr ride poses the cone by the EDGE's own lean,
            // not by a machine index: the axis stands vertical over
            // the edge point and the generatrix does the breaking -
            // a 3+2 index would fight the one fact the ride is (M2.5 E B).
            | OpParams::Deburr { .. }
            | OpParams::Contour5D { .. }
            | OpParams::Parallel5D { .. }
            | OpParams::Swarf { .. }
            | OpParams::AdvancedSwarf { .. }
            // Rotary is the same answer for the same reason (M5.1 E):
            // the axis fit frames the path, and the pose law answers
            // per move - an index would fight both.
            | OpParams::RotaryContour { .. }
            | OpParams::RotaryParallel { .. }
            | OpParams::RotaryOffset { .. }
            // The stamped ladder is that answer too: the poses ARE
            // the orientation, written per move (M5.1 stage E3).
            | OpParams::MultiAxisRough { .. }
            // Flow is the stage-C answer spoken on curved lines, and
            // the drive-curve contour poses the same way - the axis
            // law reads the vertex's own fan of planes per move.
            | OpParams::Flow5D { .. }
            | OpParams::MultiAxisContour { .. }
            // Turning is the rotary answer spoken on a lathe: the WORK
            // spins and the fitted axis frames the path - an index
            // would command a pose the kinematics already own.
            | OpParams::Facing { .. }
            | OpParams::ProfileRough { .. }
            | OpParams::ProfileFinish { .. }
            // The blade kinds are the same kinematic: the work spins,
            // the band sinks radially (M5.2 C).
            | OpParams::GrooveRough { .. }
            | OpParams::GrooveFinish { .. }
            // Threading is the same kinematic spoken on a helix: the
            // spindle itself is the C axis the pass walks (M5.2 D).
            | OpParams::Part { .. }
            | OpParams::Threading { .. }
            // The contour rides are that kinematic across a face:
            // the work spins, the nose chases the modeled skin
            // (M5.2 E).
            | OpParams::Chamfering { .. }
            | OpParams::TaperTurning { .. }
            // The crown gauge is that kinematic sunk straight in:
            // the work spins, the band holds one station still
            // (M5.2 E2).
            | OpParams::MultiPassTurning { .. }
            | OpParams::FormTurning { .. }
            // The boring bar is that kinematic inside a hole: the
            // work spins, the nose walks the wall (M5.2 F).
            | OpParams::BoringTurning { .. }
            // Standalone boring and thread milling ride the hole list
            // in plain world XY - the drilling family's own answer
            // (M2.4 C): a tilted plane has no round hole to find.
            | OpParams::Bore { .. }
            | OpParams::Thread { .. } => None,
        }
    }

    /// The rapids plane, as expressed (every kind has one).
    #[must_use]
    pub fn clearance(&self) -> Height {
        match self {
            OpParams::PlasmaProfile(p) => p.clearance_z,
            // A wire has no rapids plane (#532): it never lifts out of
            // the work; its travel plane is the workpiece's top.
            OpParams::WireContour(p) => p.top_z,
            OpParams::SinkerBurn(p) | OpParams::SinkerSideBurn(super::sinker::SinkerSideBurnOp { burn: p, .. }) => p.clearance_z,
            OpParams::Face { clearance_z, .. }
            | OpParams::Contour { clearance_z, .. }
            | OpParams::Pocket { clearance_z, .. }
            | OpParams::Adaptive2D { clearance_z, .. }
            | OpParams::Offset2D { clearance_z, .. }
            | OpParams::Engrave { clearance_z, .. }
            | OpParams::Chamfer2D { clearance_z, .. }
            | OpParams::Circular { clearance_z, .. }
            | OpParams::Slot { clearance_z, .. }
            | OpParams::TrochoidalSlot { clearance_z, .. }
            | OpParams::Trace { clearance_z, .. }
            | OpParams::Drill { clearance_z, .. }
            | OpParams::Pocket3D { clearance_z, .. }
            | OpParams::Finish3D { clearance_z, .. }
            | OpParams::Deburr { clearance_z, .. }
            | OpParams::Contour5D { clearance_z, .. }
            | OpParams::Parallel5D { clearance_z, .. }
            | OpParams::Swarf { clearance_z, .. }
            | OpParams::AdvancedSwarf { clearance_z, .. }
            | OpParams::RotaryContour { clearance_z, .. }
            | OpParams::RotaryParallel { clearance_z, .. }
            | OpParams::RotaryOffset { clearance_z, .. }
            | OpParams::MultiAxisRough { clearance_z, .. }
            | OpParams::Flow5D { clearance_z, .. }
            | OpParams::MultiAxisContour { clearance_z, .. }
            | OpParams::Facing { clearance_z, .. }
            | OpParams::ProfileRough { clearance_z, .. }
            | OpParams::ProfileFinish { clearance_z, .. }
            | OpParams::GrooveRough { clearance_z, .. }
            | OpParams::GrooveFinish { clearance_z, .. }
            | OpParams::Part { clearance_z, .. }
            | OpParams::Threading { clearance_z, .. }
            | OpParams::Chamfering { clearance_z, .. }
            | OpParams::TaperTurning { clearance_z, .. }
            | OpParams::MultiPassTurning { clearance_z, .. }
            | OpParams::FormTurning { clearance_z, .. }
            | OpParams::BoringTurning { clearance_z, .. }
            | OpParams::Bore { clearance_z, .. }
            | OpParams::Thread { clearance_z, .. }
            | OpParams::Probe { clearance_z, .. } => *clearance_z,
        }
    }

    /// The cutting region's height expression: the section/stadium plane
    /// for the depth-bounded kinds, the skim (or first-stage) plane for
    /// Face. Resolve it for a Z; the viewport draws the region loops
    /// there.
    #[must_use]
    pub fn region_height(&self) -> Height {
        match self {
            // The probe's region plane is its safe plane (#108): it
            // cuts no section.
            OpParams::Probe { clearance_z, .. } => *clearance_z,
            // The plasma contours are drawn on the plate's top (#670).
            OpParams::PlasmaProfile(p) => p.top_z,
            // The wire's profile is drawn on the workpiece's top (#532).
            OpParams::WireContour(p) => p.top_z,
            // The electrode enters at the work top (#541).
            OpParams::SinkerBurn(p) | OpParams::SinkerSideBurn(super::sinker::SinkerSideBurnOp { burn: p, .. }) => p.top_z,
            OpParams::Face { top_z, cut_z, .. } => top_z.unwrap_or(*cut_z),
            OpParams::Contour { top_z, .. }
            | OpParams::Pocket { top_z, .. }
            | OpParams::Adaptive2D { top_z, .. }
            | OpParams::Offset2D { top_z, .. }
            | OpParams::Engrave { top_z, .. }
            | OpParams::Chamfer2D { top_z, .. }
            | OpParams::Circular { top_z, .. }
            | OpParams::Slot { top_z, .. }
            | OpParams::TrochoidalSlot { top_z, .. }
            | OpParams::Trace { top_z, .. }
            | OpParams::Drill { top_z, .. }
            | OpParams::Pocket3D { top_z, .. }
            | OpParams::Finish3D { top_z, .. }
            // The window's top: the highest TIP the ride may stand at.
            | OpParams::Deburr { top_z, .. }
            | OpParams::Contour5D { top_z, .. }
            | OpParams::Parallel5D { top_z, .. }
            | OpParams::Swarf { top_z, .. }
            | OpParams::AdvancedSwarf { top_z, .. }
            | OpParams::RotaryContour { top_z, .. }
            | OpParams::RotaryParallel { top_z, .. }
            | OpParams::RotaryOffset { top_z, .. }
            | OpParams::MultiAxisRough { top_z, .. }
            | OpParams::Flow5D { top_z, .. }
            | OpParams::MultiAxisContour { top_z, .. }
            | OpParams::Facing { top_z, .. }
            // The profile window: the heights bound the length of bar
            // the rungs and the ride walk (M5.2 B).
            | OpParams::ProfileRough { top_z, .. }
            | OpParams::ProfileFinish { top_z, .. }
            // The band window's high end; for parting it is the
            // bar's own top the blade rises from (M5.2 C).
            | OpParams::GrooveRough { top_z, .. }
            | OpParams::GrooveFinish { top_z, .. }
            // The window of turns: the lattice stands between these
            // two heights and nowhere outside them (M5.2 D).
            | OpParams::Part { top_z, .. }
            // The contour window bounds the face the ride walks -
            // the same promise in a narrower band (M5.2 E).
            | OpParams::Threading { top_z, .. }
            | OpParams::Chamfering { top_z, .. }
            | OpParams::TaperTurning { top_z, .. }
            // The form window's high station: the topmost valley's
            // crown stands at or below it (M5.2 E2).
            | OpParams::MultiPassTurning { top_z, .. }
            | OpParams::FormTurning { top_z, .. }
            // The bore window's high station: the wall the ride
            // ends at stands at or below it (M5.2 F).
            | OpParams::BoringTurning { top_z, .. }
            // The holes' entry plane: the region the ride dips under
            // begins where the model's mouths stand (M2.4 C).
            | OpParams::Bore { top_z, .. }
            | OpParams::Thread { top_z, .. } => *top_z,
        }
    }

    /// The same set `stamp_with` feeds through `wh`/`wh_opt` - the two
    /// soup lists must agree, and the compiler enforces it here because
    /// every Height-typed field is visited exactly once (M3.1 C, the
    /// `stale_reason` height questions; a missed field is a silent
    /// blind spot, so keep new Height params in both lists).
    pub fn for_each_height<F: FnMut(Height)>(&self, mut f: F) {
        fn opt<F: FnMut(Height)>(h: &Option<Height>, f: &mut F) {
            if let Some(h) = h {
                f(*h);
            }
        }
        match self {
            OpParams::Probe { clearance_z, .. } => f(*clearance_z),
            OpParams::PlasmaProfile(p) => {
                f(p.clearance_z);
                f(p.top_z);
                f(p.bottom_z);
            }
            OpParams::WireContour(p) => {
                f(p.top_z);
                f(p.bottom_z);
            }
            OpParams::SinkerBurn(p) | OpParams::SinkerSideBurn(super::sinker::SinkerSideBurnOp { burn: p, .. }) => {
                f(p.clearance_z);
                f(p.top_z);
                f(p.floor_z);
            }
            OpParams::Face { clearance_z, cut_z, top_z, .. } => {
                f(*clearance_z);
                f(*cut_z);
                opt(top_z, &mut f);
            }
            OpParams::Contour { clearance_z, retract_z, top_z, bottom_z, .. } => {
                f(*clearance_z);
                f(*retract_z);
                f(*top_z);
                f(*bottom_z);
            }
            OpParams::Pocket { clearance_z, retract_z, top_z, bottom_z, .. }
            | OpParams::TrochoidalSlot { clearance_z, retract_z, top_z, bottom_z, .. } => {
                f(*clearance_z);
                opt(retract_z, &mut f);
                f(*top_z);
                f(*bottom_z);
            }
            // The two roughing twins carry the pocket family's four
            // fields verbatim (M2.4 A) - same ladder, same heights.
            OpParams::Adaptive2D { clearance_z, retract_z, top_z, bottom_z, .. }
            | OpParams::Offset2D { clearance_z, retract_z, top_z, bottom_z, .. } => {
                f(*clearance_z);
                opt(retract_z, &mut f);
                f(*top_z);
                f(*bottom_z);
            }
            OpParams::Slot { clearance_z, retract_z, top_z, bottom_z, .. } => {
                f(*clearance_z);
                opt(retract_z, &mut f);
                f(*top_z);
                f(*bottom_z);
            }
            OpParams::Trace { clearance_z, retract_z, top_z, bottom_z, .. }
            // Engrave is the trace's twin (M2.4 B): the same four.
            | OpParams::Engrave { clearance_z, retract_z, top_z, bottom_z, .. } => {
                f(*clearance_z);
                opt(retract_z, &mut f);
                f(*top_z);
                f(*bottom_z);
            }
            // The chamfer's face DEPTH is derived (variant docs): the
            // top plane is the only height the ride reads; the cone's
            // reach below it belongs to width and angle.
            OpParams::Chamfer2D { clearance_z, retract_z, top_z, .. } => {
                f(*clearance_z);
                opt(retract_z, &mut f);
                f(*top_z);
            }
            // Circular's ladder is the pocket family's four (M2.4 B),
            // vault law or flat floors - both read top and bottom.
            OpParams::Circular { clearance_z, retract_z, top_z, bottom_z, .. } => {
                f(*clearance_z);
                opt(retract_z, &mut f);
                f(*top_z);
                f(*bottom_z);
            }
            // The boring ride and the thread mill carry the drill's
            // four - the hole list's floors are params data, not
            // section heights (M2.4 C).
            OpParams::Drill { clearance_z, retract_z, top_z, bottom_z, .. }
            | OpParams::Bore { clearance_z, retract_z, top_z, bottom_z, .. }
            | OpParams::Thread { clearance_z, retract_z, top_z, bottom_z, .. } => {
                f(*clearance_z);
                opt(retract_z, &mut f);
                f(*top_z);
                opt(bottom_z, &mut f);
            }
            OpParams::Pocket3D { clearance_z, retract_z, top_z, bottom_z, .. } => {
                f(*clearance_z);
                opt(retract_z, &mut f);
                f(*top_z);
                f(*bottom_z);
            }
            OpParams::Finish3D { clearance_z, retract_z, top_z, bottom_z, .. }
            | OpParams::Contour5D { clearance_z, retract_z, top_z, bottom_z, .. }
            | OpParams::Parallel5D { clearance_z, retract_z, top_z, bottom_z, .. }
            | OpParams::Swarf { clearance_z, retract_z, top_z, bottom_z, .. }
            | OpParams::AdvancedSwarf { clearance_z, retract_z, top_z, bottom_z, .. }
            | OpParams::RotaryContour { clearance_z, retract_z, top_z, bottom_z, .. }
            | OpParams::RotaryParallel { clearance_z, retract_z, top_z, bottom_z, .. }
            // The stamped ladder stores the pocket family's four, the
            // hop included (M5.1 stage E3), and so does flow (M5.1 I)
            // and the drive-curve contour (M5.1 J) - the top and
            // bottom are its machining window, the bounds the picked
            // chain's vertices must stand within.
            | OpParams::MultiAxisRough { clearance_z, retract_z, top_z, bottom_z, .. }
            | OpParams::Flow5D { clearance_z, retract_z, top_z, bottom_z, .. }
            | OpParams::MultiAxisContour { clearance_z, retract_z, top_z, bottom_z, .. } => {
                f(*clearance_z);
                opt(retract_z, &mut f);
                f(*top_z);
                f(*bottom_z);
            }
            // The deburr ride visits the trio (M2.5 E B): the kernel
            // hops every run to `clearance_z` itself, so no retract
            // exists to visit - the facing law's shape, on a mill.
            OpParams::Deburr { clearance_z, top_z, bottom_z, .. } => {
                f(*clearance_z);
                f(*top_z);
                f(*bottom_z);
            }
            // The rough ladder carries no retract to visit - the
            // kernel names its own rapids plane (variant docs).
            OpParams::RotaryOffset { clearance_z, top_z, bottom_z, .. } => {
                f(*clearance_z);
                f(*top_z);
                f(*bottom_z);
            }
            // The facing ladder is that shape in the axis frame: the
            // station rapids live one infeed clear of the profile,
            // so no authored retract exists to visit (variant docs).
            OpParams::Facing { clearance_z, top_z, bottom_z, .. }
            // The profile kinds are the same three: the rapids plane
            // names itself off the ride's own reach, no hop authored
            // (variant docs, M5.2 B).
            | OpParams::ProfileRough { clearance_z, top_z, bottom_z, .. }
            | OpParams::ProfileFinish { clearance_z, top_z, bottom_z, .. }
            // The blade kinds state the same trio - for parting the
            // third names the plane the blade sinks through, and it
            // is still a Height to visit (M5.2 C).
            | OpParams::GrooveRough { clearance_z, top_z, bottom_z, .. }
            | OpParams::GrooveFinish { clearance_z, top_z, bottom_z, .. }
            | OpParams::Part { clearance_z, top_z, bottom_z, .. }
            // Threading states the same trio - the rapids plane and
            // the window pair the lattice of turns fills (M5.2 D) -
            // and so do the contour rides: the face stands between
            // the same two stations (M5.2 E).
            | OpParams::Threading { clearance_z, top_z, bottom_z, .. }
            | OpParams::Chamfering { clearance_z, top_z, bottom_z, .. }
            | OpParams::TaperTurning { clearance_z, top_z, bottom_z, .. }
            // The gauge states the same trio - the rapids plane and
            // the window the formed surfaces stand in (M5.2 E2).
            | OpParams::MultiPassTurning { clearance_z, top_z, bottom_z, .. }
            // The bar states the same trio - the rapids plane and
            // the window of wall the ride walks (M5.2 F).
            | OpParams::BoringTurning { clearance_z, top_z, bottom_z, .. }
            | OpParams::FormTurning { clearance_z, top_z, bottom_z, .. } => {
                f(*clearance_z);
                f(*top_z);
                f(*bottom_z);
            }
        }
    }

    // (the world-feeding stamp_with stays in the parent module)
}
