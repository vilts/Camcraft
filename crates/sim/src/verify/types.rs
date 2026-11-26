//! Split from `verify.rs` by M5.0.1; structure only.


use crate::geo2d::Region;
pub use crate::tools::ToolGeom;
/// What the path means, which decides which laws apply.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    /// Pocket/slot: material is removed from inside the region.
    Area,
    /// Contour/face: the path trims the region's boundary.
    Boundary,
    /// Trace: the cutter center runs on the boundary itself; laws that
    /// assume tangency do not apply.
    Engrave,
    /// V-carve (#1443): a cone's tip rides the medial axis of the
    /// region, and each branch of the axis is ridden two times, one
    /// time from each wall. Engrave's stand-downs apply (the cut is a
    /// mark inside the region, proved by the generator), and the
    /// crossing law (a) stands down too: the second ride of a branch
    /// lies on the first within the chord tolerance, so their chords
    /// cross at a shared depth by design. That is the pattern, not a
    /// path defect.
    Carve,
    /// The cone rides (M2.5 E B): a chamfer mill whose GENERATRIX lies
    /// on the wall - the axis runs exactly on the edge's own XY, and
    /// on a convex crease the virtual tip stands deliberately INSIDE
    /// the corner it is making (the material above the tip plane,
    /// within one break width, is the burr: the cut's promise, not a
    /// collision). Law (b)'s tangency reading convicts both facts, so
    /// it stands down - Engrave's words, written from the cone side.
    /// Unlike Engrave the shank law KEEPS its voice: a leaning-edge
    /// ride straddles a wall with a cylinder wider than the cone ever
    /// opens, and the column beside it is real steel the way an
    /// engraver's air never is. The kernel's reach doors (the cone's
    /// congruence, the flute's depth) are this mode's construction
    /// law, measured at the dispatch.
    Cone,
    /// Ball-rolling 3-D finish (waterline): the centers sit exactly r
    /// off the model at each tangent point and nearer than r to the
    /// section silhouettes elsewhere, so the tangency gouge law would
    /// flag every ring. It stands down in favor of the measured sweep
    /// envelope below, gated by `finish_tol`.
    Envelope,
    /// The turned family (M5.2): the work spins, the tool traverses
    /// the meridian. The XY-plane laws below read a lathe's chords
    /// as degenerate - radial plunges "cross" at the axis, tangency
    /// fills the section - so (a) and (b) stand down in favor of the
    /// turning branch's meridian scan (motion's `turning::revolve`),
    /// which judges the revolved cut volume exactly. The reach laws
    /// stay: they are pose-independent.
    Revolve,
    /// The radial drill (M5.2 F2): a live tool plunges HORIZONTALLY
    /// into the turned solid's wall. The XY laws read that chain
    /// through the mill's vertical-axis eye: a plunge chord crossing
    /// the silhouette head-on is the hole itself (b convicts it), two
    /// holes drilled through one another at shared height are a
    /// cross-drilled intersection, not a collision (a convicts it),
    /// and the leaning-column enclosure of a fully horizontal shank
    /// is a vertical slab as wide as the band is long (c/c3 convict
    /// the part's own walls), and a height field cannot SEE a
    /// horizontal bore - the retract sliding out along its own kerf
    /// stands under solid stock by the column's reading, so (d)'s
    /// column convicts it too. (a)-(c3) stand down in favor of the
    /// kernel's construction law - doors measured on the mesh, exits
    /// along the hole's own axis - exactly the promise Revolve's
    /// meridian scan keeps for the turned family; (d) reads the
    /// rapids as a solid instead (#79, `rapids_radial`): the posed
    /// flute body against the stock prism minus the earlier kerf, so
    /// the traverse promise is proved at the post gate. The fixture laws
    /// stay: they are 3-D boxes, no height field in them, and steel
    /// keeps no tangency courtesies.
    Radial,
    /// The rotary kinds (M13): a ball-nose ladder posed RADIALLY about
    /// a fitted axis - the tool lies down beside a standing bar, or
    /// hangs under a laid one, and every height-field law below reads
    /// that through a vertical column: rest material "cleared" above a
    /// horizontal ball (it clears the RADIAL column, not the vertical
    /// one), a horizontal shank band enclosed as a column standing up
    /// through the part, a lateral rapid round the bar read as travel
    /// under stock it never enters. Laws (a)-(d) and (c3) stand down;
    /// the envelope (f) measures the ball exactly as `Envelope` does
    /// (posed centers, never a +Z lift), and the band and rapid
    /// questions belong to the radial field law of [`super::rotary`],
    /// which reads rest material in the frame the work turns in. The
    /// fixture laws stay: boxes, no height field.
    Rotary,
    /// The undercut cutters (#1444): a head wider than its neck cuts
    /// SIDEWAYS under a roof, so it clears no column - the stock above
    /// the head still stands. The column laws would read every pass as
    /// a cleared kerf (and acquit a lift straight through the roof), so
    /// the rest model takes no cut from this path; the crossing law (a)
    /// and the region tangency (b) stand down (passes retrace by design,
    /// and a 2-D region cannot say an undercut); the solid law (b2)
    /// judges head and neck against the mesh with the cutter's real
    /// profile; and the band law (c) hands over to the undercut law
    /// ([`super::undercut`]): no Z move with the head in or under
    /// material, no neck or holder band through material above the
    /// head. Rapids (d) and the fixture laws stay.
    Undercut,
}

#[derive(Clone, Debug, PartialEq)]
pub struct VerifyContext<'a> {
    pub region: &'a Region,
    /// The stock prism footprint; pass it when part islands must be
    /// checked too - otherwise the region stands in for it.
    pub stock: Option<&'a Region>,
    /// The mesh a model-driven op cuts to (M2.1's 3-D pocket): the cut
    /// target changes with depth, so the static region cannot be the
    /// whole law. Given, the gouge law checks each move against the
    /// model's cross-section strictly above that move's deepest plane
    /// (the level law's own islands), and the reach laws sample those
    /// sections' walls as the islands they are. Every 2.5-D op passes
    /// `None` - its laws are byte-identical to what they always were.
    pub model: Option<&'a [crate::geometry::Triangle]>,
    pub tool: &'a ToolGeom,
    /// The real tip of `tool` (M13.7): flat, ball, bull corner or point
    /// cone, read by the solid law (b2). `None` judges with the square
    /// stack of `tool` - never smaller than any real tip, so it can
    /// only over-report. A cutter whose stack disagrees with `tool`
    /// (radius, flute, shank or holder) is not trusted: the square
    /// stack stands in ([`super::probe::Cutter::judged`]).
    pub cutter: Option<super::probe::Cutter>,
    pub mode: Mode,
    /// Top of the stock column in Z (the surface the tool starts from).
    pub stock_top_z: f64,
    /// The op's safe plane. Law (d) no longer stands down at it (issue
    /// #2121: a plane inside the stock is not safe) - it reads every
    /// lateral rapid under the stock top; the plane only says which
    /// convictions the operation layer may re-read against the program
    /// ahead (`acquit_traverses_by_prior`).
    pub clearance_z: f64,
    /// Finish allowance already baked into the path offsets.
    pub stock_to_leave: f64,
    /// Radial tolerance for the gouge law.
    pub gouge_tol: f64,
    /// Margin for "below the clearance plane" and "moved in XY".
    pub z_tol: f64,
    /// The deviation budget the measured envelope of `Mode::Envelope`
    /// is judged against. `None` everywhere the envelope law never
    /// runs - which keeps every existing op's verdict byte-identical,
    /// the same discipline as `model`.
    pub finish_tol: Option<f64>,
    /// Workholding hardware, in world space (M3.2 stage B). Fixtures
    /// are NOT cut material and never gouge-relevant; they participate
    /// in exactly two laws: (1) the shank/holder band-reach laws, where
    /// a vise jaw in the band's path is a crash just as uncut material
    /// in it is, and (2) the rapid law, where a rapid whose path dips
    /// into a fixture box is a crash - steel is never kerf. With the
    /// slice empty every law is byte-identical to what it always was,
    /// the same `None` discipline as `model`.
    pub fixtures: &'a [crate::fixture::FixtureBox],
    /// The spindle-housing band the machine file derives (M3.2 stage
    /// B), as `(radius, length)` standing on top of the holder - the
    /// offset comes from the tool stack, so the caller cannot get out
    /// of agreement with it. `None` when the `.mch` states no body
    /// geometry - the unstated-limit law - which keeps every
    /// pre-stage-B verdict byte-identical. It joins the fixture band
    /// law only: against stock the housing still stands down by the
    /// same kerf reasoning as the holder band.
    pub spindle: Option<(f64, f64)>,
    /// How much coarser than the importer's promise the judged model's
    /// mesh is, mm (#864): the chord error the import actually kept less
    /// the cap it promises. A facet of a curved wall stands inside the
    /// true surface by up to its chord error, so on a convex wall a cut
    /// that deep into the real part reads as clean against the mesh; the
    /// gouge laws (b), (b2) and (f) give that much of their tolerance
    /// back ([`VerifyContext::tightened`]), and - #904 - the collision
    /// laws read it too: the reach law (c) and its sweep (c3) grow their
    /// bands by it ([`VerifyContext::band_growth`]) and shrink the kerf
    /// that excuses the flutes' own cut, and the rapid law (d) and its
    /// posed reading shrink the slack of the footprint they march.
    /// `0.0` - every model that met
    /// the promise, and every context that never names a model - reads
    /// each law exactly as it always did. Kept apart from `gouge_tol`,
    /// which is also the budget moves are read into chords at and must
    /// not shrink with the model.
    pub model_excess: f64,
    /// Every reduced source was made coarse only by its file tolerance.
    /// False, including an absent record, keeps the strict law.
    pub file_tolerance: bool,
    /// The declared circles `(centre, radius)` a containment kind's
    /// region draws as rim loops (#1057): our own polygons of a hole the
    /// operation names, not a model's facets, so law (b) reads a loop
    /// that traces one at the untightened `gouge_tol`
    /// ([`VerifyContext::loop_tol`]). Empty everywhere else.
    pub rims: &'a [(crate::geo2d::Point2, f64)],
    /// Geometric upper bound of the finished hole walls (#3451).
    /// Only the axial cone boundary check reads this cap. `None` keeps
    /// the stock-top cap; the rapid and collision laws always use stock.
    pub rim_top_z: Option<f64>,
    /// The spindle a turning nose path cuts about (#1408): given, the
    /// envelope law (f) reads the hidden half as the turning work sees
    /// it (`stock_sim::envelope_scan_turned`). `None` - every mill
    /// path, and a turning one no axis fits - reads as it always did.
    pub lathe_axis: Option<crate::stock_sim::LatheAxis>,
    /// Declared corner relief zones (#2400) for inside-corner pockets and
    /// slots, bounded strictly to cutter radius, pocket floor Z, and stock.
    pub corner_reliefs: &'a [super::CornerReliefZone],
    /// The stepdown the plunge depth law (d3, #3679) judges a feed
    /// descent against: the deepest engagement below the floor that the
    /// cuts before the plunge left under the full tool disc. `None` -
    /// every kind that cuts no levels (drills, bores, threads, turning,
    /// wire, plasma) and every context that states no limit - keeps the
    /// law silent and every stored verdict byte-identical, the same
    /// discipline as `finish_tol` and `model`.
    pub plunge_stepdown: Option<f64>,
}

/// The inert tool of [`VerifyContext::default`].
static NO_TOOL: ToolGeom = ToolGeom {
    radius: 0.0,
    nose_radius: 0.0,
    flute_len: 0.0,
    shank_radius: 0.0,
    shank_len: 0.0,
    holder_radius: 0.0,
    holder_len: 0.0,
};
/// The empty region of [`VerifyContext::default`].
static NO_REGION: std::sync::LazyLock<Region> = std::sync::LazyLock::new(Region::default);

/// The base a context is written over (`..Default::default()`), so a new
/// field with an inert reading (#864's `model_excess`) is not spelled out
/// at every construction site. On its own it judges nothing.
impl Default for VerifyContext<'_> {
    fn default() -> Self {
        VerifyContext {
            region: &NO_REGION,
            stock: None,
            model: None,
            tool: &NO_TOOL,
            cutter: None,
            mode: Mode::Area,
            stock_top_z: 0.0,
            clearance_z: 0.0,
            stock_to_leave: 0.0,
            gouge_tol: 0.0,
            z_tol: 0.0,
            finish_tol: None,
            fixtures: &[],
            spindle: None,
            model_excess: 0.0,
            file_tolerance: false,
            rims: &[],
            rim_top_z: None,
            lathe_axis: None,
            corner_reliefs: &[],
            plunge_stepdown: None,
        }
    }
}

/// Which machine element struck a fixture (M3.2 stage B). Serde on it
/// for the same reason as on [`Warning`]: the pair is persisted with the
/// cached path, so the names are a file-format contract.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FixturePart {
    /// The shank band of the tool assembly.
    Shank,
    /// The holder band.
    Holder,
    /// The spindle housing band the machine file's `spindle` geometry
    /// adds above the holder - present only when the `.mch` states both
    /// body diameter and length (the unstated-limit law again).
    Spindle,
    /// The tool body on a rapid move.
    Rapid,
    /// The flute column itself, on a cutting move whose bands cleared:
    /// the edge milling into clamping steel - the worst thing this law
    /// can report, and named as itself, never as a "shank".
    Flute,
}

/// What fell back to the Offset rings in a [`Warning::StrategyFallback`]
/// (#3542): the text names the operation's own choice. Absent in a cache
/// saved before #3542: read as the Adaptive plan, the only cause then.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FallbackCause {
    /// The Adaptive plan could not cut an area (#3035).
    #[default]
    Adaptive,
    /// The selected "Pass starts at" side would leave material uncut, so
    /// the area is cut in the Auto order (the completeness guard, #3316).
    PassStart,
}

/// Serde derives: the operation layer persists warnings with the cached
/// path (project v3 `OperationDto.cache`), so the enum is also a file
/// format - variant names are that contract, keep them stable.
/// (`Eq` is deliberately absent - the envelope variant carries
/// measurements, and nothing in this codebase needs warnings to be
/// hashable; `PartialEq` is all the dedup and assert sites use.)
#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Warning {
    /// Feed moves `moves[0]` and `moves[1]` cross in XY at shared depth.
    Crossing { moves: [usize; 2] },
    /// Move `move`'s cutter center is inside the finish surface.
    Gouge { at: usize },
    /// Move `move` carries shank/holder through material; `part` marks
    /// whether the offended material is the part/an island (never to be
    /// cut) rather than stock the op simply has not cleared yet.
    ShankHit { at: usize, part: bool },
    /// The assembly swept through material DURING a turn (M5.1 stage
    /// G): the move commands a new pose, and somewhere in the swing -
    /// with the table partway along and the head partway round, in
    /// either family's order of arrival - a band meets steel, though
    /// both endpoint poses pass law (c) clean. Advisory, one per
    /// move, at most: moves law (c) already convicted are not
    /// re-tried here, and `part` reads like (c)'s.
    SweepHit { at: usize, part: bool },
    /// Rapid `move` travels through stock: under the clearance plane,
    /// or (#2121) at it under the stock top.
    RapidInStock { at: usize },
    /// The tool assembly meets the finished part on rapid move `at` (#3520).
    RapidInPart { at: usize },
    /// #2901: the path starts at `height_mm`, under the `top_mm` of the
    /// start stock or of a fixture (`fixture`). The first traverse comes
    /// from an X/Y the machine does not know, so it can cross that top
    /// anywhere ([`super::first_traverse_low`]). The law gives the two
    /// heights in the path's frame; the operation layer gives them in
    /// the frame the user sets the heights in (#2991). `tilted`: a 3+2
    /// plane, whose heights are measured along its tool axis - the text
    /// then says so and names the raise, not a Z value. Absent in a
    /// cache saved before #2991: read as upright.
    FirstTraverseLow {
        at: usize,
        height_mm: f64,
        top_mm: f64,
        fixture: bool,
        #[serde(default)]
        tilted: bool,
    },
    /// #3081: a 3+2 plane whose tool axis points down toward the machine
    /// table (a head tilt past 90 degrees). The path starts at `height_mm`
    /// along that axis; the move to it from the tool-change height (the Z
    /// travel top) can end under the table top. A HIGHER plane along the
    /// axis stands LOWER in the world, so the repair is a plane at
    /// `limit_mm` or under it - never a raise. The operation layer gives
    /// the two heights in the frame the user sets them in (#3148).
    FirstTraverseUnderTable { at: usize, height_mm: f64, limit_mm: f64 },
    /// Move `move` carries a modelled lathe holder body (#369) through
    /// material standing about the spindle: `part` as ShankHit reads
    /// it, `short_mm` the clearance shortfall - the radial depth the
    /// body reaches into that material at its worst instant.
    HolderHit { at: usize, part: bool, short_mm: f64 },
    /// Move `move` runs the named `fixture_part` of the machine through
    /// a fixture (M3.2 stage B): a band collision on a cutting move, or
    /// the tool body itself on a rapid. At most one per move - the
    /// first, worst band.
    FixtureHit {
        at: usize,
        fixture_part: FixturePart,
    },
    /// Defensive: a non-finite coordinate, or a tool axis that is
    /// non-finite or of zero length (#1312; generators gate both out).
    NonFinite { at: usize },
    /// A 3-D finish's measured ball envelope misses the model:
    /// `gouge_mm` below it (worst), `rest_mm` above it (worst),
    /// reported at the cutting move nearest the deviation's XY.
    Envelope {
        at: usize,
        gouge_mm: f64,
        rest_mm: f64,
    },
    /// The envelope could not judge rest. For ball scans the hidden-rest
    /// scan stopped at its budget (`cells` in the lattice, `claims` plane
    /// claims binned), so the reported rest covers only the ridge law.
    /// Zero cells and claims mark the bull-nose sweep, which measures no
    /// rest. Reported at the first cutting move; holds the post.
    RestUnjudged { at: usize, cells: usize, claims: usize },
    /// A 3-D finish's spacing left a scallop past tolerance (M13.9,
    /// the ridge flank law): `mm` standing outside every ball at a
    /// point the pass flanked, against the `gate_mm` the scan holds
    /// (tolerance plus its lattice floor), at the cutting move nearest
    /// it. A quality defect, not a gouge or a collision (RECOMMENDED
    /// "M13.8 B" (3)): shown and named everywhere a warning is, and
    /// the one kind that never blocks the post.
    Scallop { at: usize, mm: f64, gate_mm: f64 },
    /// A probe op's search limit (#570): the seek at move `at` may run
    /// `mm` past the expected contact before the control alarms, more
    /// than the `limit_mm` a touch probe's stylus is trusted to take. A
    /// missing or misplaced feature then drives the stylus that far into
    /// its overtravel. Advisory like the scallop: shown and named, never
    /// binding - a stated probe distance is the operator's to choose.
    ProbeSearch { at: usize, mm: f64, limit_mm: f64 },
    /// A sinker finish out of order (#543): the finish burn at move `at`
    /// is undersized `finish_mm` per side, not less than the `rough_mm`
    /// of the rough burn it follows - the finish electrode is no larger
    /// than the rough, so it takes nothing off the walls the rough left.
    /// Advisory: the finish's own size law already holds the cavity to
    /// size, so the post is not held.
    SinkerFinishOrder { at: usize, finish_mm: f64, rough_mm: f64 },
    /// A stock-source 2D Chamfer whose solid-law check breaks into a
    /// part (#1637): choosing the part as source instead of the stock
    /// avoids cutting into the part edge. Blocking like `Gouge`.
    StockChamferGouge { at: usize },
    /// Move `at` violates a protected "Do not touch" face or edge (#2055):
    /// `item` is its 1-based list index, `part` which section of the tool
    /// assembly came inside clearance, and `short_mm` the shortfall.
    /// Always blocks posting.
    AvoidHit {
        at: usize,
        item: u32,
        part: FixturePart,
        short_mm: f64,
    },
    /// The same for a stock-source Deburr (#2313): it breaks the stock's
    /// edge, and the part under that edge is cut. Blocking like `Gouge`.
    StockDeburrGouge { at: usize },
    /// A hole wider than its drill (#2496): the hole at move `at` is
    /// modelled `hole_mm` across, and the Drill-kind tool drills it at
    /// its own `drill_mm`, so the finished hole is undersize. Advisory: a
    /// pilot drill is a valid plan, so the post is not held, but each such
    /// hole is named.
    DrillUndersize { at: usize, hole_mm: f64, drill_mm: f64 },
    /// An undercut cutter moves in Z at move `at` while material stands
    /// in or over its head's footprint (#1444): a lift there drives the
    /// head into the roof of its own undercut, a dive drives it into
    /// uncut stock it cannot plunge. Blocking like `Gouge`.
    UndercutLift { at: usize },
    /// An Adaptive op whose planner gave `areas` areas (a pocket, a 3-D
    /// level) to the Offset rings (#3035), from the first cut at `at`.
    /// The notes name each one. Advisory: the rings are verified like any
    /// path, so the post is not held - but the strategy the user asked for
    /// did not cut those areas, and the op says so. `cause` (#3542): the
    /// Adaptive plan, or a "Pass starts at" side cut in the Auto order.
    StrategyFallback {
        at: usize,
        areas: usize,
        #[serde(default)]
        cause: FallbackCause,
    },
    /// #3525: the operation cuts `depth_mm` below the stock top, deeper
    /// than the tool's `flute_mm` flute length, from the cutting move
    /// `at` on ([`super::flute_reach`]). Always advisory (#3525
    /// decision): it names the cause of the shank and holder findings that
    /// follow it, and those findings hold the post.
    FluteShort { at: usize, flute_mm: f64, depth_mm: f64 },
    /// #3679: the feed plunge at move `at` reaches `mm` below the floor
    /// that the cuts before it left under the full tool disc, deeper
    /// than the stated stepdown `limit_mm` - the rim stands on stock
    /// that no level above cut, and the cutter takes that much material
    /// in one bite. Always advisory (the #3525 pattern): nothing gouges
    /// and nothing collides, the risk is the tool load, so the finding
    /// is shown and named everywhere a warning is and the post is not
    /// held.
    PlungeTooDeep { at: usize, mm: f64, limit_mm: f64 },
    /// #3748: a start-side pass ("Pass starts at" Inside, or Outside when
    /// `inside` is false) round a part that the user set to the `entry`
    /// lead-in, where Arc from the air is the recommended one. The entry
    /// is used as selected, from the first cut at `at`. Advisory: the
    /// path is verified like any other, so the post is not held.
    EntryNotRecommended { at: usize, entry: super::AdvisedEntry, inside: bool },
    /// #3884: a start-side pass where the selected entry did not fit at
    /// `entries` pass starts, from the first cut at `at`: a named stand-in
    /// entry (a ramp, a helix, an axial dive or a plunge) was used there.
    /// Advisory: the path is verified, so the post is not held.
    EntryStandIn { at: usize, entries: usize },
    /// #3884: a start-side pass whose selected Plunge plunges into uncut
    /// stock at a pass start that no earlier cut cleared, from the first
    /// cut at `at`. Advisory: the entry is the user's and is verified.
    PlungeIntoStock { at: usize },
}

impl Warning {
    /// Whether this warning holds the post: every kind but the advisory
    /// readings - the scallop (M13.9), the probe search, the sinker
    /// order, the undersize drill, the strategy fallback, the flute
    /// reach (#3525) and the plunge depth (#3679) - which name a risk
    /// the collision laws alone decide.
    #[must_use]
    pub fn blocks_post(&self) -> bool {
        !matches!(self, Warning::Scallop { .. }
                | Warning::ProbeSearch { .. }
                | Warning::SinkerFinishOrder { .. }
                | Warning::DrillUndersize { .. }
                | Warning::StrategyFallback { .. }
                | Warning::FluteShort { .. }
                | Warning::PlungeTooDeep { .. }
                | Warning::EntryNotRecommended { .. }
                | Warning::EntryStandIn { .. }
                | Warning::PlungeIntoStock { .. })
    }

    /// The envelope report's unjudged hidden half, as the warning that
    /// says so (M13.1): `Some(RestUnjudged)` exactly when the scan ran
    /// out of budget, located at `at` (the path's first cut).
    #[must_use]
    pub fn hidden_unjudged(rep: &crate::stock_sim::EnvelopeReport, at: usize) -> Option<Self> {
        match rep.hidden {
            crate::stock_sim::HiddenRest::OverBudget { cells, claims } => {
                Some(Warning::RestUnjudged { at, cells, claims })
            }
            crate::stock_sim::HiddenRest::Judged => None,
        }
    }
}

impl core::fmt::Display for Warning {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Warning::Crossing { moves } => write!(
                f,
                "feed moves {} and {} cross in XY at shared depth",
                moves[0], moves[1]
            ),
            Warning::Gouge { at } => {
                write!(f, "move {at} cuts past the finish surface")
            }
            Warning::StockChamferGouge { at } => {
                write!(
                    f,
                    "move {at} cuts past the finish surface (stock-source chamfer breaks into a part; chamfer with the part as source)"
                )
            }
            Warning::StockDeburrGouge { at } => {
                write!(f, "move {at} cuts past the finish surface (stock-source deburr breaks into a part; use the part as source)")
            }
            Warning::HolderHit { at, part, short_mm } => write!(
                f,
                "holder body at move {at} rubs the {} - {short_mm:.3} mm short of clearance",
                if *part { "part" } else { "uncut stock" }
            ),
            Warning::ShankHit { at, part } => write!(
                f,
                "shank/holder at move {at} collides with the {}",
                if *part { "part or an island" } else { "uncut stock" }
            ),
            Warning::SweepHit { at, part } => write!(
                f,
                "shank swept through the {} mid-turn at move {at}",
                if *part { "part or an island" } else { "uncut stock" }
            ),
            Warning::RapidInStock { at } => {
                write!(f, "rapid move {at} travels through stock (below the clearance plane, or at it below the stock top)")
            }
            Warning::RapidInPart { at } => {
                write!(f, "tool meets the part on rapid move {at}")
            }
            Warning::FirstTraverseLow { at, height_mm, top_mm, fixture, tilted: false } => write!(
                f,
                "the first rapid (move {at}) comes from an unknown X/Y at the clearance height Z{height_mm:.3}, under the {} \
                 top at Z{top_mm:.3} - raise the clearance height above Z{top_mm:.3}",
                if *fixture { "fixture" } else { "stock" }
            ),
            // #2991: a 3+2 plane measures its heights along the tilted
            // tool axis, so a Z value would not be one the user can find.
            // The raise is the same in each frame.
            Warning::FirstTraverseLow { at, height_mm, top_mm, fixture, tilted: true } => write!(
                f,
                "the first rapid (move {at}) comes from an unknown position at the clearance height {height_mm:.3} \
                 mm, under the {} top at {top_mm:.3} mm (both measured along the tilted tool axis) - raise the clearance height by at least {:.3} mm",
                if *fixture { "fixture" } else { "stock" },
                top_mm - height_mm
            ),
            // #3148: the axis is always tilted here, so the heights are
            // the user's values along it, never a Z value.
            Warning::FirstTraverseUnderTable { at, height_mm, limit_mm } => write!(
                f,
                "the first rapid (move {at}) runs from the tool-change height along a tool axis that points down toward the \
                 machine table; to the clearance plane at {height_mm:.3} mm it can go under the table top - lower the \
                 clearance plane to {limit_mm:.3} mm or under it (both measured along the tilted tool axis)"
            ),
            Warning::FixtureHit { at, fixture_part } => write!(
                f,
                "{} at move {at} strikes a fixture",
                match fixture_part {
                    FixturePart::Shank => "shank",
                    FixturePart::Holder => "holder",
                    FixturePart::Spindle => "spindle housing",
                    FixturePart::Rapid => "rapid",
                    FixturePart::Flute => "flute",
                }
            ),
            Warning::NonFinite { at } => write!(f, "move {at} has a non-finite coordinate or tool axis"),
            Warning::Envelope {
                at,
                gouge_mm,
                rest_mm,
            } => write!(
                f,
                "finish envelope near move {at}: {gouge_mm:.4} mm gouge, {rest_mm:.4} mm rest"
            ),
            Warning::RestUnjudged { at, cells: 0, claims: 0 } => write!(f, "bull-nose rest unmeasured from move {at}"),
            Warning::RestUnjudged { at, cells, claims } => write!(
                f,
                "finish envelope from move {at}: hidden rest NOT JUDGED - the scan's budget ran out ({cells} cells, {claims} plane claims); material under an overhang may stand unreported"
            ),
            Warning::Scallop { at, mm, gate_mm } => write!(
                f,
                "scallop {mm:.3} mm over the {gate_mm:.3} mm tolerance at move {at} (surface quality - the post is not held)"
            ),
            Warning::ProbeSearch { at, mm, limit_mm } => write!(
                f,
                "probe distance {mm:.3} mm at move {at}: the search may run that far past the expected contact, beyond the {limit_mm:.3} mm a stylus is trusted to overtravel (the post is not held)"
            ),
            Warning::SinkerFinishOrder { at, finish_mm, rough_mm } => write!(
                f,
                "sinker finish at move {at}: its electrode's undersize {finish_mm:.3} mm per side is not smaller than the rough's {rough_mm:.3} mm - the finish electrode is no larger than the rough and takes nothing off its walls (the post is not held)"
            ),
            Warning::UndercutLift { at } => write!(
                f,
                "move {at} moves the undercut cutter in Z with material in or over its head (it must enter and leave sideways, clear of the stock)"
            ),
            Warning::AvoidHit { at, item, part, short_mm } => write!(
                f,
                "{} at move {at} violates protected 'Do not touch' item #{item} ({short_mm:.3} mm short of clearance)",
                match part {
                    FixturePart::Shank => "shank",
                    FixturePart::Holder => "holder",
                    FixturePart::Spindle => "spindle housing",
                    FixturePart::Rapid => "rapid",
                    FixturePart::Flute => "flute",
                }
            ),
            Warning::DrillUndersize { at, hole_mm, drill_mm } => write!(
                f,
                "undersize hole at move {at}: the hole is {hole_mm:.3} mm, and the {drill_mm:.3} mm drill makes it {drill_mm:.3} mm - use a {hole_mm:.3} mm drill, or follow with a Bore or a reamer (the post is not held: a pilot drill is a valid plan)"
            ),
            Warning::StrategyFallback { at, areas, cause: FallbackCause::Adaptive } => write!(
                f,
                "Adaptive fell back to Offset rings on {areas} area{} from move {at}: the notes name each one and why (the post is not held: the rings are verified)",
                if *areas == 1 { "" } else { "s" }
            ),
            Warning::StrategyFallback { at, areas, cause: FallbackCause::PassStart } => write!(
                f,
                "Pass starts at: the selected start side would leave material uncut in {areas} area{}, so the Auto order was used there instead, from move {at}: the notes name each one (the post is not held: the cut is verified)",
                if *areas == 1 { "" } else { "s" }
            ),
            Warning::FluteShort { at, flute_mm, depth_mm } => write!(
                f,
                "the tool's flute length is {flute_mm:.3} mm, but the cut goes {depth_mm:.3} mm below the stock top from move {at}: \
                 above the flutes, the shank or the holder goes into the cut; the findings after this one name the moves where \
                 it meets material - use a tool with a longer flute, or cut less deep (this finding alone does not hold the post)"
            ),
            Warning::PlungeTooDeep { at, mm, limit_mm } => write!(
                f,
                "feed plunge at move {at} reaches {mm:.3} mm below the floor that no level above cut, deeper than one \
                 stepdown ({limit_mm:.3} mm) - cut the levels above first, or plunge in pecks of one stepdown (the post is \
                 not held: the risk is the tool load)"
            ),
            Warning::EntryNotRecommended { at, entry, inside } => write!(
                f,
                "{}, from move {at} (the post is not held: the cut is verified)",
                super::entry_advice(*entry, *inside)
            ),
            Warning::EntryStandIn { at, entries } => write!(
                f,
                "Pass starts at: the selected entry did not fit at {entries} pass start{} in material, from move {at}: \
                 a stand-in entry is used there, and the notes name each one - set the stock so that an Arc entry can \
                 start in the air outside it, or use a smaller stepover (the post is not held: the cut is verified)",
                if *entries == 1 { "" } else { "s" }
            ),
            Warning::PlungeIntoStock { at } => write!(
                f,
                "Pass starts at: the selected Plunge entry plunges into uncut stock where a pass starts in material, \
                 from move {at}: Arc is recommended, because it enters from the air outside the stock where it fits \
                 (the post is not held: the cut is verified)"
            ),
        }
    }
}
