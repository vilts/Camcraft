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
    /// Safe plane: rapids must live at or above it.
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
    /// Rapid `move` travels under the clearance plane through stock.
    RapidInStock { at: usize },
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
    /// Defensive: a non-finite coordinate (generators gate this out).
    NonFinite { at: usize },
    /// A 3-D finish's measured ball envelope misses the model:
    /// `gouge_mm` below it (worst), `rest_mm` above it (worst),
    /// reported at the cutting move nearest the deviation's XY.
    Envelope {
        at: usize,
        gouge_mm: f64,
        rest_mm: f64,
    },
    /// A 3-D finish's envelope verdict could not judge the material
    /// hidden under overhangs (M13.1): the hidden-rest scan stopped at
    /// its budget (`cells` the lattice, `claims` the plane claims
    /// binned when it stopped), so whatever rest the verdict reports
    /// speaks for the ridge law alone. "Not judged", never a zero -
    /// reported at the first cutting move, and like every cached
    /// warning it holds the post.
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
}

impl Warning {
    /// Whether this warning holds the post: every kind but the scallop
    /// reading (M13.9), which is a quality note - shown, never binding.
    #[must_use]
    pub fn blocks_post(&self) -> bool {
        !matches!(self, Warning::Scallop { .. } | Warning::ProbeSearch { .. })
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
                write!(f, "rapid move {at} travels below the clearance plane inside stock")
            }
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
            Warning::NonFinite { at } => write!(f, "move {at} has a non-finite coordinate"),
            Warning::Envelope {
                at,
                gouge_mm,
                rest_mm,
            } => write!(
                f,
                "finish envelope near move {at}: {gouge_mm:.4} mm gouge, {rest_mm:.4} mm rest"
            ),
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
        }
    }
}
