//! Zigzag / waterline pocketing (split from `machining2d.rs` by M5.0.1);
//! M11.1 C adds the optional corner/link fillets - provenance in
//! `fillet.rs` (tangent circle to two lines, RS-274 / ISO 6983 G2/G3).

use crate::cl::{PathBuilder, Toolpath};
use crate::entry::Entry3D;
use crate::load::{load_in_door, load_step};
use crate::geo2d::Region;
use crate::geometry::Point3;
use crate::links::Links;

use super::corner_relief::{CornerRelief, ResolvedRelief, plan_corner_reliefs};
use super::face::GenError;
use super::arcs::ARC_FIT_TOL;
use super::fillet_tally::corner_arc_radius;
use super::level_rings::ring_plan;
use super::pass_start::{PassStart, StartSide};
use super::pocket_emit::{PocketWalks, emit_pocket_passes};
use super::pocket_hsm_start::adaptive_passes_from;
use super::pocket_zigzag::rest_passes;
pub(crate) use super::pocket_plan::plan_passes;
pub(super) use super::pocket_plan::non_sliver_center;
use super::predrilled::PreHole;
pub use super::pocket_strategy::PocketStrategy;


/// 2D pocketing: clear every bit of material inside a region down to
/// `bottom_z`. Islands (region holes) are never entered; the cutter
/// center stays inside the region eroded by the tool radius, so the tool
/// edge never crosses a wall it is meant to leave standing.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PocketParams {
    pub tool_diameter: f64,
    /// Distance between successive rows (zigzag) or rings (waterline).
    /// Validated against the tool radius: a wider step leaves bands the
    /// path never reaches, and that is refused rather than accepted
    /// silently.
    pub stepover: f64,
    pub stock_to_leave: f64,
    pub clearance_z: f64,
    /// First cut level; material is engaged from here down.
    pub top_z: f64,
    pub bottom_z: f64,
    pub stepdown: f64,
    pub feed: f64,
    pub plunge_feed: f64,
    /// Zigzag: the first row runs -X to +X. Waterline: rings are
    /// travelled with material on the left of the cut.
    pub climb: bool,
    pub strategy: PocketStrategy,
    /// Between-ring hop plane within one level. `None` hops to
    /// clearance - every pre-M1.5.2 pocket's shape, kept byte-exact.
    pub retract_z: Option<f64>,
    /// How the ladder crosses a level plane (M5.1 H, the family the
    /// 3-D ladder names in `crate::entry`). `Plunge` is every pocket
    /// posted before this field existed, and the arm's legality
    /// proofs live in `run_levels` the same way `ladder.rs` owns
    /// them for the 3-D family.
    pub entry: Entry3D,
    /// Helix centerline circle radius (mm); validated at most the tool
    /// radius so successive turns overlap and the dive cuts a clean
    /// bore - the 3-D ladder's door, same words.
    pub helix_radius: f64,
    /// The `Adaptive` strategy's optimal load: radial width of cut as a
    /// fraction of the tool diameter (ae/D, `crate::load`); read only
    /// under `Adaptive` (through [`PocketParams::tiling_step`]), still
    /// validated finite. `crate::load::DEFAULT_OPTIMAL_LOAD` is exactly
    /// the step the old 45-degree default cut.
    pub optimal_load: f64,
    /// Turning-point blends (M11.1 C): every corner of a pass - the
    /// zigzag's row-to-link jogs, a ring's corners, line or arc on
    /// either side - rides one tangent fillet where the law of
    /// `fillet.rs` / `fillet_arc.rs` proves it inside the
    /// cutter-center area; refusals stay sharp and are named in the
    /// path's notes, with the worst corner rest. `false` - every
    /// pre-M11.1 file - is the sharp walk, unchanged to the byte.
    pub blend_turns: bool,
    /// The corner arc's radius (#1733), read only with `blend_turns`:
    /// `None` is the tool radius every earlier file blends at; a smaller
    /// one leaves less corner rest. The law still shrinks a corner that
    /// has no room; `corner_arc_radius` is the door.
    pub blend_radius: Option<f64>,
    /// Level transitions and links (M11.2 C/D); [`Links::PLANAR`] is
    /// the ladder every earlier pocket posted.
    pub links: Links,
}

impl PocketParams {
    pub fn validate(&self) -> Result<(), GenError> {
        let mut finite = [
            self.tool_diameter,
            self.stock_to_leave,
            self.clearance_z,
            self.top_z,
            self.bottom_z,
            self.stepdown,
            self.stepover,
            self.feed,
            self.plunge_feed,
            self.helix_radius,
            self.optimal_load,
        ]
        .into_iter()
        .all(f64::is_finite);
        finite &= self.retract_z.is_none_or(f64::is_finite);
        if !finite {
            return Err(GenError::Invalid("parameters must be finite".into()));
        }
        if self.tool_diameter <= 0.0 || self.stepdown <= 0.0 {
            return Err(GenError::Invalid("diameter and stepdown must be positive".into()));
        }
        if self.stepover <= 0.0 {
            return Err(GenError::Invalid("stepover must be positive".into()));
        }
        // Two parallel cut lines a step apart cover every point between
        // them (distance at most step/2 to the nearer one), and zigzag
        // puts grazing rows on the extreme coverage centers, so a step
        // within the diameter covers the region. Waterline's innermost
        // ring can stand a whole step from the material it collapses
        // around, so it must stay within one radius. Auto is bounded by
        // the looser law: its waterline branch carries the radius bound
        // inside `resolve`, and anything else runs zigzag. The checked
        // number is `tiling_step` - what spaces the passes - so an
        // `Adaptive` plan is honest at its engagement-law step even
        // where the (ignored) stepover field would not be (M5.1 H;
        // the 3-D ladder makes the same distinction the other way:
        // it bounds the field and never coarsens the grid past the law).
        let reach = match self.strategy {
            PocketStrategy::Zigzag | PocketStrategy::Auto => self.tool_diameter,
            // Adaptive rings are Waterline's geometry; the same reach
            // bound keeps their innermost ring honest. The law's step
            // is at most one radius for any angle in the door, so
            // beyond it the door is the check.
            PocketStrategy::Waterline | PocketStrategy::Adaptive | PocketStrategy::OffsetRings => {
                self.tool_diameter * 0.5
            }
        };
        if self.tiling_step() > reach {
            let what = if self.strategy.steps_by_load() { "optimal-load step" } else { "stepover" };
            return Err(GenError::Invalid(format!(
                "{what} {:.3} mm leaves bands the tool cannot reach (at most {reach:.3} mm for this strategy)",
                self.tiling_step()
            )));
        }
        if self.feed <= 0.0 || self.plunge_feed <= 0.0 {
            return Err(GenError::Invalid("feeds must be positive".into()));
        }
        if self.stock_to_leave < 0.0 {
            return Err(GenError::Invalid("stock to leave must not be negative".into()));
        }
        if self.top_z < self.bottom_z {
            return Err(GenError::Invalid("cut top below cut bottom".into()));
        }
        if self.clearance_z < self.top_z {
            return Err(GenError::Invalid("clearance plane below the first cut level".into()));
        }
        if self.retract_z.is_some_and(|rz| rz < self.top_z) {
            return Err(GenError::Invalid("retract plane sits inside the material".into()));
        }
        self.links.validate().map_err(|e| GenError::Invalid(e.into()))?;
        if self.blend_turns {
            corner_arc_radius(self.blend_radius, self.tool_diameter).map_err(GenError::Invalid)?;
        }
        if self.entry == Entry3D::Helix || self.links.transition == Some(Entry3D::Helix) {
            let r = self.tool_diameter * 0.5;
            if self.helix_radius <= 0.0 || self.helix_radius > r {
                return Err(GenError::Invalid(
                    "helix radius must be positive and at most the tool radius".into(),
                ));
            }
        }
        if matches!(self.strategy, PocketStrategy::Adaptive | PocketStrategy::OffsetRings) {
            // The law's own bounds, same as the 3-D ladder's door: a
            // zero load is a zero step (an infinite ladder), and past
            // half the diameter (90 degrees of contact) the step passes
            // one radius - past where consecutive rings still overlap.
            if !load_in_door(self.optimal_load) {
                return Err(GenError::Invalid(
                    "optimal load must be in (0, 50] % of the tool diameter".into(),
                ));
            }
        }
        Ok(())
    }

    /// What actually spaces the passes at a level: the optimal-load
    /// step under `Adaptive`, the stated stepover otherwise. The pass
    /// plan and the reach door both ask this one function, mirroring
    /// `Pocket3DParams::tiling_step` (M5.1 H).
    pub(super) fn tiling_step(&self) -> f64 {
        if self.strategy.steps_by_load() { load_step(self.tool_diameter, self.optimal_load) } else { self.stepover }
    }
}

/// Pocket `region` (the material to remove; its loops' interiors are
/// islands to leave standing). The cutter-center area is the region
/// eroded by `r + leave` - the opposite sign to contouring, because here
/// the tool must stay inside the material rather than outside it. Every
/// feed point lies in that area, so the tool edge never crosses a wall.
///
/// Transitions: between levels the tool crosses in with its `entry`
/// family - the dive it always posted, or the ramp diagonal / helical
/// bore the 3-D ladder proves and `run_levels` now shares (M5.1 H);
/// within a level a straight connection between two cut segments is
/// kept as a Feed exactly when [`super::slots::chord_clears`] proves it never leaves
/// the cutter-center area (boundary grazing counts as staying in - it
/// is the tangency wall passes are built on), and otherwise it becomes
/// lift-rapid-plunge. Rapids inside the footprint below the safe plane
/// are the verifier's business (step 7).
pub fn pocket(region: &Region, p: &PocketParams) -> Result<Toolpath, GenError> {
    pocket_entered(region, p, &[])
}

/// [`pocket`] with the holes the caller proved open (#1449): read only
/// when `p.links.approach.predrilled` asks (`predrilled.rs`).
pub fn pocket_entered(region: &Region, p: &PocketParams, holes: &[PreHole]) -> Result<Toolpath, GenError> {
    pocket_entered_with_relief(region, p, holes, &CornerRelief::default())
}

/// [`pocket_entered`] with optional inside-corner reliefs (#2400) for closed
/// 2D pocket and square-ended slot boundaries.
pub fn pocket_entered_with_relief(
    region: &Region,
    p: &PocketParams,
    holes: &[PreHole],
    relief: &CornerRelief,
) -> Result<Toolpath, GenError> {
    pocket_started(region, p, holes, relief, PassStart::Auto)
}

/// [`pocket_entered_with_relief`] with the side each level starts on
/// (#3316); [`PassStart::Auto`] is that function, byte for byte.
pub fn pocket_started(
    region: &Region,
    p: &PocketParams,
    holes: &[PreHole],
    relief: &CornerRelief,
    start: PassStart,
) -> Result<Toolpath, GenError> {
    pocket_started_on(region, p, holes, relief, &StartSide::from(start))
}

/// [`pocket_started`] with the stock known: Inside and Outside walk the
/// level curves of one side (`level_rings`), each full-width cut named.
/// The Adaptive plan reads `side.stock` and `side.start` through
/// `pocket_hsm_start::adaptive_passes_from` (#3355).
pub fn pocket_started_on(
    region: &Region,
    p: &PocketParams,
    holes: &[PreHole],
    relief: &CornerRelief,
    side: &StartSide,
) -> Result<Toolpath, GenError> {
    p.validate()?;
    relief.validate()?;
    if region.is_empty() {
        return Err(GenError::Empty("empty pocket region".into()));
    }
    let off = p.tool_diameter * 0.5 + p.stock_to_leave;
    let center = non_sliver_center(region, off)?;
    if center.is_empty() {
        // Material exists that no legal tool position can reach.
        return Err(GenError::Invalid("pocket is narrower than the tool".into()));
    }
    let plan = plan_corner_reliefs(region, &center, off, relief)?;
    pocket_over_with_relief(region, &center, p, Vec::new(), false, holes, &plan.reliefs, side)
}

/// The pocket passes over a given cutter-center area: [`pocket`]'s body
/// once the area is known, shared with rest machining (`rest2d`, #1446),
/// whose area is the part of the pocket's own that touches the rest
/// material. `center` must lie inside `region` eroded by the tool radius
/// plus the stock to leave; `notes` are said first on the path.
/// `hold_crossings` holds every fitted arc to the crossing law on the
/// posted path (`arc_cross`): rest areas have thin horns where two
/// fitted arcs can cross within tolerance. `holes` are the proved
/// pre-drilled entries (#1449; rest machining passes none).
#[allow(clippy::too_many_arguments)] // the body of every planar pocket: area, plan inputs and the #3316 pass start, with its #3355 stock outline, threaded once
pub(super) fn pocket_over_with_relief(
    region: &Region,
    center: &Region,
    p: &PocketParams,
    notes: Vec<String>,
    hold_crossings: bool,
    holes: &[PreHole],
    corner_reliefs: &[ResolvedRelief],
    side: &StartSide,
) -> Result<Toolpath, GenError> {
    // Auto commits to a shape here, from the region's own outline - the
    // outermost loop (largest area; holes and islands never decide the
    // pocket's strategy). The panel quotes the same law on the same
    // points, so what was cut is what was said.
    let strategy = match p.strategy {
        PocketStrategy::Auto => {
            let outer = region
                .loops()
                .iter()
                .max_by(|a, b| a.area().abs().total_cmp(&b.area().abs()))
                .map(|l| l.points())
                .unwrap_or(&[]);
            p.strategy.resolve(outer, p.stepover, p.tool_diameter, ARC_FIT_TOL)
        }
        s => s,
    };
    let mut b = PathBuilder::approached(Point3::new(0.0, 0.0, p.clearance_z));
    notes.into_iter().for_each(|n| b.note(n));
    // #3744: Auto round a part picks Inside or Outside of the one ring
    // set here, so the rings, the links and the entry read one side.
    let asked = side.start;
    let side = &super::ring_side::resolve(center, strategy, p.tool_diameter * 0.5, side)?;
    // The HSM plan may choose where its entry stands (`pocket_hsm`).
    let (mut passes, strategy, room, joins) = match strategy {
        PocketStrategy::Adaptive => {
            let (passes, notes, s, room, joins) = adaptive_passes_from(region, center, p, "this pocket", side)?;
            notes.into_iter().for_each(|n| b.note(n));
            (passes, s, room, joins)
        }
        s => {
            let (passes, notes) = ring_plan(region, center, p, s, side)?;
            notes.into_iter().for_each(|n| b.note(n));
            (passes, s, None, Vec::new())
        }
    };
    // #3021: rows alone leave a cusp between row ends on every wall and
    // island; each Zigzag pocket closes with the area's boundary rings.
    // A blended row turn can cross a ring, so the blends obey the
    // crossing law as the rest pocket's do.
    let zigzag = strategy == PocketStrategy::Zigzag;
    if zigzag {
        passes = rest_passes(center, p.tiling_step(), p.climb)?;
        // #3354: rows have no inside or outside; say so by name.
        if side.start != PassStart::Auto {
            b.note(super::pass_start::PASS_START_ZIGZAG_NOTE.to_owned());
        }
    }
    let hold_crossings = hold_crossings || zigzag;
    // #3746: a start-side ring plan round a part enters from the air
    // outside the stock box.
    let air = super::air_entry::AirRoom::for_side(side, strategy, center, p.tool_diameter * 0.5, p.links.approach.approach_distance);
    // #3884: a selected Arc on an Outside ring pass with no air room is
    // refused by name (no automatic ramp or helix).
    // An Adaptive op on its ring fallback too (#3884 review).
    let rings = matches!(strategy, PocketStrategy::Waterline | PocketStrategy::OffsetRings);
    super::entry_warn::refuse_outside_arc(asked, p.entry, rings, air.is_some())?;
    // #3748: the user's Inside or Outside with a lead-in other than Arc:
    // used as selected, named. #3884: the air room is not the gate.
    // Not for an Adaptive op on its ring fallback: its page has no advice.
    if rings
        && p.strategy != PocketStrategy::Adaptive
        && let Some(n) = super::pass_start::entry_advice_note(asked, p.entry)
    {
        b.note_once(&n);
    }
    emit_pocket_passes(
        b,
        p,
        PocketWalks {
            center,
            links_center: center,
            strategy,
            passes,
            room,
            joins,
            hold_crossings,
            holes,
            corner_reliefs,
            cleared_links: side.start != PassStart::Auto,
            ring_links: match side.start {
                PassStart::Auto => None,
                _ => super::ring_links::for_side(side, strategy, p.tool_diameter * 0.5, None)?,
            },
            air,
        },
    )
}
