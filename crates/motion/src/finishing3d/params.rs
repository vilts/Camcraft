//! Split from `finishing3d.rs` by M5.0.1; structure only.

pub use super::validate::{scallop_stepdown, scallop_stepover};
pub(super) use super::validate::{dirs, refinement_floor, validate};

/// Note prefix pushed onto [`crate::cl::Toolpath::notes`] when the
/// waterline ring pass is disabled and steep flanks stand uncut (#2811).
pub const WATERLINES_OFF_NOTE: &str = "waterlines off:";

/// Which finishing kinematic produces the path. Waterline is the
/// Z-level ring ladder; Parallel adds the Z-following shallow raster
/// (walls keep riding the rings - the auto-split, see the module
/// laws); ParallelMulti runs the raster twice, one pass per XY axis,
/// so the cusp budget is spent along both directions; Scallop runs
/// it twice along the DIAGONALS - the constant-cusp promise: no
/// linear terrain feature (which in this codebase's prismatic
/// models means none aligned to X or Y) can run parallel to both
/// families, so no feature's cusp escapes a crossing cut. The flat
/// cusp of the crossed pair is the single family's - the kernel
/// measures it, and no analytic sqrt(2) discount is claimed. Radial
/// (M2.2d) sends chords through the shallow terrain's center, one
/// per stepover of arc at the rim; Spiral runs ONE Archimedean
/// curve in from the silhouette - the retract-free continuous cut.
/// Both ride `march_curve`'s laws verbatim - only the placement of
/// the ground lines changes, never what a step promises - and both
/// hand the steep flanks to the same rings. Contour (M2.2e) shrinks
/// the terrain's center-shadow by one stepover per pass - concentric
/// loops, each lifted sample sitting tangent wherever a tangent
/// exists - and Project runs only its boundary: the model's own
/// silhouette, XY kept, Z lifted. Rings close both families.
/// SteepShallow (M2.5 A) is the auto-split spoken as a partition:
/// the raster rides the shallow ground exactly as Parallel does,
/// and the ring ladder keeps its hands OFF shallow tangency - the
/// 45° line stops being a meeting place and becomes a border, so
/// no slope is ridden twice and the flanks answer to their OWN
/// spacing number (`steep_stepover`, the ring ladder's author when
/// stated). Flat (M2.5 A) finishes the floors only: the near-level
/// facets' footprints, shrunk one stepover per pass at constant
/// tip-Z - the shadow family's choreography on the ground the
/// waterline's rest verdict always refused to claim. Pencil and
/// Corner (M2.5 B) are the crease pair: each rides the locus of
/// centers tangent to BOTH faces of a concave edge at once, and the
/// band `p/tan(theta/2)` - the skirt a ball of radius p can never
/// clean - says which of the two speaks: Corner for the tool's own
/// p = r, Pencil for a stated reference's p = R, keeping only where
/// the bigger ball's shadow stands past the tolerance.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FinishPattern {
    Waterline,
    Parallel,
    ParallelMulti,
    Scallop,
    Radial,
    Spiral,
    Contour,
    Project,
    /// The two-truth split as a border, each side with its number.
    SteepShallow,
    /// Floors only, planar, one pass per floor.
    Flat,
    /// The concave corner loci a reference ball leaves standing,
    /// ridden by this op's smaller ball (M2.5 B): `pencil_reference`
    /// is the pen, and absent means every corner.
    Pencil,
    /// The simultaneous two-face tangent pass with the operation's
    /// own ball - where the tool's own band beats the tolerance
    /// (M2.5 B). `pencil_reference` is inert here.
    Corner,
    /// Parallel's twin with the sloped laws always on (M2.5 C): the
    /// diagonal chain between the raster's lines (`ramp`) and the
    /// ramped first contacts on steep tangents (`entry_ramp`) are
    /// forced on whatever the pane says - the catalog's "sloped
    /// paths" name, assembled from laws this module already owns.
    Ramp,
    /// The spiral with the terrain's own spacing (M2.5 C): one
    /// in-winding curve whose radius advances by the LOCAL scallop
    /// law - wide where the ground tilts, narrow where it's flat -
    /// so the cusp budget is spent evenly instead of at the
    /// footprint's worst tilt.
    MorphSpiral,
    /// Rings of equal GEODESIC distance from the model's open
    /// boundary (M2.5 C): contours that run WITH the surface - down
    /// the wall and across the floor on one continuous loop -
    /// placed by arc-length along the mesh, not by Z. Needs an open
    /// boundary to march from; a closed shell is refused by name.
    Geodesic,
    /// The two families BOTH ride the transition (M2.5 D): the
    /// raster's lines over the shallow ground exactly as Parallel
    /// places them, and the ring ladder kept wherever a loop's
    /// tangency reaches past 30 degrees - so the band between the
    /// floor's law and the wall's law is cut twice, from both
    /// directions, instead of leaving the 45° border's staircase as
    /// the price of one clean split. Flat interiors stay raster-only:
    /// a ring that touches nothing flatter than the band is the
    /// ladder's hands off the floor, as the border law always said.
    Blend,
    /// The ladder's levels joined by ruled lofts (M2.5 D2): the
    /// waterline rings ride as always, and between every consecutive
    /// pair of levels whose loops pair (area rank + centroid within
    /// one tool diameter) one interpolated guide runs, its XY the
    /// loop-to-loop ruling's midpoint and its Z the column's own
    /// answer - halving the spacing quarters the cusp, and where the
    /// levels' geometry disagrees about being one feature, nothing
    /// is paired and the rings alone carry the step, as before.
    Morph,
    /// The flowline pass (M2.5 E): lines that CURVE with the part,
    /// seeded on the lattice and traced along the surface's own
    /// least-curving grain - the curvature field of `multiaxis`'
    /// 5-axis sibling, run with NO axes stamped: a plain XYZ
    /// finish, the tool's axis vertical, the lines the only thing
    /// that bends. Flat ground makes no claim; there the fallback
    /// walks straight, and flow over plain ground IS the parallel
    /// raster - cutting, never stalling.
    Flow,
}

/// How the chain between two cut runs crosses the ground between
/// them, asked once per break. `Jump` is every pre-gap-link file:
/// retract (or clearance when no retract is stated) and rapid
/// across. `Straight` first tries one direct link: a single
/// straight move from where the last run ended to where the next
/// begins, at the two points' own heights - no rise, no traverse,
/// no plunge. That line is fixed, so nothing is chosen for it: the
/// exact sweep verdict answers the whole question (a ball swept
/// along a segment is that segment inflated by `r`, the same
/// question the envelope scan asks), and its budget keeps every
/// rest tangency - both ends are legal centers by the march's own
/// construction, tangent to their ground at distance exactly `r` -
/// while a real bite refuses. A refused link takes the `Jump` hop.
/// `ramp`'s certified diagonal still wins the whole question when
/// it can be struck: a chain that cuts its way across needs no
/// link at all. The ladder is ramp, straight, jump - the smoothest
/// answer the facets certify, and the jump stands when none can.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum GapLink {
    #[default]
    Jump,
    /// One straight link between the runs at their own heights,
    /// where the exact sweep certifies the ball crosses clear.
    Straight,
}

/// Everything the 3D finishers need that the tool and geometry don't
/// already say. Heights arrive resolved to millimeters by the
/// operation layer (same discipline as every other params shape).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Finish3DParams {
    /// Which kinematic runs; `Waterline` ignores the raster fields,
    /// exactly as if this enum had never arrived.
    pub pattern: FinishPattern,
    /// Raster line spacing override, mm. `None` follows the cos-law
    /// `√(8·r·tol)·min(m_z)` from the tolerance, like the stepdown
    /// does for the rings: the tolerance is the author.
    pub stepover: Option<f64>,
    /// Ball-nose diameter, mm.
    pub tool_diameter: f64,
    /// Surface deviation budget, mm - both directions. It sets the
    /// stepdown and the arc-fit chord budget together: the rings are
    /// promised within `tol` of the true tangent locus.
    pub tolerance: f64,
    pub clearance_z: f64,
    pub retract_z: Option<f64>,
    /// Tip-Z of the first (highest) ring.
    pub top_z: f64,
    /// Tip-Z of the last ring.
    pub bottom_z: f64,
    pub feed: f64,
    pub plunge_feed: f64,
    /// Climb runs rings in stored order (the forbidden region riding
    /// on the left), conventional reverses - the same XOR contour
    /// applies to planar loops.
    pub climb: bool,
    /// Ramp the entries (M2.2e): between two runs of the SAME family,
    /// one straight diagonal Feed replaces the hop-to-clearance
    /// wherever [`diagonal_ok`] can certify it - the ball never sinks
    /// below its promised surface en route and never leaves the
    /// region. Where it can't certify (a hill between the lines, open
    /// air, a flank crossing), the hop stands, unchanged. Between-
    /// family transits are never ramped by this flag: the diagonal
    /// here is a chain between cuts, not a way into a new one. First
    /// contacts are the domain of [`Finish3DParams::entry_ramp`]
    /// (M8.2 B) - the one place a diagonal IS a way into a cut.
    pub ramp: bool,
    /// Ramp the FIRST contacts on steep tangents (M8.2 B): the ring
    /// law promises the vertical plunge never cuts above its level,
    /// but on a flank steeper than the 45° split the ball MEETS the
    /// wall all at once. With this on, such an entry slides in along
    /// its contact normal instead: a certified diagonal Feed from an
    /// air point down to the tangent, engaging the wall in order. The
    /// certification currency is the same as the law's test: sampled
    /// centers never bite past `tolerance`. Where the normal can't be
    /// read, points at or below the horizon (an overhang, or a flank
    /// too near-vertical to lift off - a lateral in-feed at depth is
    /// not an entry), can't start below the clearance plane, or bites
    /// along the way, the vertical plunge stands and the path carries
    /// a note saying why. `false` is v0.4's behavior byte for byte;
    /// the ring walk itself is never touched.
    pub entry_ramp: bool,
    /// Floor for the adaptive march's refinement, mm (M2.2e): the
    /// march bisects a chord whenever its middle sinks past the
    /// quarter budget, down to `delta0/4096` by its own right. Set
    /// this to say NO cut segment shorter than `s` - coarse ground
    /// you chose, not ground the chord law found. The run-edge
    /// bisections are NOT floored: where a cut starts and stops is
    /// the material's business. `None` rides the march's own floor;
    /// Waterline ignores it, exactly as if it had never arrived
    /// (the same jurisdiction the raster fields keep).
    pub min_step: Option<f64>,
    /// The steepest descent a ramp link may carry, degrees (M2.2e):
    /// between two runs, the chain's TRAVERSE is a straight line,
    /// and a drop steeper than `dz = d_xy·tan(angle)` is not a ramp:
    /// it is a plunge in a diagonal coat, and plunges belong to
    /// the run's own first contact. 45° is the default and the
    /// law's own gift: shallow ground is exactly the ground whose
    /// support climbs no more than one in one, so an angle of 45
    /// certifies every traverse the raster's jurisdiction can
    /// present - the default is v17's behavior, spelled. Links
    /// that barely traverse (`d_xy` below the tolerance) are the
    /// vertical case the ramp law already settled, gate untouched.
    pub ramp_angle: f64,
    /// The ring ladder's spacing, mm (M2.5 A): the flank's own
    /// number, stated at the slope the partition draws. Waterline's
    /// ladder has never had one - the scallop stepdown authored it -
    /// and that is the surface-finish tail M2.2 left for this
    /// stage: a `stepover` for the lines, a `steep_stepover` for
    /// the rings. `Some(s)` spaces the levels by `s` exactly (the
    /// same authored-over-law grammar `stepover` keeps for the
    /// raster - the envelope verdict, not the number, is what holds
    /// the promise); `None` - every pre-M2.5 file, and every
    /// pattern other than SteepShallow - rides the scallop law
    /// `2√(r·tol)`, byte for byte, as if this field had never
    /// arrived.
    pub steep_stepover: Option<f64>,
    /// The reference ball's diameter, mm (M2.5 B): Pencil cleans the
    /// corner bands this tool's ball cannot reach but that a bigger
    /// ball of diameter `d` leaves standing - it cuts where
    /// `(d/2 − r)/tan(θ/2)` beats the tolerance. `None` says "no
    /// bigger ball was ever named": every concave corner the tool's
    /// locus rests on is fair game. The number must exceed the op's
    /// own tool diameter - a reference not bigger than the tool
    /// hides under no band, and that is refused by name, not
    /// shrunk. Every other pattern ignores it, exactly as the
    /// raster fields ignore Waterline, inert as if it had never
    /// arrived.
    pub pencil_reference: Option<f64>,
    /// The link's author between two runs (the linking menu): `Jump`
    /// hops to `retract_z`/clearance exactly as every pre-gap-link
    /// file ran; `Straight` links the runs with one direct move at
    /// their own heights where the exact sweep certifies it, and
    /// takes the hop when it cannot - see [`GapLink`] and
    /// [`straight_ok`](super::gaplink::straight_ok). `ramp`'s
    /// diagonal still takes the break first wherever
    /// [`diagonal_ok`](super::march::diagonal_ok) certifies it; the
    /// gap link governs the break the diagonal refused. It is the
    /// raster run chain's height; the ring walk keeps the plain
    /// retract/clearance hop for now.
    pub gap_link: GapLink,
    /// Run the waterline ring pass on steep flanks after the shallow
    /// pass (#2811). `true` (default) preserves every pre-switch path
    /// byte for byte; `false` skips the ring pass so only the shallow
    /// pass runs, emitting a note when steep flanks stand uncut and
    /// refusing with `GenError::Empty` on an all-steep model.
    pub waterlines: bool,
}

/// The steep/shallow jurisdiction line, as a cosine: facets whose
/// upward normal tilts more than 45° off vertical get no raster
/// support and belong to the rings. The same 45° the stepdown law
/// already promises (`|α| ≤ 45°` keeps the ring cusp inside `tol`),
/// so the two strategies meet exactly at each one's limit - no
/// territory claimed twice, none dropped.
pub(super) const SPLIT_MZ: f64 = std::f64::consts::FRAC_1_SQRT_2;

/// The Blend's band line, as a cosine: 30° off vertical. Why there:
/// the ring cusp on a facet tilted `α` is `Δz² / (8·r·cos²α)` - at
/// 30° it is `4/3` of the vertical promise, at 45° twice it - so
/// between 30° and the split the two families' numbers are within
/// a factor of the SAME tolerance of each other, and cutting that
/// band twice shaves the border's staircase instead of trading
/// promises. Flatter ground is worth only the raster's line; the
/// rings let it alone, exactly as the border law says.
pub(super) const BLEND_MZ: f64 = 0.866_025_403_784_438_7;

/// The numeric skin of the laws: a center that touches a facet sits
/// at distance exactly `r` from it, and float says `r ± 1e-9·r`.
/// Accept contact within this much bite.
pub(super) const TOUCH_EPS: f64 = 1e-7;
