//! Issue #1950: the stock ring of an open-side level cut from the air
//! side inward - contour-parallel fronts of the stock outline (Held 1991,
//! "On the Computational Geometry of Pocket Machining", LNCS 500), one
//! load width apart, joined as one inward spiral while a front is a
//! closed loop (Held & Spielberger 2009, CAD 41(7)), each cut read on the
//! in-process stock before it is kept (Stori & Wright 2000, J. Manuf.
//! Systems 19(3)). First principles and that literature; no vendor
//! material or output consulted.

use crate::geo2d::{Point2, Region};
use crate::load::{deg_at_rim, load_at_rim};

use super::front::{dist, len};
use super::raster::Raster;
use super::{AdaptivePlan, AdaptiveSpec, GRID_PER_D, lift};
use crate::machining2d::GenError;

/// The entry plunge stands this far outside the disc that touches the
/// stock (mm): the first plunge is fully in air.
const AIR_GAP: f64 = 0.005;

/// Fronts are capped (the Prime Directive's "never hang").
const MAX_FRONTS: usize = 2000;

/// The lead-in keeps this far under the bound (degrees): the rim's
/// sampling step.
const LEAD_MARGIN_DEG: f64 = 1.0;

/// The lead-in starts at most this many load widths off its front ...
const LEAD_START_W: usize = 4;

/// ... reaches it within this many tool radii ...
const LEAD_PER_R: f64 = 12.0;

/// ... and tries this many gaps a step.
const LEAD_TRIES: usize = 16;

/// The lead-out leaves the front over this many load widths.
const LEAD_OUT_PER_W: f64 = 3.0;

/// Wall laps held off the centre outline before the lap on it ...
const WALL_LAPS: usize = 2;

/// ... each this share of a load width further off.
const WALL_LAP_SHARE: f64 = 0.9;

/// A run end is read over this many tool radii of its length.
const END_PER_R: f64 = 2.0;

/// The air-side plan and its readings.
#[derive(Clone, Debug)]
pub(crate) struct AirPlan {
    pub plan: AdaptivePlan,
    /// Worst planned load (ae/D) over the whole plan, and where.
    pub worst: (f64, Point2),
    /// Worst planned load within `END_PER_R` radii of a run start or end
    /// where a front breaks into runs at the part wall, and where.
    pub ends: (f64, Point2),
    /// Fronts cut as closed spiral loops, and fronts cut as open runs.
    pub fronts: (usize, usize),
    /// The bound less the worst reading (degrees of rim; >= 0 within it).
    pub slack_deg: f64,
    /// Why the plan is over the load bound (`None`: within it). The law
    /// of `lift::keeps`: readings are whole rim samples, so the compare
    /// is in degrees with `1e-9` float slack - a reading equal to the
    /// bound passes, one sample over is refused.
    pub over: Option<String>,
}

/// Plan the material `mat` (already clipped to `stock`) from the air
/// outside `stock`, the tool centre kept in `center`. `None`: the piece
/// has no air side, or its first front is not one closed loop in
/// `center` (the caller keeps the growth spiral).
pub(crate) fn plan_from_air(mat: &Region, stock: &Region, center: &Region, s: &AdaptiveSpec) -> Result<Option<AirPlan>, GenError> {
    if !(s.r > 0.0 && s.load > 0.0 && s.load < 1.0 && s.tol > -0.5) {
        return Err(GenError::Invalid("air-side plan needs positive radius and load".into()));
    }
    let r = s.r;
    if mat.is_empty() || center.difference(stock)?.area() <= r * r * 1e-3 {
        return Ok(None);
    }
    let (d, w, seg) = (2.0 * r, 2.0 * r * s.load, r / 8.0);
    let mut ras = Raster::from_region(mat, d / GRID_PER_D, 3.0 * r)?;
    ras.open_air(&stock.offset(4.0 * r)?.difference(stock)?);
    let ras0 = ras.clone();
    let rim = crate::quality::rim_for(d, ras.h);
    let read = (r, rim, r / 10.0);
    let bite = deg_at_rim(super::BITE_LOAD, d, rim);
    let mut passes: Vec<Vec<Point2>> = Vec::new();
    let mut worst = (0.0f64, s.start);
    let mut ends = (0.0f64, s.start);
    let mut spiral: Vec<Point2> = Vec::new();
    let (mut closed, mut open) = (0usize, 0usize);
    let mut last_closed: Option<Vec<Point2>> = None;
    let mut fronts: Vec<Vec<Vec<Point2>>> = Vec::new();
    // No pass crosses an earlier one (the plan's crossing law): each is
    // cut past its crossing chords before it is read (`air_uncross`).
    let mut kept = super::air_uncross::Kept::default();
    let bound = deg_at_rim(s.load * (1.0 + s.tol.max(0.0)), d, rim);
    for k in 1..=MAX_FRONTS {
        let front = stock.offset(r - k as f64 * w)?;
        if front.is_empty() {
            break;
        }
        let mut runs = Vec::new();
        let mut whole = true;
        for l in front.loops() {
            let ring = climb_ring(l.points(), seg, s.climb);
            let inside: Vec<bool> = ring.iter().map(|&p| center.contains(p)).collect();
            whole &= inside.iter().all(|&b| b);
            runs.extend(super::air_chain::split_runs(&ring, &inside));
        }
        if whole && fronts.is_empty() && front.loops().len() == 1 {
            // One closed loop: ramp onto it from the last front (the
            // first from the air), one continuous inward spiral.
            let ring = climb_ring(front.loops()[0].points(), seg, s.climb);
            let from = spiral.last().copied().unwrap_or(ring[0]);
            let ramp = ramp_onto(&ring, from, if k == 1 { w + AIR_GAP } else { w }, s.climb);
            if k == 1 && !(center.contains(ramp[0]) && ras.disc_clear(ramp[0], r)) {
                return Ok(None);
            }
            spiral.extend(ramp);
            last_closed = Some(ring);
            closed += 1;
            continue;
        }
        if k == 1 {
            return Ok(None);
        }
        if let Some(ring) = last_closed.take() {
            // The ramp left a wedge at its start: one closing lap.
            let at = nearest(&ring, spiral.last().copied().unwrap_or(ring[0]));
            spiral.extend(ring[at..].iter().chain(&ring[1..=at]).copied());
            for piece in kept.split(&std::mem::take(&mut spiral)) {
                note(&mut worst, ras.walk(&piece, read.0, read.1, read.2));
                kept.add(&piece);
                passes.push(piece);
            }
        }
        fronts.push(runs);
    }
    for piece in kept.split(&spiral) {
        note(&mut worst, ras.walk(&piece, read.0, read.1, read.2));
        kept.add(&piece);
        passes.push(piece);
    }
    // The open fronts region by region: each run reached from the last of
    // its chain in the material plane where the link reads within the
    // bound (`air_chain`), so only the move between regions is a hop.
    for chain in super::air_chain::chains(fronts, 3.0 * r) {
        let mut tail: Option<usize> = None;
        for run in chain.into_iter().filter(|run| run.len() >= 2) {
            // Lead in from the cleared side (the last front's level) on
            // the steepest slope the load allows, lead out away from it.
            // A run that goes on its chain leads in from the run just cut:
            // back along that path to where the run start's normal meets
            // it, then down onto the run (`air_chain::onto`).
            let prev = tail.and_then(|t| passes.last().and_then(|l| l.get(t..)));
            let n0 = normal(&run, 0, s.climb);
            let on = prev.and_then(|p| super::air_chain::onto(p, run[0], n0, LEAD_START_W as f64 * w).map(|j| (p, j)));
            let (pass, mut planned) = match on {
                Some((p, j)) => {
                    let g = (p[j].x - run[0].x) * n0.x + (p[j].y - run[0].y) * n0.y;
                    let back: Vec<Point2> = p.get(j + 1..p.len() - 1).map(|b| b.iter().rev().copied().collect()).unwrap_or_default();
                    (leads(&ras, &run, w, center, s.climb, (r, rim, bound), Some((p[j], g))), Some(back))
                }
                None => (leads(&ras, &run, w, center, s.climb, (r, rim, bound), None), None),
            };
            let start = pass.first().copied();
            let pieces = kept.split(&pass);
            open += usize::from(!pieces.is_empty());
            for (pi, pass) in pieces.into_iter().enumerate() {
                let prev = tail.and_then(|t| passes.last().and_then(|l| l.get(t..)));
                let joint = planned.take().filter(|_| pi == 0 && start == pass.first().copied());
                let joint = joint.or_else(|| prev.and_then(|p| super::air_chain::link(&ras, &kept, center, p, pass[0], (r, rim, bound - LEAD_MARGIN_DEG))));
                if let Some(joint) = &joint {
                    note(&mut worst, ras.walk(&[&joint[..], &pass[..1]].concat(), read.0, read.1, read.2));
                    kept.add(joint);
                }
                // Where a piece starts the tool stands in the stock it
                // plunges into: read as the straight wall of that depth.
                let sunk = super::air_chain::plunge_depth(&ras, pass[0], r);
                if sunk > 0.0 {
                    note(&mut ends, (deg_at_rim(sunk / d, d, rim), pass[0]));
                }
                walk_ends(&mut ras, &pass, read, END_PER_R * r, (&mut worst, &mut ends));
                kept.add(&pass);
                match (joint, passes.last_mut()) {
                    (Some(joint), Some(last)) => {
                        let before = last.len() + joint.len();
                        last.extend(joint);
                        if let Some(&p) = last.last() {
                            kept.add(&[p, pass[0]]);
                        }
                        tail = Some(before);
                        last.extend(pass);
                    }
                    _ => {
                        tail = Some(0);
                        passes.push(pass);
                    }
                }
            }
        }
    }
    if closed == 0 {
        return Ok(None);
    }
    // The wall laps: the centre outline where the disc still bites (the
    // cusps the fronts leave where they meet the part wall), first held
    // off it by whole load widths, outermost first, then on it.
    for j in (0..=WALL_LAPS).rev() {
        let lap = if j == 0 { center.clone() } else { center.offset(-(j as f64) * WALL_LAP_SHARE * w)? };
        for l in lap.loops() {
            // A centre loop has the part on its right: climb as it is.
            let ring = climb_ring(l.points(), seg, !s.climb);
            let bites: Vec<bool> = ring.iter().map(|&p| !ras.disc_clear(p, r)).collect();
            for whole in super::air_chain::split_runs(&ring, &super::air_chain::grow_marks(&bites)) {
                for run in kept.split(&whole) {
                    let mut wall_ends = (0.0, s.start);
                    walk_ends(&mut ras, &run, read, END_PER_R * r, (&mut worst, &mut wall_ends));
                    kept.add(&run);
                    passes.push(run);
                }
            }
        }
    }
    // Each pass reached by the microlift return law, else a hop.
    // A pass whose lead grazes starts on it: the walk in order is read
    // again and kept in the worst reading.
    let joined = super::air_links::air_joins(&ras0, &passes, center, (r, rim, read.2, bite));
    note(&mut worst, joined.worst);
    let (passes, joins) = (joined.passes, joined.joins);
    let mut plan = AdaptivePlan { rest: ras.standing() as f64 * ras.h * ras.h, ..AdaptivePlan::default() };
    plan.travel = lift::travel(&ras0, &passes, &joins, (r, rim, read.2, bite));
    (plan.passes, plan.joins) = (passes, joins);
    let (wl, el) = (load_at_rim(worst.0, d, rim), load_at_rim(ends.0, d, rim));
    plan.notes.push(format!("adaptive: stock ring cut from the air in {closed} spiral and {open} open fronts"));
    // The run-start plunge readings are kept in `ends` only: the bound
    // holds for the higher of the two.
    let top = if ends.0 > worst.0 { ends } else { worst };
    let mut over = None;
    if top.0 > bound + 1e-9 {
        let tl = load_at_rim(top.0, d, rim);
        over = Some(format!(
            "would cut {:.1}% of D at ({:.2}, {:.2}), over the {:.1}% optimal load by {:.0}%",
            tl * 100.0,
            top.1.x,
            top.1.y,
            s.load * 100.0,
            (tl / s.load - 1.0) * 100.0
        ));
    }
    let slack_deg = bound - top.0;
    Ok(Some(AirPlan { plan, worst: (wl, worst.1), ends: (el, ends.1), fronts: (closed, open), slack_deg, over }))
}

fn note(worst: &mut (f64, Point2), w: (f64, Point2)) {
    if w.0 > worst.0 {
        *worst = w;
    }
}

/// Walk `pass` on the stock, its first and last `span` mm read apart as
/// the run ends.
fn walk_ends(ras: &mut Raster, pass: &[Point2], read: (f64, f64, f64), span: f64, (worst, ends): (&mut (f64, Point2), &mut (f64, Point2))) {
    let total = len(pass);
    let (mut acc, mut cut) = (0.0, vec![0usize]);
    for i in 1..pass.len() {
        acc += dist(pass[i - 1], pass[i]);
        if (cut.len() == 1 && acc >= span.min(0.5 * total)) || (cut.len() == 2 && acc >= total - span.min(0.5 * total)) {
            cut.push(i);
        }
    }
    cut.push(pass.len() - 1);
    for (j, win) in cut.windows(2).enumerate() {
        let Some(part) = pass.get(win[0]..=win[1]) else { continue };
        let wk = ras.walk(part, read.0, read.1, read.2);
        note(worst, wk);
        if j == 0 || j + 2 == cut.len() {
            note(ends, wk);
        }
    }
}

/// The loop as a closed climb walk (material on the right: a region's
/// loops reversed), at most `seg` apart, its first point repeated.
fn climb_ring(pts: &[Point2], seg: f64, climb: bool) -> Vec<Point2> {
    let mut v: Vec<Point2> = pts.to_vec();
    if climb {
        v.reverse();
    }
    super::front::densify(&v, seg, true)
}

/// The unit normal toward the cleared side (left of a climb walk, right
/// of a conventional one) at point `i`.
fn normal(pts: &[Point2], i: usize, climb: bool) -> Point2 {
    let a = pts[i.saturating_sub(1)];
    let b = pts[(i + 1).min(pts.len() - 1)];
    let (tx, ty) = (b.x - a.x, b.y - a.y);
    let l = tx.hypot(ty).max(1e-12);
    if climb { Point2::new(-ty / l, tx / l) } else { Point2::new(ty / l, -tx / l) }
}

fn nearest(ring: &[Point2], p: Point2) -> usize {
    let n = ring.len().saturating_sub(1).max(1);
    (0..n).min_by(|&i, &j| dist(ring[i], p).total_cmp(&dist(ring[j], p))).unwrap_or(0)
}

/// One revolution of `ring` from the point nearest `from`, each point
/// held off toward the cleared side by `gap` at the start, falling to 0
/// at the end (the step between revolutions spread over the whole one).
fn ramp_onto(ring: &[Point2], from: Point2, gap: f64, climb: bool) -> Vec<Point2> {
    let at = nearest(ring, from);
    let walk: Vec<Point2> = ring[at..].iter().chain(&ring[1..=at]).copied().collect();
    let total = len(&walk).max(1e-12);
    let mut acc = 0.0;
    let mut out = Vec::with_capacity(walk.len());
    for i in 0..walk.len() {
        if i > 0 {
            acc += dist(walk[i - 1], walk[i]);
        }
        let n = normal(&walk, i, climb);
        let g = gap * (1.0 - acc / total);
        out.push(Point2::new(walk[i].x + n.x * g, walk[i].y + n.y * g));
    }
    out
}

/// `run` with its lead-in and a lead-out of `LEAD_OUT_PER_W` widths,
/// each point held off toward the cleared side by a gap that stays in
/// `center`. The lead-in starts where the disc stands clear (at most
/// `LEAD_START_W` widths off), and each step drops the gap as far as the
/// contact angle read on the stock allows (`bound`, degrees): an oblique
/// entry reads its slope on top of the wall it meets (the engagement arc
/// of peripheral milling, first principles; measured, not assumed).
fn leads(ras: &Raster, run: &[Point2], w: f64, center: &Region, climb: bool, (r, rim, bound): (f64, f64, f64), from: Option<(Point2, f64)>) -> Vec<Point2> {
    let off = |i: usize, g: f64| {
        let n = normal(run, i, climb);
        let q = Point2::new(run[i].x + n.x * g, run[i].y + n.y * g);
        (g > 0.0 && center.contains(q)).then_some(q)
    };
    let (first, mut g) = from.unwrap_or_else(|| {
        let mut g = w;
        for k in 2..=(2 * LEAD_START_W) {
            if off(0, g).is_some_and(|q| ras.disc_clear(q, r)) {
                break;
            }
            g = 0.5 * k as f64 * w;
        }
        (off(0, g).unwrap_or(run[0]), g)
    });
    let mut stock = ras.clone();
    let mut pass = vec![first];
    stock.cut(pass[0], pass[0], r);
    let mut i = 1;
    while i < run.len() && g > 0.0 && len(&run[..=i]) < LEAD_PER_R * r {
        let prev = pass[pass.len() - 1];
        let ds = dist(run[i - 1], run[i]);
        // The lowest gap whose step reads within the bound (else the
        // least read), from a little up to steeply down.
        let mut best: Option<(f64, f64, Point2)> = None;
        for j in 0..=LEAD_TRIES {
            let gj = (g + ds * (0.2 - 1.0 * j as f64 / LEAD_TRIES as f64)).max(0.0);
            let q = if gj > 0.0 { off(i, gj) } else { Some(run[i]) };
            let Some(q) = q else { continue };
            let deg = stock.peek(&[prev, q], rim, 0.1 * r);
            let fits = deg <= bound - LEAD_MARGIN_DEG;
            if best.is_none_or(|(bd, bg, _)| (fits && (gj < bg || bd > bound - LEAD_MARGIN_DEG)) || (!fits && bd > bound - LEAD_MARGIN_DEG && deg < bd)) {
                best = Some((deg, gj, q));
            }
        }
        let Some((bd, gj, q)) = best else { break };
        if bd > bound - LEAD_MARGIN_DEG {
            // No gap holds the load here (a short run into the wall):
            // the run ends, its rest is the wall laps'.
            return pass;
        }
        stock.cut(prev, q, r);
        (g, i) = (gj, i + 1);
        pass.push(q);
    }
    let total = len(run);
    let out_len = LEAD_OUT_PER_W * w;
    let mut acc = len(&run[..i.min(run.len())]);
    for j in i..run.len() {
        if j > i {
            acc += dist(run[j - 1], run[j]);
        }
        let rest = total - acc;
        let gj = if rest < out_len { w * (1.0 - rest / out_len) } else { 0.0 };
        pass.push(off(j, gj).unwrap_or(run[j]));
    }
    pass
}

#[cfg(test)]
#[path = "air_rings_tests.rs"]
mod tests;
