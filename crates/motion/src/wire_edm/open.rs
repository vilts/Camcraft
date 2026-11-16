//! The wire's OPEN run (#1109, part of #290): a cut-off line, a slot or
//! a relief entered from the stock's edge. An open run encloses
//! nothing, so punch and die mean nothing; the kerf side is stated
//! instead as LEFT or RIGHT of the run's travel (its point order) -
//! never inferred, the same law as punch / die (#290 D8). The kept side
//! is the other one.
//!
//! - The wire-centre path is the run's one-sided parallel at the kerf
//!   offset (wire radius + spark gap), its joins mitred: where the wire
//!   rides outside a turn the kept corner stays sharp; where it rides
//!   inside one, the corner minimum holds as for a closed profile.
//! - No start hole: both ends of the run must reach (or pass) the
//!   stock's edge, and the path is extended along the run's own end
//!   directions until the wire stands the kerf offset plus the stated
//!   clearance off the stock - threaded there, in the air, and run out
//!   the same way at the far end (the wire leaves the cut; no return).
//! - Refused by name: a side of die / punch (no kerf side stated), no
//!   stock to read the edge from, an end that stops inside the stock, a
//!   run that never enters it, a run that crosses or doubles back on
//!   itself, several runs or a run mixed with closed profiles, a corner
//!   tighter than the wire, and a run-out that would cut the stock.
//!
//! Provenance: general wire EDM practice as the umbrella cites it
//! (#290: an open cut is threaded outside the work and runs off its
//! edge; the kerf is wire radius + gap per side; the inside-corner
//! minimum) and plain 2D geometry (one-sided polyline offset with
//! mitred joins, segment intersection). Our own code; no vendor post,
//! control manual or CAM product behaviour was consulted.

use crate::cl::{MoveKind, PathBuilder, Toolpath};
use crate::geo2d::{Loop, Point2, Region};
use crate::geometry::Point3;
use crate::machining2d::GenError;

use super::ring::{dist, pt_ring};
use super::{laws, WireParams, WireSide, MIN_LEAD};

/// How far (mm) inside the stock's edge a run's end may stand and still
/// count as reaching it: 1 µm, a picked edge drawn on the stock's face.
pub const EDGE_TOL: f64 = 1e-3;
/// The largest clearance (mm) an op may state - bounds a hand-edited
/// file, nothing a cut would ask.
pub const CLEARANCE_MAX: f64 = 1000.0;
/// The most points an open run may carry (the crossing test is a sweep).
const MAX_POINTS: usize = 100_000;

fn invalid(why: String) -> GenError {
    GenError::Invalid(format!("wire contour: {why}"))
}

fn at(why: String, q: Point2, z: f64) -> GenError {
    GenError::invalid_at(format!("wire contour: {why}"), Point3::new(q.x, q.y, z))
}

/// How deep `q` stands inside the stock: positive inside, negative
/// outside, the distance to the nearest edge either way.
fn depth(stock: &Region, q: Point2) -> f64 {
    let d = stock.loops().iter().map(|l| pt_ring(q, l.points())).fold(f64::INFINITY, f64::min);
    if stock.contains(q) {
        d
    } else {
        -d
    }
}

/// Cut the one open run in `chains` with the kerf on the stated side.
///
/// # Errors
/// Every refusal the module head lists, by name; none is a panic.
pub(super) fn wire_open(loops: &[Loop], chains: &[Vec<Point2>], p: &WireParams) -> Result<Toolpath, GenError> {
    let sign = match p.side {
        WireSide::Left => 1.0,
        WireSide::Right => -1.0,
        WireSide::Die | WireSide::Punch => {
            return Err(invalid(format!(
                "the profile is open and its side is {} - an open profile has no punch or die side; state the kerf side: left or right of the run's travel",
                p.side.word()
            )));
        }
    };
    if !loops.is_empty() || chains.len() != 1 {
        return Err(invalid(format!(
            "{} open run(s) and {} closed profile(s) are given - one operation cuts one open run from the stock's edge; give each its own operation",
            chains.len(),
            loops.len()
        )));
    }
    if !(p.clearance.is_finite() && (0.0..=CLEARANCE_MAX).contains(&p.clearance)) {
        return Err(invalid(format!(
            "the run-out clearance must be between 0 and {CLEARANCE_MAX} mm (it states {})",
            p.clearance
        )));
    }
    let Some(stock) = p.stock.as_ref().filter(|s| !s.is_empty()) else {
        return Err(invalid(
            "an open run is entered from the stock's edge, and the setup states no stock to read that edge from".into(),
        ));
    };
    let run = clean(chains.first().map_or(&[][..], Vec::as_slice))?;
    let z = p.plane_z;
    crossing(&run, z)?;
    reaches_edge(stock, &run, z)?;
    let k = p.kerf_offset();
    let ride = offset(&run, sign * k, z)?;
    let (Some(&first), Some(&last)) = (ride.first(), ride.last()) else {
        return Err(invalid("the open run lays down no cut".into()));
    };
    let target = k + p.clearance;
    let back = unit(run[0], run[1]);
    let fwd = unit(run[run.len() - 2], run[run.len() - 1]);
    let t0 = run_out(stock, first, Point2::new(-back.x, -back.y), target, k, z)?;
    let t1 = run_out(stock, last, fwd, target, k, z)?;
    let mut b = PathBuilder::new(Point3::new(t0.x, t0.y, z));
    let mut prev = t0;
    for q in ride.iter().copied().chain(std::iter::once(t1)) {
        if dist(prev, q) > MIN_LEAD {
            b.push(MoveKind::Feed, Point3::new(q.x, q.y, z), None);
            prev = q;
        }
    }
    let mut path = b.finish();
    if path.moves.is_empty() {
        return Err(invalid("the open run lays down no cut".into()));
    }
    let wire: Vec<Point2> = std::iter::once(t0).chain(ride.iter().copied()).chain(std::iter::once(t1)).collect();
    laws::cover_run(&run, false, &wire, false, p)?;
    laws::kerf(&path, &run, false, k, z)?;
    path.notes.extend(notes(p, t0, t1));
    Ok(path)
}

/// The run's points, finite, with coincident neighbours dropped.
fn clean(raw: &[Point2]) -> Result<Vec<Point2>, GenError> {
    if raw.len() > MAX_POINTS {
        return Err(invalid(format!("the open run has {} points - more than {MAX_POINTS}", raw.len())));
    }
    if raw.iter().any(|q| !(q.x.is_finite() && q.y.is_finite())) {
        return Err(invalid("the open run has a point that is not finite".into()));
    }
    let mut out: Vec<Point2> = Vec::with_capacity(raw.len());
    for &q in raw {
        if out.last().is_none_or(|l| dist(*l, q) > MIN_LEAD) {
            out.push(q);
        }
    }
    if out.len() < 2 {
        return Err(invalid("the open run has no length - there is nothing to cut along".into()));
    }
    Ok(out)
}

fn unit(a: Point2, b: Point2) -> Point2 {
    let l = dist(a, b);
    Point2::new((b.x - a.x) / l, (b.y - a.y) / l)
}

fn cross(o: Point2, a: Point2, b: Point2) -> f64 {
    (a.x - o.x) * (b.y - o.y) - (a.y - o.y) * (b.x - o.x)
}

/// Whether segments a-b and c-d touch (collinear overlaps included).
fn touch(a: Point2, b: Point2, c: Point2, d: Point2) -> bool {
    const E: f64 = 1e-12;
    let (d1, d2, d3, d4) = (cross(c, d, a), cross(c, d, b), cross(a, b, c), cross(a, b, d));
    let on = |p: Point2, q: Point2, r: Point2| {
        r.x >= p.x.min(q.x) - E && r.x <= p.x.max(q.x) + E && r.y >= p.y.min(q.y) - E && r.y <= p.y.max(q.y) + E
    };
    if ((d1 > E && d2 < -E) || (d1 < -E && d2 > E)) && ((d3 > E && d4 < -E) || (d3 < -E && d4 > E)) {
        return true;
    }
    (d1.abs() <= E && on(c, d, a))
        || (d2.abs() <= E && on(c, d, b))
        || (d3.abs() <= E && on(a, b, c))
        || (d4.abs() <= E && on(a, b, d))
}

/// A run that crosses or touches itself (other than at a shared vertex
/// of neighbouring segments) is refused: its kerf would cut its own
/// kept side. A sweep over the segments sorted by their left end.
fn crossing(run: &[Point2], z: f64) -> Result<(), GenError> {
    let n = run.len() - 1;
    let mut order: Vec<usize> = (0..n).collect();
    let lo = |i: usize| run[i].x.min(run[i + 1].x);
    let hi = |i: usize| run[i].x.max(run[i + 1].x);
    order.sort_by(|&a, &b| lo(a).total_cmp(&lo(b)));
    for (oi, &i) in order.iter().enumerate() {
        for &j in &order[oi + 1..] {
            if lo(j) > hi(i) {
                break;
            }
            if i.abs_diff(j) > 1 && touch(run[i], run[i + 1], run[j], run[j + 1]) {
                let q = run[i.min(j) + 1];
                return Err(at(
                    format!(
                        "the open run crosses itself (segments {} and {}, near X{:.3} Y{:.3}) - a wire path cannot cross its own kerf",
                        i.min(j),
                        i.max(j),
                        q.x,
                        q.y
                    ),
                    q,
                    z,
                ));
            }
        }
    }
    Ok(())
}

/// Both ends reach the stock's edge (on it or past it), and the run
/// enters the stock somewhere - an open run is threaded outside.
fn reaches_edge(stock: &Region, run: &[Point2], z: f64) -> Result<(), GenError> {
    let ends = [("start", run[0]), ("end", run[run.len() - 1])];
    for (name, q) in ends {
        let d = depth(stock, q);
        if d.is_nan() || d > EDGE_TOL {
            return Err(at(
                format!(
                    "the open run's {name} at X{:.3} Y{:.3} stops {d:.3} mm inside the stock - an open run has no start hole and must reach the stock's edge at both ends; extend it to the edge (or cut it as a closed profile from a start hole)",
                    q.x, q.y
                ),
                q,
                z,
            ));
        }
    }
    let enters = run.windows(2).any(|w| {
        (0..=16).any(|j| {
            let f = f64::from(j) / 16.0;
            depth(stock, Point2::new(w[0].x + (w[1].x - w[0].x) * f, w[0].y + (w[1].y - w[0].y) * f)) > EDGE_TOL
        })
    });
    if !enters {
        return Err(invalid("the open run never enters the stock - it cuts nothing".into()));
    }
    Ok(())
}

/// The run's one-sided parallel at signed offset `s` (positive: left of
/// travel), its joins mitred. A run that doubles back, or a turn the
/// wire's side folds over, is refused by name.
fn offset(run: &[Point2], s: f64, z: f64) -> Result<Vec<Point2>, GenError> {
    let n = run.len();
    let normal = |i: usize| {
        let u = unit(run[i], run[i + 1]);
        Point2::new(-u.y, u.x)
    };
    let mut out = Vec::with_capacity(n);
    let n0 = normal(0);
    out.push(Point2::new(run[0].x + s * n0.x, run[0].y + s * n0.y));
    for (i, &q) in run.iter().enumerate().take(n - 1).skip(1) {
        let (a, b) = (normal(i - 1), normal(i));
        let c = 1.0 + a.x * b.x + a.y * b.y;
        if !(c.is_finite() && c > 1e-3) {
            return Err(at(
                format!("the open run doubles back on itself at X{:.3} Y{:.3} - the wire cannot turn back along its own kerf", q.x, q.y),
                q,
                z,
            ));
        }
        out.push(Point2::new(q.x + s * (a.x + b.x) / c, q.y + s * (a.y + b.y) / c));
    }
    let nl = normal(n - 2);
    out.push(Point2::new(run[n - 1].x + s * nl.x, run[n - 1].y + s * nl.y));
    // The wire's side of a turn folds over itself when the turn is
    // tighter than the offset: the parallel then runs backwards.
    for i in 0..n - 1 {
        let (u, w) = (unit(run[i], run[i + 1]), (out[i + 1].x - out[i].x, out[i + 1].y - out[i].y));
        if u.x * w.0 + u.y * w.1 <= 0.0 {
            let q = run[i + 1];
            return Err(at(
                format!(
                    "a corner of the open run near X{:.3} Y{:.3} is tighter than the wire can follow on its {} side - the minimum inside radius is the kerf offset ({:.4} mm)",
                    q.x,
                    q.y,
                    if s > 0.0 { "left" } else { "right" },
                    s.abs()
                ),
                q,
                z,
            ));
        }
    }
    Ok(out)
}

/// Where the wire is threaded (or leaves): from the path's end `from`
/// along `dir` until it stands `target` (kerf offset + clearance) off
/// the stock. The run-out may not cut the stock deeper than the kerf
/// offset `k` it already stands in at the run's end.
fn run_out(stock: &Region, from: Point2, dir: Point2, target: f64, k: f64, z: f64) -> Result<Point2, GenError> {
    let along = |e: f64| Point2::new(from.x + dir.x * e, from.y + dir.y * e);
    let ok = |e: f64| -depth(stock, along(e)) >= target;
    let span = stock.bounds().map_or(0.0, |(a, b)| dist(a, b)) + 100.0 * target + 1.0;
    let steps = 8192u32;
    let step = span / f64::from(steps);
    let found = (0..=steps).map(|j| f64::from(j) * step).find(|&e| ok(e));
    let Some(hi) = found else {
        return Err(at(
            format!(
                "the open run's end near X{:.3} Y{:.3} never runs clear of the stock along its own direction - extend the run straight out past the edge",
                from.x, from.y
            ),
            from,
            z,
        ));
    };
    // The shortest run-out: bisect down from the first clear step.
    let (mut lo, mut hi) = ((hi - step).max(0.0), hi);
    if hi > 0.0 {
        for _ in 0..40 {
            let mid = 0.5 * (lo + hi);
            if ok(mid) {
                hi = mid;
            } else {
                lo = mid;
            }
        }
    }
    let samples = 256u32;
    for j in 0..=samples {
        let q = along(hi * f64::from(j) / f64::from(samples));
        if depth(stock, q) > k + EDGE_TOL {
            return Err(at(
                format!(
                    "the wire's run-out near X{:.3} Y{:.3} would cut into the stock past the run's end - extend the run straight out past the stock's edge",
                    q.x, q.y
                ),
                q,
                z,
            ));
        }
    }
    Ok(along(hi))
}

fn notes(p: &WireParams, t0: Point2, t1: Point2) -> Vec<String> {
    let cond = if p.condition.is_empty() { String::new() } else { format!(", condition {}", p.condition) };
    vec![
        format!(
            "wire EDM: open run, kerf on the {} of travel - the wire runs {:.4} mm off the line (wire radius {:.4} + spark gap {:.4}{cond}); kerf width {:.4} mm, the {} side kept",
            p.side.word(),
            p.kerf_offset(),
            p.wire_diameter * 0.5,
            p.gap,
            p.kerf_width(),
            if p.side == WireSide::Left { "right" } else { "left" }
        ),
        format!(
            "wire EDM: threaded outside the stock at X{:.3} Y{:.3} and run out at X{:.3} Y{:.3}, each {:.3} mm clear of the stock's edge past the kerf; no start hole, no return",
            t0.x, t0.y, t1.x, t1.y, p.clearance
        ),
        "wire EDM: a vertical cut - the moves state no pose (XY only) and no feed (the control's generator sets the rate); not verified here (no wire sweep against the part yet)".into(),
    ]
}
