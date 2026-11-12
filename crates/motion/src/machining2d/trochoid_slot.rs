//! The trochoidal slot (issue #220): a channel of width `W` along a
//! centre chain cleared by trochoid loops whose engagement is MEASURED,
//! not assumed. Engagement angle `theta = theta_out - theta_in` on the
//! current stock, the step adapted until `theta <= limit` everywhere
//! (Otkur & Lazoglu 2007, IJMTM 47; Rauch, Duc & Hascoet 2009, IJMTM 49,
//! "Improving trochoidal tool paths generation and implementation using
//! process constraints modelling"; the maximum-engagement constraint of
//! the B-spline trochoid study, Int J Adv Manuf Technol 2022, doi
//! 10.1007/s00170-022-09527-z). The loop layout is `trochoid_loops`;
//! the stock and the reading are `quality`'s dexel yardstick (the same
//! measurement every Adaptive pin reads), run on the candidate level
//! before it is kept. Loop band `B = (W - D) / 2` by default. Ours, no
//! vendor material.
//!
//! Prime Directive: a channel no wider than the tool, a loop band wider
//! than the room, a limit no step down the ladder can hold, a loop that
//! would leave the channel, a crossing the verifier would hold, a
//! lifted return that would cut stock, a step so short the slot would
//! need more than the loop cap, a microlift that would feed above the
//! clearance plane and a bore helix past the turn cap are each refused
//! by name (#774).

use crate::cl::{MoveKind, PathBuilder, Toolpath, lattice};
use crate::geo2d::{Point2, Region};
use crate::geometry::Point3;
use crate::quality::{QualitySpec, measure, stock_for};

use super::face::GenError;
use super::levels::cut_levels;
use super::trochoid_air::lifted_cut;
use super::trochoid_loops::{Chain, El, Layout, MAX_LOOPS, contained, layout};

/// The forward step is tried at these shares of the user's, longest
/// first, until the measured engagement holds.
const LADDER: [f64; 11] = [1.0, 0.85, 0.7, 0.55, 0.45, 0.35, 0.28, 0.22, 0.15, 0.1, 0.07];

/// A loop band narrower than this share of the tool diameter is no
/// trochoid (the loops would be a slot with extra steps).
const MIN_BAND_PER_D: f64 = 0.02;

/// Loops a candidate step is first tried on.
const TRIAL_LOOPS: usize = 12;

/// How the tool gets from one loop's end back to the next loop's start.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum TrochoidReturn {
    /// Lifted `height` off the floor, retracing the front arc it just
    /// cut (its own swept air) at the return feed.
    Microlift { height: f64 },
    /// Up to the retract plane, across, down again.
    Retract,
}

/// The default microlift height (mm): the HSM planner's own lift.
pub const DEFAULT_MICROLIFT: f64 = 0.3;

/// The default engagement limit (degrees).
pub const DEFAULT_MAX_ENGAGEMENT: f64 = 90.0;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TrochoidParams {
    pub tool_diameter: f64,
    /// Width of the finished channel.
    pub width: f64,
    /// Width of the band the loop centres sweep; `None` is all the room
    /// the channel leaves (`W - D - 2 leave`).
    pub loop_width: Option<f64>,
    /// Forward step per loop (mm) - the most the planner may use.
    pub step: f64,
    /// Engagement limit (degrees of the cutter's periphery in contact).
    pub max_engagement: f64,
    pub ret: TrochoidReturn,
    /// Fillet radius from the wall lead into each front arc (0: none).
    pub smoothing: f64,
    pub stock_to_leave: f64,
    pub clearance_z: f64,
    pub retract_z: Option<f64>,
    pub top_z: f64,
    pub bottom_z: f64,
    pub stepdown: f64,
    /// Feed on the cutting arc, on the return, and into the material.
    pub feed: f64,
    pub return_feed: f64,
    pub plunge_feed: f64,
    pub climb: bool,
}

impl TrochoidParams {
    fn validate(&self) -> Result<(), GenError> {
        let mut nums = vec![
            self.tool_diameter,
            self.width,
            self.step,
            self.max_engagement,
            self.smoothing,
            self.stock_to_leave,
            self.clearance_z,
            self.top_z,
            self.bottom_z,
            self.stepdown,
            self.feed,
            self.return_feed,
            self.plunge_feed,
        ];
        nums.extend(self.retract_z);
        nums.extend(self.loop_width);
        if let TrochoidReturn::Microlift { height } = self.ret {
            nums.push(height);
        }
        let bad = |m: &str| Err(GenError::Invalid(format!("trochoidal slot: {m}")));
        if !nums.iter().all(|v| v.is_finite()) {
            return bad("parameters must be finite");
        }
        if self.tool_diameter <= 0.0 || self.stepdown <= 0.0 || self.step <= 0.0 {
            return bad("diameter, stepdown and forward step must be positive");
        }
        if self.feed <= 0.0 || self.return_feed <= 0.0 || self.plunge_feed <= 0.0 {
            return bad("feeds must be positive");
        }
        if !(self.max_engagement > 0.0 && self.max_engagement <= 180.0) {
            return bad("the engagement limit must lie in (0, 180] degrees");
        }
        if self.smoothing < 0.0 || self.stock_to_leave < 0.0 {
            return bad("smoothing radius and stock to leave must not be negative");
        }
        if let TrochoidReturn::Microlift { height } = self.ret
            && height <= 0.0
        {
            return bad("the microlift height must be positive");
        }
        if self.top_z < self.bottom_z {
            return bad("cut top below cut bottom");
        }
        if self.clearance_z < self.top_z {
            return bad("clearance plane below the first cut level");
        }
        // A lifted return is a feed move: it never runs above the
        // clearance plane, even lifted off the stock top (#774).
        if let TrochoidReturn::Microlift { height } = self.ret
            && height > self.clearance_z - self.top_z + 1e-9
        {
            return Err(GenError::Invalid(format!(
                "trochoidal slot: a {height:.3} mm microlift would feed the return above the clearance plane - at most {:.3} mm (the clearance above the top) fits; use the Retract return to hop higher",
                self.clearance_z - self.top_z
            )));
        }
        if self.retract_z.is_some_and(|rz| rz < self.top_z) {
            return bad("retract plane sits inside the material");
        }
        Ok(())
    }

    /// The loop band's half-width, or why the channel has none.
    fn band(&self) -> Result<f64, GenError> {
        let room = self.width - self.tool_diameter - 2.0 * self.stock_to_leave;
        if room <= MIN_BAND_PER_D * self.tool_diameter {
            return Err(GenError::Invalid(format!(
                "trochoidal slot: the channel ({:.3} mm, {:.3} mm left on each wall) is too narrow for a {:.3} mm tool to loop in - cut it as a plain Slot",
                self.width, self.stock_to_leave, self.tool_diameter
            )));
        }
        let w = self.loop_width.unwrap_or(room);
        if w > room + 1e-9 {
            return Err(GenError::Invalid(format!(
                "trochoidal slot: a loop width of {w:.3} mm leaves the channel - at most {room:.3} mm fits"
            )));
        }
        if w <= MIN_BAND_PER_D * self.tool_diameter {
            return Err(GenError::Invalid(format!("trochoidal slot: a loop width of {w:.3} mm is no loop")));
        }
        Ok(0.5 * w)
    }
}

/// What the planner kept: the path, the loop layout's numbers and the
/// engagement it measured.
#[derive(Clone, Debug)]
pub struct TrochoidPlan {
    pub path: Toolpath,
    /// The forward step used (mm) and the loop circle's radius.
    pub step: f64,
    pub loop_radius: f64,
    /// Front-arc loops per level.
    pub loops: usize,
    /// The worst contact angle the planner measured on one level.
    pub max_engagement: f64,
}

/// Cut the trochoidal slot along `chain` (see [`TrochoidParams`]).
pub fn trochoidal_slot(chain: &[Point2], p: &TrochoidParams) -> Result<Toolpath, GenError> {
    plan_trochoidal_slot(chain, p).map(|pl| pl.path)
}

/// [`trochoidal_slot`] with the planner's numbers.
pub fn plan_trochoidal_slot(chain: &[Point2], p: &TrochoidParams) -> Result<TrochoidPlan, GenError> {
    p.validate()?;
    let Some(ch) = Chain::new(chain) else {
        return Err(GenError::Invalid("trochoidal slot: the centre line needs two distinct, finite points".into()));
    };
    let b = p.band()?;
    let r = 0.5 * p.tool_diameter;
    let room = 0.5 * p.width - r - p.stock_to_leave;
    let channel = Region::sweep(&[chain.to_vec()], 0.5 * p.width - p.stock_to_leave)?;
    let spec = QualitySpec::new(p.tool_diameter);
    let stock = || {
        stock_for(&channel, 1.0, 0.0, p.tool_diameter, &spec)
            .map_err(|e| GenError::Invalid(format!("trochoidal slot: measuring stock: {e}")))
    };
    let gauge = |path: &Toolpath| -> Result<_, GenError> {
        measure(path, &mut stock()?, &spec)
            .map_err(|e| GenError::Invalid(format!("trochoidal slot: measuring stock: {e}")))
    };
    // One level at Z 0 under a 1 mm block of the channel, measured.
    let level = |lay: &Layout| -> Result<Toolpath, GenError> {
        let mut bld = PathBuilder::new(Point3::new(lay.entry.x, lay.entry.y, 1.0));
        emit_level(&mut bld, lay, p, 0.0, 1.0, false)?;
        Ok(bld.finish())
    };
    let mut best: Option<(f64, Point3, f64)> = None;
    // The longest step that needed more loops than the cap (every
    // shorter rung needs more still).
    let mut capped: Option<f64> = None;
    for f in LADDER {
        let step = p.step * f;
        let Some(lay) = layout(&ch, b, step, p.smoothing, p.climb) else {
            capped = capped.or(Some(step));
            continue;
        };
        contained(&ch, &lay, room)?;
        // The first loops first (the steady state comes within a few
        // loops; a failing step need not be walked to the end), then
        // the whole level.
        let head = Layout { pieces: lay.pieces.iter().take(TRIAL_LOOPS).cloned().collect(), ..lay.clone() };
        let full = lay.pieces.len() > TRIAL_LOOPS;
        let mut q = gauge(&level(&head)?)?;
        let mut trial = None;
        if q.max_deg <= p.max_engagement && full {
            let t = level(&lay)?;
            q = gauge(&t)?;
            trial = Some(t);
        }
        if best.is_none_or(|(d, _, _)| q.max_deg < d) {
            best = Some((q.max_deg, q.max_at, step));
        }
        if q.max_deg > p.max_engagement {
            continue;
        }
        // The lifted returns land on air, read inside the disc.
        let trial = match trial {
            Some(t) => t,
            None => level(&lay)?,
        };
        let lifted = lifted_cut(&trial, &mut stock()?, r)
            .map_err(|e| GenError::Invalid(format!("trochoidal slot: reading the lifted returns: {e}")))?;
        if let Some(at) = lifted {
            return Err(GenError::Invalid(format!(
                "trochoidal slot: a lifted return at ({:.3}, {:.3}) would cut stock",
                at.x, at.y
            )));
        }
        return finish(&lay, p, step, q.max_deg, b, room);
    }
    let cap = |s: f64| {
        format!(
            "a {s:.4} mm forward step along the {:.3} mm centre line needs more than the {MAX_LOOPS} loop cap",
            ch.len()
        )
    };
    match (best, capped) {
        (None, Some(s)) => Err(GenError::Invalid(format!(
            "trochoidal slot: the forward step is too small - {} (every shorter step down the ladder needs more); raise the forward step",
            cap(s)
        ))),
        (Some((deg, at, step)), capped) => Err(GenError::Invalid(format!(
            "trochoidal slot: no forward step holds the {:.1} deg engagement limit - the least, {:.1} deg at ({:.3}, {:.3}), was read at a {:.4} mm step{}",
            p.max_engagement,
            deg,
            at.x,
            at.y,
            step,
            capped.map_or(String::new(), |s| format!(" ({}, so no shorter step was tried)", cap(s)))
        ))),
        (None, None) => Err(GenError::Invalid("trochoidal slot: no forward step on the ladder could be laid out".into())),
    }
}

/// The kept plan over every level, said in the notes and held to the
/// verifier's crossing law.
fn finish(lay: &Layout, p: &TrochoidParams, step: f64, deg: f64, b: f64, room: f64) -> Result<TrochoidPlan, GenError> {
    let zs = cut_levels(p.top_z, p.bottom_z, p.stepdown)?;
    let hop = p.retract_z.unwrap_or(p.clearance_z);
    let mut bld = PathBuilder::approached(Point3::new(0.0, 0.0, p.clearance_z));
    bld.push(MoveKind::Rapid, Point3::new(lay.entry.x, lay.entry.y, p.clearance_z), None);
    let mut above = p.top_z;
    for (i, &z) in zs.iter().enumerate() {
        if i > 0 {
            let at = bld.at();
            bld.push(MoveKind::Rapid, Point3::new(at.x, at.y, hop), None);
            bld.push(MoveKind::Rapid, Point3::new(lay.entry.x, lay.entry.y, hop), None);
        }
        let from = (above + crate::approach::DEFAULT_PLUNGE_HEIGHT).min(hop).max(above);
        bld.push(MoveKind::Rapid, Point3::new(lay.entry.x, lay.entry.y, from), None);
        // #871: a part-way refusal hands back the levels so far.
        emit_level(&mut bld, lay, p, z, from, true).map_err(|e| e.with_ghost_moves(bld.moves()))?;
        above = z;
    }
    let at = bld.at();
    bld.push(MoveKind::Rapid, Point3::new(at.x, at.y, p.clearance_z), None);
    bld.note(format!(
        "trochoidal slot: {} loops of radius {:.3} mm a level at a {:.4} mm step, measured {:.1} deg engagement (limit {:.1} deg)",
        lay.loops, lay.rho, lay.spacing, deg, p.max_engagement
    ));
    if step < p.step - 1e-12 {
        bld.note(format!(
            "trochoidal slot: the forward step was cut from {:.4} to {:.4} mm to hold the engagement limit",
            p.step, step
        ));
    }
    if room - b > 1e-6 {
        bld.note(format!(
            "trochoidal slot: the loops stop {:.3} mm short of each wall - finish the walls with a contour",
            room - b
        ));
    }
    let path = bld.finish();
    if !path.all_finite() {
        return Err(GenError::Invalid("trochoidal slot: path left the finite horizon".into()));
    }
    if let Some([i, j]) = crate::verify::crossed_pairs(&path, super::ARC_FIT_TOL).first().copied() {
        let at = path.moves.get(i).map_or(Point3::new(0.0, 0.0, 0.0), |m| m.to);
        return Err(GenError::Invalid(format!(
            "trochoidal slot: the loops would cross themselves (moves {i} and {j}, near ({:.3}, {:.3}))",
            at.x, at.y
        ))
        .with_ghost(&path));
    }
    Ok(TrochoidPlan { path, step, loop_radius: lay.rho, loops: lay.loops, max_engagement: deg })
}

/// One level at `z`: the helix bore from `from`, the floor circle, then
/// each loop's wall lead and cut, the returns between them. `levels`
/// says whether a retract return may hop (the planner's single level
/// lifts in place instead, which reads the same stock).
fn emit_level(
    b: &mut PathBuilder,
    lay: &Layout,
    p: &TrochoidParams,
    z: f64,
    from: f64,
    levels: bool,
) -> Result<(), GenError> {
    let p3 = |q: Point2, zz: f64| Point3::new(lattice(q.x), lattice(q.y), lattice(zz));
    let ccw = lay.ccw;
    // Past the turn cap the bore is refused by name, never cut as one
    // turn standing in for the many the drop needs (#774).
    let Some(turns) = crate::links::helix_turns(from - z, lay.b, 0.0) else {
        return Err(GenError::Invalid(format!(
            "trochoidal slot: the bore helix would need more than {} turns to descend {:.3} mm on a {:.3} mm helix radius (half the loop band) - use a smaller stepdown or a wider loop band",
            crate::links::MAX_HELIX_TURNS,
            from - z,
            lay.b
        )));
    };
    let turns = turns.ceil().max(1.0);
    let halves = (2.0 * turns) as usize;
    for k in 1..=halves {
        let zz = from - (from - z) * k as f64 / halves as f64;
        let to = if k % 2 == 1 { lay.opposite } else { lay.entry };
        b.push_arc(MoveKind::Feed, p3(to, zz), Some(p.plunge_feed), lay.c0, ccw);
    }
    b.push_arc(MoveKind::Feed, p3(lay.opposite, z), Some(p.feed), lay.c0, ccw);
    b.push_arc(MoveKind::Feed, p3(lay.entry, z), Some(p.feed), lay.c0, ccw);
    let n = lay.pieces.len();
    for (i, pc) in lay.pieces.iter().enumerate() {
        b.push(MoveKind::Feed, p3(pc.start, z), Some(p.feed));
        for el in &pc.cut {
            let El::Arc { center, to, ccw } = *el;
            b.push_arc(MoveKind::Feed, p3(to, z), Some(p.feed), center, ccw);
        }
        if i + 1 == n {
            break;
        }
        let e = pc.end();
        match p.ret {
            TrochoidReturn::Microlift { height } => {
                let zl = z + height;
                b.push(MoveKind::Feed, p3(e, zl), Some(p.return_feed));
                for el in pc.retrace() {
                    let El::Arc { center, to, ccw } = el;
                    b.push_arc(MoveKind::Feed, p3(to, zl), Some(p.return_feed), center, ccw);
                }
                b.push(MoveKind::Feed, p3(pc.start, z), Some(p.plunge_feed));
            }
            TrochoidReturn::Retract => {
                let hop = if levels { p.retract_z.unwrap_or(p.clearance_z) } else { z + 0.5 };
                let near = (z + crate::approach::DEFAULT_PLUNGE_HEIGHT).min(hop);
                b.push(MoveKind::Rapid, p3(e, hop), None);
                b.push(MoveKind::Rapid, p3(pc.start, hop), None);
                b.push(MoveKind::Rapid, p3(pc.start, near), None);
                b.push(MoveKind::Feed, p3(pc.start, z), Some(p.plunge_feed));
            }
        }
    }
    Ok(())
}
