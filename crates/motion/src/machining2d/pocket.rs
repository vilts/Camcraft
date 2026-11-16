//! Zigzag / waterline pocketing (split from `machining2d.rs` by M5.0.1);
//! M11.1 C adds the optional corner/link fillets - provenance in
//! `fillet.rs` (tangent circle to two lines, RS-274 / ISO 6983 G2/G3).

use crate::cl::{MoveKind, PathBuilder, Toolpath};
use crate::entry::Entry3D;
use crate::load::{load_in_door, load_step};
use crate::geo2d::{Point2, Region};
use crate::geometry::Point3;
use crate::links::Links;

use super::face::{GenError, band};
use super::levels::{Cert, Motion, cut_levels, run_level_walks};
use super::arcs::{ARC_FIT_TOL, Seg, fit_arcs};
use super::arc_cross::post_crossing_free;
use super::arc_proof::{ProofTally, fit_proved};
use super::fillet::{Legal, fillet_corners};
use super::fillet_tally::{FilletTally, corner_arc_radius, fillet_floor};
use super::waterscan::{scan_row, waterline_passes};
use super::slots::chord_clears;
use super::pocket_hsm::adaptive_passes;
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
    pub(super) fn validate(&self) -> Result<(), GenError> {
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
    fn tiling_step(&self) -> f64 {
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
/// kept as a Feed exactly when [`chord_clears`] proves it never leaves
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
    p.validate()?;
    if region.is_empty() {
        return Err(GenError::Empty("empty pocket region".into()));
    }
    let center = region.offset(-(p.tool_diameter * 0.5 + p.stock_to_leave))?;
    if center.is_empty() {
        // Material exists that no legal tool position can reach.
        return Err(GenError::Invalid("pocket is narrower than the tool".into()));
    }
    pocket_over(region, &center, p, Vec::new(), false, holes)
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
pub(super) fn pocket_over(
    region: &Region,
    center: &Region,
    p: &PocketParams,
    notes: Vec<String>,
    hold_crossings: bool,
    holes: &[PreHole],
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
    // The HSM plan may choose where its entry stands (`pocket_hsm`).
    let (passes, strategy, room, joins) = match strategy {
        PocketStrategy::Adaptive => {
            let (passes, notes, s, room, joins) = adaptive_passes(center, p, "this pocket", true)?;
            notes.into_iter().for_each(|n| b.note(n));
            (passes, s, room, joins)
        }
        s => (plan_passes(center, p.tiling_step(), p.climb, s)?, s, None, Vec::new()),
    };
    // The horizontal arc entry (M11.3) lands on the first pass: a closed
    // first ring starts mid its longest edge, where an arc has room.
    let passes = if p.links.approach.arc_xy && room.is_none() { super::descent_xy::reseat_first(passes) } else { passes };
    // #1449: a proved hole re-seats the plan on itself (not the HSM plan's own bore).
    let passes = super::predrilled::reseat(passes, holes, center, p.links.approach.predrilled && room.is_none());
    let zs = cut_levels(p.top_z, p.bottom_z, p.stepdown)?;
    // Waterline rings circle around material, so they get the arc fitter
    // (rings straight enough to be lines stay exactly that); zigzag rows
    // are straight by construction and keep the plain walk.
    // The HSM plan's chords are fitted at the ring budget (a quarter of
    // the chord tolerance: the walk is already a tessellation) and every
    // arc is proved inside the center area or stays chords (`arc_proof`).
    let fit = match strategy {
        PocketStrategy::Zigzag | PocketStrategy::Auto => None,
        PocketStrategy::Waterline | PocketStrategy::OffsetRings => Some(ARC_FIT_TOL),
        PocketStrategy::Adaptive => Some(ARC_FIT_TOL * 0.25),
    };
    let mut proof = ProofTally::default();
    let mut fit_pass = |pass: &[Point2], tol: f64| -> Vec<Seg> {
        if strategy == PocketStrategy::Adaptive { fit_proved(pass, tol, center, &mut proof) } else { fit_arcs(pass, tol) }
    };
    // The HSM plan's entry may orbit a narrowed helix (`pocket_hsm`).
    let helix_r = room.as_ref().map_or(p.helix_radius, |e| e.helix_r);
    let motion = Motion::new(p.clearance_z, p.retract_z, p.feed, p.plunge_feed)
        .with_entry(p.entry, helix_r)
        .with_arc_radius(p.tool_diameter)
        .with_links(p.links, helix_r)
        .with_top(p.top_z)
        .with_xy_arc(p.links.approach.arc_xy)
        .with_microlift(joins)
        .with_holes(holes);
    // The walks: fitted where the strategy fits, then - opted in - the
    // corner fillets, proved inside the cutter-center area. Passes are
    // one plan at every level, so the plan's refusals are said once.
    let runs: Option<Vec<Vec<Seg>>> = if p.blend_turns {
        let legal = Legal { region: center, inside: true, air_left: None };
        let rho = corner_arc_radius(p.blend_radius, p.tool_diameter).map_err(GenError::Invalid)?;
        let mut tally = FilletTally::default();
        let runs = passes
            .iter()
            .map(|pass| {
                let Some((first, rest)) = pass.split_first() else {
                    return Vec::new();
                };
                let walk = match fit {
                    Some(tol) => fit_pass(pass, tol),
                    None => vec![Seg::Line(rest.to_vec())],
                };
                let (walk, t) = fillet_corners(&walk, *first, rho, ARC_FIT_TOL, &legal);
                tally.add(t);
                walk
            })
            .collect();
        for n in tally.notes("pass", fillet_floor(ARC_FIT_TOL)) {
            b.note(n);
        }
        Some(runs)
    } else {
        fit.map(|tol| passes.iter().map(|p| fit_pass(p, tol)).collect())
    };
    // The entry room: the plan's spot as a zero-length stadium (its disc
    // certifies the chords, its triangle is where `dive_room` finds it).
    // The entry and a "same as entry" level change are planned by the
    // shared descents (`descent::plan`: one entry per level, plunge
    // height, centre cutting) against it - the helix closes on the pass
    // start exactly around the spot the plan bored. Keep-down links and
    // a user-set transition are proved against the whole center area.
    let cert = match &room {
        Some(e) => Cert::Stadium { a: e.seed, b: e.seed, h: e.clear, tess: &e.tess },
        None => Cert::Region(center),
    };
    let post = |b: &mut PathBuilder, runs: Option<&[Vec<Seg>]>| {
        run_level_walks(b, &passes, &zs, &motion, runs, Some(&cert), Some(&Cert::Region(center)));
        let cur = b.at();
        b.push(MoveKind::Rapid, Point3::new(cur.x, cur.y, p.clearance_z), None);
    };
    // The HSM plan's arcs are held to the crossing law on the posted
    // path too (`arc_proof`): an arc that would cross another feed is
    // given up and the walks are posted again.
    let mut path = match runs {
        Some(runs) if strategy == PocketStrategy::Adaptive || hold_crossings => post_crossing_free(b, &passes, runs, &mut proof, post),
        runs => {
            post(&mut b, runs.as_deref());
            b.finish()
        }
    };
    if let Some(n) = proof.note() {
        path.notes.push(n);
    }
    if let Some(why) = crate::approach::refusal(&path) {
        // #261 part 3: the moves built so far ride the refusal, display-only.
        return Err(GenError::Invalid(why).with_ghost(&path));
    }
    if !path.all_finite() {
        return Err(GenError::Invalid("path left the finite horizon".into()));
    }
    Ok(path)
}

/// Serpentine rows across the cutter-center area, each scanline split
/// where the even-odd parity says material stands (islands fall out of
/// the interval walk for free). Rows band [lo.y, hi.y] at `stepover` with
/// the extremes kept: a row on the area's own boundary is the pass whose
/// edge grazes the wall, and dropping it would leave the wall's last band
/// standing.
pub(super) fn zigzag_passes(center: &Region, stepover: f64, climb: bool) -> Result<Vec<Vec<Point2>>, GenError> {
    let (lo, hi) = match center.bounds() {
        Some(b) => b,
        None => return Err(GenError::Empty("empty pocket region".into())),
    };
    let ys = band(lo.y, hi.y, 0.0, stepover)?;
    let mut passes: Vec<Vec<Point2>> = Vec::new();
    let mut cur: Vec<Point2> = Vec::new();
    for (i, &y) in ys.iter().enumerate() {
        let segs = scan_row(center, y, y == hi.y);
        let left_to_right = climb == (i % 2 == 0);
        for k in 0..segs.len() {
            let (x0, x1) = segs[if left_to_right { k } else { segs.len() - 1 - k }];
            let (s, e) = if left_to_right {
                (Point2::new(x0, y), Point2::new(x1, y))
            } else {
                (Point2::new(x1, y), Point2::new(x0, y))
            };
            let join = match cur.last() {
                Some(&prev) => chord_clears(center, prev, s),
                None => false,
            };
            if join {
                // The corner matters: the connection feeds prev -> s (the
                // tested chord) and only then along the row to e. Pushing
                // only e would cut prev -> e, a diagonal the join test
                // never proved.
                cur.push(s);
                cur.push(e);
            } else {
                if cur.len() >= 2 {
                    passes.push(std::mem::take(&mut cur));
                }
                cur = vec![s, e];
            }
        }
    }
    if cur.len() >= 2 {
        passes.push(cur);
    }
    Ok(passes)
}

/// Concentric rings marching inward: ring k is the cutter-center area
/// eroded by `k * stepover`, travelled closed. Offsets are taken from
/// `center` at absolute distances (never buffer-of-buffer), so micron
/// quantization cannot compound across rings.
/// The cutter-center passes for one planar area: the shape law both the
/// 2-D pocket and the 3-D pocket's per-level plan share (the strategies
/// band/erode `center` identically; only the numbers come from the
/// operation's params either way).
pub(crate) fn plan_passes(
    center: &Region,
    stepover: f64,
    climb: bool,
    strategy: PocketStrategy,
) -> Result<Vec<Vec<Point2>>, GenError> {
    match strategy {
        PocketStrategy::Zigzag | PocketStrategy::Auto => zigzag_passes(center, stepover, climb),
        // Offset rings are ring tiling at the caller's step; the HSM
        // plan is `pocket_hsm.rs`'s (the planar pocket's own, and the
        // 3-D ladder's per level through `machining3d::hsm`), which
        // hands a refused area back here as rings.
        PocketStrategy::Waterline | PocketStrategy::Adaptive | PocketStrategy::OffsetRings => {
            waterline_passes(center, stepover, climb)
        }
    }
}
