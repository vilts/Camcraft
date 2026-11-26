//! Split from `finishing3d.rs` by M5.0.1; structure only.

use crate::cl::{lattice, MoveKind, PathBuilder};
use crate::geo2d::Point2;
use crate::geometry::Point3;

use super::params::{Finish3DParams, GapLink};
use super::grid::SupSet;
use super::support::{support_at};
/// Append a sample to the piece being marched. Runs are walked once;
/// only the zero-length duplicate a finished bisection can leave
/// behind is refused. The stored point rides the micron lattice
/// (M4.2b): every coordinate the march answers from, and the
/// duplicate test itself, then read bits no platform's libm can
/// wobble - a ULP apart sample pair is one point on glibc and the
/// Windows CRT alike. The reported path is unchanged: the lattice
/// is exactly what the golden writer rounds to.
pub(super) fn push_run_pt(line: &mut Vec<(Point2, f64)>, q: Point2, c: f64) {
    let q = Point2::new(lattice(q.x), lattice(q.y));
    if line.last().is_none_or(|(lq, _)| lq.x != q.x || lq.y != q.y) {
        line.push((q, c));
    }
}

/// The march laws, stated once for every Z-following curve family -
/// raster lines, radial chords, the spiral (M2.2d). Samples ride a
/// parameterized curve `pos(t)` from `t0` to `t1`; `unit(t)` is the
/// parameter advance that carries ONE millimeter of XY ground
/// (1.0 for lines, 1/radius for the spiral); `dist(a, b)` is the
/// true XY distance between two parameters - the break law and the
/// step arithmetic speak millimeters, not parameter, so a curve
/// that parameterizes unevenly still promises what a line does.
/// The three refinements that make the march honest: validity
/// bisection at a run's ends, so a plunge starts just INSIDE the
/// material's edge rather than a hair outside it, hanging the ball
/// over the edge to dig; chord bisection, so the sample string over
/// tessellated ridges sinks at most `eps_chord` of center - the
/// same budget split the ring's scan law makes; and the
/// discontinuity break, so a curve whose next sample stands on a
/// DIFFERENT surface (the center leapt more than XY-span + tol)
/// breaks there instead of dragging one move's chord through the
/// shelf's edge - no cut move ever spans two surfaces. Returns the
/// curve's pieces, in travel order, air stretches gone.
#[allow(clippy::too_many_arguments)] // the march kernel: one slot per law, every caller fills all ten
pub(super) fn march_curve(
    t0: f64,
    t1: f64,
    pos: &impl Fn(f64) -> Point2,
    dist: &impl Fn(f64, f64) -> f64,
    unit: &impl Fn(f64) -> f64,
    sup: &impl Fn(Point2) -> Option<f64>,
    delta0: f64,
    tol: f64,
    eps_chord: f64,
    floor_mm: f64,
) -> Vec<Vec<(Point2, f64)>> {
    march_curve_seek(t0, t1, pos, dist, unit, sup, delta0, tol, eps_chord, floor_mm, tol)
}

/// [`march_curve`] with the skip phase's first stride at the cruise
/// `4·delta0` - the blind probe every family ran before M13.9. The
/// spiral family keeps it (pattern-gated, as the slope ladder is
/// scoped to Waterline/Morph): on the hill the stride ladder found an
/// inner-turn piece the blind probe had dropped, and the column it
/// exposed reads 0.156 against the spiral's 0.144 pin - the ratchet
/// never rises. (The fold-annulus law, #36's edge contact, has since
/// landed; moving the spiral onto the ladder is a separate change.)
#[allow(clippy::too_many_arguments)]
pub(super) fn march_curve_blind(
    t0: f64,
    t1: f64,
    pos: &impl Fn(f64) -> Point2,
    dist: &impl Fn(f64, f64) -> f64,
    unit: &impl Fn(f64) -> f64,
    sup: &impl Fn(Point2) -> Option<f64>,
    delta0: f64,
    tol: f64,
    eps_chord: f64,
    floor_mm: f64,
) -> Vec<Vec<(Point2, f64)>> {
    march_curve_seek(t0, t1, pos, dist, unit, sup, delta0, tol, eps_chord, floor_mm, 4.0 * delta0)
}

#[allow(clippy::too_many_arguments)]
fn march_curve_seek(
    t0: f64,
    t1: f64,
    pos: &impl Fn(f64) -> Point2,
    dist: &impl Fn(f64, f64) -> f64,
    unit: &impl Fn(f64) -> f64,
    sup: &impl Fn(Point2) -> Option<f64>,
    delta0: f64,
    tol: f64,
    eps_chord: f64,
    floor_mm: f64,
    seek: f64,
) -> Vec<Vec<(Point2, f64)>> {
    const MAX_BISECT: usize = 24;
    // The refinement floor (M2.2e): the march's own right to stop
    // bisecting, raised by an explicit minimum step. This floors
    // REFINEMENT only - the run-edge bisections below find where
    // material starts and stops, and that is the model's answer,
    // never the spacing parameter's.
    let delta_min = (delta0 / 4096.0).max(floor_mm);
    // M4.2b: the surface is asked only at lattice points. A sample
    // position arrives through libm (the radial chord's cos/sin, the
    // loop's hypot-summed arc length), so one ULP of platform drift
    // used to be enough to move a `dev > eps_chord` verdict - the
    // dome goldens' 3-versus-2-move drift. Quantized inputs make the
    // tangent query's mul/add/sqrt chain speak identical bits on
    // glibc and the Windows CRT, and the halving ladder - the
    // march's only libm-adjacent decision - lands the same way
    // everywhere. Nothing reported changes: round-onto-the-lattice
    // is a fixpoint of the µm rounding the golden writer applies.
    let ask = |q: Point2| sup(Point2::new(lattice(q.x), lattice(q.y)));
    // The skip phase's stride ladder (M13.9): the search for the next
    // ground starts one tolerance past where ground ended and doubles
    // per empty sample up to the cruise `4·delta0`. A flat cruise
    // stride was the cos-law rasters' over-read: on tessellated ground
    // the strict tangency leaves a fold gap of `r·|Δm|` (0.2 mm on the
    // r20 dome) between every pair of facets, so a line is runs of a
    // facet's width with hairline gaps between - and a blind 4·delta0
    // probe landing in the next gap, or a bisection converging to a
    // LATER transition, dropped whole runs, leaving neighbouring lines
    // three stepovers apart at the crown (0.349 mm read as scallop).
    // Resetting to `tol` after every run end finds the gap's far edge
    // with a sample or two; open air still reaches cruise in a
    // handful of doublings, so an air stretch costs what it did.
    let cruise = 4.0 * delta0;
    let seek0 = seek.clamp(delta_min, cruise);
    let mut stride = seek0;
    let mut line: Vec<(Point2, f64)> = Vec::new();
    let mut pieces: Vec<Vec<(Point2, f64)>> = Vec::new();
    let mut t = t0;
    let mut cz = ask(pos(t));
    if let Some(c) = cz {
        push_run_pt(&mut line, pos(t), c);
    }
    'march: while t <= t1 {
        if cz.is_none() {
            // Skip phase: search for the next edge of material at the
            // stride ladder, then bisect the start of the run.
            let tn = (t + stride * unit(t)).min(t1);
            if tn <= t {
                break;
            }
            match ask(pos(tn)) {
                None => {
                    t = tn;
                    stride = (stride * 2.0).min(cruise);
                }
                Some(c) => {
                    // The stretch between was not cuttable ground -
                    // air or occlusion. Whatever stands here is a
                    // new run, not a continuation of the line:
                    // append-only would ride one lateral Feed
                    // across the gap, through whatever wall or
                    // shelf edge hides the ground. Flush the line.
                    if !line.is_empty() {
                        pieces.push(std::mem::take(&mut line));
                    }
                    let (mut lo, mut hic, mut hicz) = (t, tn, c); // lo invalid, hic valid
                    for _ in 0..MAX_BISECT {
                        let mid = (lo + hic) * 0.5;
                        match ask(pos(mid)) {
                            Some(cm) => {
                                hic = mid;
                                hicz = cm;
                            }
                            None => lo = mid,
                        }
                    }
                    push_run_pt(&mut line, pos(hic), hicz);
                    t = hic;
                    cz = Some(hicz);
                }
            }
        } else {
            // Valid phase: march inside material.
            let mut delta = delta0;
            loop {
                let tn = (t + delta * unit(t)).min(t1);
                // Two ways the parameter stops advancing: the step
                // clamps onto t1 (the spiral's inner end sits exactly
                // there, still valid ground), or float stagnation -
                // `t` grown so far past `t1` that `delta * unit(t)`
                // no longer moves it. Either way the march is over;
                // a plain `break` here would return to the outer
                // condition, which is still true at t == t1, and the
                // loop would spin forever re-clamping the same
                // sample. Break the MARCH, not the step.
                if tn <= t {
                    break 'march;
                }
                match ask(pos(tn)) {
                    None => {
                        // End of the run: bisect to its edge from
                        // the last known valid point.
                        let (mut lo, mut loc, mut hic) = (t, cz.unwrap_or(0.0), tn);
                        for _ in 0..MAX_BISECT {
                            let mid = (lo + hic) * 0.5;
                            match ask(pos(mid)) {
                                Some(cm) => {
                                    lo = mid;
                                    loc = cm;
                                }
                                None => hic = mid,
                            }
                        }
                        push_run_pt(&mut line, pos(lo), loc);
                        t = lo;
                        cz = None;
                        stride = seek0; // ground just ended: look close first
                        break;
                    }
                    Some(c) if (c - cz.unwrap_or(c)).abs() > dist(t, tn) + tol => {
                        // Surface discontinuity: the center leapt
                        // higher than the terrain could climb over
                        // the same XY span - the next sample stands
                        // on a different surface (a shelf's edge,
                        // the mouth above a hidden floor). One move
                        // across that gap would drag its chord
                        // through material, so the curve BREAKS
                        // here: this piece ends behind, a new one
                        // starts here, and the choreography hands
                        // over at the hop, the ball clear of both
                        // surfaces in between.
                        if !line.is_empty() {
                            pieces.push(std::mem::take(&mut line));
                        }
                        push_run_pt(&mut line, pos(tn), c);
                        t = tn;
                        cz = Some(c);
                    }
                    Some(c) if delta > delta_min => {
                        let cur = cz.unwrap_or(c);
                        let mid = (t + tn) * 0.5;
                        let dev = match ask(pos(mid)) {
                            Some(cm) => (cm - (cur + c) * 0.5).abs(),
                            None => f64::INFINITY, // straddles an edge: refine
                        };
                        if dev > eps_chord {
                            // The staircase halts AT the floor, not
                            // below it: a floor of 0.3 means no cut
                            // chord shorter than 0.3, even though
                            // the halving sequence would land on
                            // 0.274. Clamping (not skipping) keeps
                            // the march advancing.
                            delta = (delta * 0.5).max(delta_min);
                            continue;
                        }
                        push_run_pt(&mut line, pos(tn), c);
                        t = tn;
                        cz = Some(c);
                        delta = (delta * 2.0).min(delta0);
                    }
                    Some(c) => {
                        // Accepted at or below the floor (or below
                        // the march's own right): the chord stands,
                        // and the step grows back toward cruise -
                        // one halving away from the test's reach -
                        // so a floor never freezes the march at its
                        // own depth across ground that needs none
                        // of it. Growth is capped at delta0, and
                        // any fold ahead re-triggers the test arm.
                        push_run_pt(&mut line, pos(tn), c);
                        t = tn;
                        cz = Some(c);
                        delta = (delta * 2.0).min(delta0);
                    }
                }
                if t >= t1 {
                    break 'march;
                }
            }
        }
    }
    if !line.is_empty() {
        pieces.push(line);
    }
    pieces
}

/// The ladder's own choreography: curve to curve inside one pass
/// links straight when `gap_link`'s sweep certifies the link, else
/// hops at `hop` (the retract FEEDed up, so the envelope sees the
/// column it vacates); pass to pass climbs to clearance with
/// rapids, like a level change among rings; the very first entry
/// just travels over at its own height, like the first ring.
/// The probe cap for [`diagonal_ok`]: a diagonal whose XY span needs
/// more samples than this is not certified - it takes the hop
/// instead. Same honesty law as the solid-rest budgets: a claim too
/// wide to check is declined, never approximated.
pub(super) const RAMP_PROBE_MAX: i64 = 4096;

/// The ramp law in one question: may the tool feed in a straight line
/// from where it stands (`from`, tip) to the next run's start (`to`,
/// tip) without ever cutting deeper than the terrain promises? At
/// each probe the center height is `z_lin + r`; it must meet
/// `support_at`'s own floor - resting within `tol` of tangent is the
/// finish's whole promise - and the support must exist at all (open
/// air or a flank outside the region ends the diagonal). Endpoints
/// need no probe: both are run samples, legal by the march's own
/// construction. A zero-XY diagonal is a vertical feed between two
/// legal centers - linear z between them can only ride ABOVE a
/// constant support, so the loop proves it and no special case is
/// needed. Too long to certify within the probe cap: refused, hop.
pub(super) fn diagonal_ok(from: Point3, to: Point3, r: f64, p: &Finish3DParams, sups: &SupSet) -> bool {
    let d = (to.x - from.x).hypot(to.y - from.y);
    if !(d.is_finite() && to.x.is_finite() && to.y.is_finite() && to.z.is_finite()) {
        return false;
    }
    // The angle law (M2.2e): the chain's TRAVERSE descends no
    // steeper than the ramp angle - one in tan(angle) of drop per
    // mm of ground. A steeper link is a plunge wearing a diagonal
    // coat, and plunges belong to the run's first contact, not to
    // this chain. `d` at or under the tolerance barely traverses:
    // that is the vertical case already settled above (two legal
    // centers of one column), gate inert there by design. The `+
    // tol` slack makes the 45° default certify a full 1:1 climb
    // exactly - shallow ground's own slope limit, so the default
    // stands for what v17 ran. NaN tan (a nonsense angle) fails
    // every compare: refused, hop, never a silent pass.
    let dz = (to.z - from.z).abs();
    if d > p.tolerance && dz > d * p.ramp_angle.to_radians().tan() + p.tolerance {
        return false;
    }
    let eps = p.tolerance * 0.25;
    let n = (d / eps).ceil();
    if n.is_nan() || n > RAMP_PROBE_MAX as f64 {
        return false; // NaN and over-cap both land here, honestly
    }
    let n = n as i64;
    for k in 1..n {
        let t = k as f64 / n as f64;
        let qx = from.x + (to.x - from.x) * t;
        let qy = from.y + (to.y - from.y) * t;
        let cz = from.z + (to.z - from.z) * t + r;
        match support_at(qx, qy, r, p.bottom_z, sups) {
            Some(s) if cz >= s - p.tolerance => {}
            _ => return false,
        }
    }
    true
}

pub(super) fn emit_runs(
    b: &mut PathBuilder,
    p: &Finish3DParams,
    r: f64,
    sups: &SupSet,
    runs: &[Vec<(Point2, f64)>],
    continuing: bool,
) {
    let hop = p.retract_z.unwrap_or(p.clearance_z).min(p.clearance_z);
    let model_top = sups.iter().flat_map(|s| s.tri.iter().map(|v| v.z)).fold(f64::NEG_INFINITY, f64::max);
    for (i, run) in runs.iter().enumerate() {
        let (x0, y0) = (run[0].0.x, run[0].0.y);
        let z0 = run[0].1 - r;
        let cur = b.at();
        if i == 0 && !continuing {
            b.push(MoveKind::Rapid, Point3::new(x0, y0, cur.z.max(z0)), None);
        } else if i == 0 {
            let trav = p.clearance_z.max(z0);
            b.push(MoveKind::Rapid, Point3::new(cur.x, cur.y, trav), None);
            b.push(MoveKind::Rapid, Point3::new(x0, y0, trav), None);
        } else if p.ramp && diagonal_ok(cur, Point3::new(x0, y0, z0), r, p, sups) {
            // One diagonal Feed IS the entry: it lands exactly on the
            // run's first point, so the plunge below collapses to
            // nothing by push's own zero-length law.
            b.push(MoveKind::Feed, Point3::new(x0, y0, z0), Some(p.feed));
        } else if p.gap_link == GapLink::Straight
            && super::gaplink::straight_ok(cur, Point3::new(x0, y0, z0), r, sups)
        {
            // The smoothest link the facets certify: one straight
            // move at the run ends' own heights, no hop at all -
            // and the plunge below collapses to nothing the same
            // way. Where the sweep refuses, the plain hop below
            // stands, byte for byte as every older file ran.
            b.push(MoveKind::Feed, Point3::new(x0, y0, z0), Some(p.feed));
        } else {
            // The plain hop (#3627): never below the run end (the tool
            // rests on the part there), and across only at a plane over
            // every facet or one the exact sweep clears - else clearance.
            let trav = hop.max(z0).max(cur.z);
            let (a, c) = (Point3::new(cur.x, cur.y, trav), Point3::new(x0, y0, trav));
            let clear = trav >= model_top || super::gaplink::straight_ok(a, c, r, sups);
            let trav = if clear { trav } else { p.clearance_z.max(trav) };
            b.push(MoveKind::Feed, Point3::new(cur.x, cur.y, trav), Some(p.feed));
            b.push(MoveKind::Rapid, Point3::new(x0, y0, trav), None);
        }
        b.push(MoveKind::Plunge, Point3::new(x0, y0, z0), Some(p.plunge_feed));
        for (q, czz) in run {
            b.push(MoveKind::Feed, Point3::new(q.x, q.y, czz - r), Some(p.feed));
        }
    }
}
