//! #2561 (second review of #2705): the two cuts beside a tab against the
//! tab's core, not against "does it come nearer". Plane geometry: a tool
//! disc of diameter `D` at a point `q` of a cut that starts at the tab end
//! `E` takes, behind `E`, nothing that the end kerf (the disc at `E`) does
//! not take while `q` is in the disc of radius `D` about `E` and runs away
//! from `E`. From the first point where the cut leaves that disc, or turns
//! back toward `E`, every later point is read against the tab line without
//! its first and last `D / 2` (the parts that the two end kerfs take). A
//! return run beside the tab at a constant distance is read too.

use crate::geo2d::Point2;

use super::ring::dist;
use super::tab_bridge::{SLACK, pt_seg, seg_seg};

/// A walk that comes back toward the tab end by more than this (mm) has
/// turned back; less is chord noise of a flattened arc.
const TURN: f64 = 1e-3;

/// The smallest distance from the cut `path` to the core of the tab `line`
/// (`line[0]` is where the cut before the tab ends, the last point is where
/// the cut after it starts), read where the cut can take more than the end
/// kerf. `from_end`: `path` starts at the last point of `line` (the cut
/// after the tab). `to_start`: `path` ends at `line[0]` (the cut before it).
/// A NaN distance reads as a touch: the caller holds the loop.
pub(super) fn kerf_gap(path: &[Point2], line: &[Point2], from_end: bool, to_start: bool, d: f64) -> f64 {
    let (Some(&s), Some(&e)) = (line.first(), line.last()) else { return f64::INFINITY };
    if path.len() < 2 {
        return f64::INFINITY;
    }
    let total: f64 = path.windows(2).map(|w| dist(w[0], w[1])).sum();
    let a_e = if from_end { leave(path, e, d) } else { 0.0 };
    let a_s = if to_start {
        let back: Vec<Point2> = path.iter().rev().copied().collect();
        total - leave(&back, s, d)
    } else {
        total
    };
    let line_len: f64 = line.windows(2).map(|w| dist(w[0], w[1])).sum();
    let (lo, hi) = (0.5 * d, line_len - 0.5 * d);
    // Line pieces for each class of path piece: (near E, near S).
    let far = core(line, lo, hi, &[], d);
    let near_e = core(line, lo, hi, &[e], d);
    let near_s = core(line, lo, hi, &[s], d);
    let near_both = core(line, lo, hi, &[e, s], d);
    let mut best = f64::INFINITY;
    let mut run = 0.0;
    for w in path.windows(2) {
        let len = dist(w[0], w[1]);
        // Cut the piece where the walk leaves either end's disc.
        let mut ts = vec![0.0, 1.0];
        for a in [a_e, a_s] {
            if len > 0.0 && a > run && a < run + len {
                ts.push((a - run) / len);
            }
        }
        ts.sort_by(f64::total_cmp);
        for t in ts.windows(2) {
            let at = |u: f64| Point2::new(w[0].x + (w[1].x - w[0].x) * u, w[0].y + (w[1].y - w[0].y) * u);
            let mid = run + 0.5 * (t[0] + t[1]) * len;
            // A NaN arc compares false: the piece is read as far, the strict
            // class. A piece of no length on the mark (to a rounding) is near.
            let (ne, ns) = (from_end && mid <= a_e + SLACK, to_start && mid >= a_s - SLACK);
            let pieces = match (ne, ns) {
                (false, false) => &far,
                (true, false) => &near_e,
                (false, true) => &near_s,
                (true, true) => &near_both,
            };
            let (p0, p1) = (at(t[0]), at(t[1]));
            for &(a, b) in pieces {
                let g = seg_seg(a, b, p0, p1);
                best = best.min(if g.is_nan() { 0.0 } else { g });
            }
        }
        run += len;
    }
    if total.is_nan() || line_len.is_nan() { 0.0 } else { best }
}

/// The arc length along `path` (it starts at `end`) where it stops being
/// near `end`: its first point `d` from `end`, or the start of the first
/// piece that comes back toward `end`. NaN reads as "left at once".
fn leave(path: &[Point2], end: Point2, d: f64) -> f64 {
    let mut run = 0.0;
    let mut peak = path.first().map_or(0.0, |&q| dist(q, end));
    for w in path.windows(2) {
        let len = dist(w[0], w[1]);
        // NaN on either side of each test reads as "left here".
        let (g, lim) = (pt_seg(end, w[0], w[1]), peak - TURN);
        if g < lim || g.is_nan() || lim.is_nan() {
            return run;
        }
        let r1 = dist(w[1], end);
        if r1 >= d || r1.is_nan() || d.is_nan() {
            // Leaves on this piece: bisect for the point (the distance from
            // `end` along a segment that starts inside the disc).
            let at = |u: f64| Point2::new(w[0].x + (w[1].x - w[0].x) * u, w[0].y + (w[1].y - w[0].y) * u);
            let (mut lo, mut hi) = (0.0, 1.0);
            for _ in 0..40 {
                let mid = 0.5 * (lo + hi);
                if dist(at(mid), end) >= d { hi = mid } else { lo = mid }
            }
            return run + lo * len;
        }
        peak = peak.max(r1);
        run += len;
    }
    run
}

/// The pieces of `line` between arc `lo` and `hi` (from `line[0]`), less
/// the open discs of radius `r` about each point of `holes`.
fn core(line: &[Point2], lo: f64, hi: f64, holes: &[Point2], r: f64) -> Vec<(Point2, Point2)> {
    let mut out = Vec::new();
    let mut run = 0.0;
    for w in line.windows(2) {
        let (a, b) = (w[0], w[1]);
        let len = dist(a, b);
        let (t0, t1) = if len > 0.0 { (((lo - run) / len).max(0.0), ((hi - run) / len).min(1.0)) } else { (0.0, 0.0) };
        run += len;
        if t1 < t0 || t0.is_nan() || t1.is_nan() || len <= 0.0 {
            continue;
        }
        let mut spans = vec![(t0, t1)];
        for &c in holes {
            let Some((r0, r1)) = inside(a, b, c, r) else { continue };
            spans = spans
                .into_iter()
                .flat_map(|(u0, u1)| [(u0, u1.min(r0)), (u0.max(r1), u1)])
                .filter(|&(u0, u1)| u1 - u0 > SLACK)
                .collect();
        }
        let at = |u: f64| Point2::new(a.x + (b.x - a.x) * u, a.y + (b.y - a.y) * u);
        out.extend(spans.into_iter().map(|(u0, u1)| (at(u0), at(u1))));
    }
    out
}

/// The parameter span of segment `a`-`b` inside the disc of radius `r`
/// about `c`, when there is one.
fn inside(a: Point2, b: Point2, c: Point2, r: f64) -> Option<(f64, f64)> {
    let (dx, dy, fx, fy) = (b.x - a.x, b.y - a.y, a.x - c.x, a.y - c.y);
    let qa = dx * dx + dy * dy;
    let qb = 2.0 * (fx * dx + fy * dy);
    let qc = fx * fx + fy * fy - r * r;
    let disc = qb * qb - 4.0 * qa * qc;
    if !(qa > 0.0 && disc > 0.0) {
        return None;
    }
    let root = disc.sqrt();
    Some(((-qb - root) / (2.0 * qa), (-qb + root) / (2.0 * qa)))
}
