//! Provenance: Held's published zigzag pocketing model and our scan-row implementation (#1490).
//! Rest rows end inside the center boundary; closing rings remove wall cusps.
//! On the rotary wrap (#3168) a ring that wraps the seam skips the seam's
//! two cover copies - one azimuth - and keeps one period per arc.

use crate::cl::lattice;
use crate::geo2d::{Point2, Region};
use super::face::{GenError, band};
use super::slots::chord_clears;
use super::waterscan::scan_row;

/// Rows followed by each rest area's closed boundary. A gap up to 0.01 mm
/// prevents rounded scan endpoints crossing the closing ring; the ring
/// cuts that gap. Limit it to a quarter step so tiny tools retain rows.
/// All passes stay inside the original center region. Row points stand
/// on the 1 um lattice (the gap holds them inside): a helix entry snaps
/// its turns there, and an off-lattice start made two levels' helices
/// at one start read as crossing chords (#3021).
pub(crate) fn rest_passes(center: &Region, step: f64, climb: bool) -> Result<Vec<Vec<Point2>>, GenError> {
    ringed_passes(center, step, climb, None)
}

/// The rotary wrap's seam law on the developed plane (#3168): the cover
/// runs from `lo` to `hi` in s and repeats every `period`. A boundary
/// loop that reaches a cover end within `reach` wraps the seam - its
/// cover-end edges are the seam's copies at ONE azimuth - so its ring
/// drops those edges and keeps one period per leftover arc. A loop that
/// spans a period mid-cover touches no cover end: it wraps an azimuth
/// it never stands on, and closes as the no-seam case does.
#[derive(Clone, Copy, Debug)]
pub(crate) struct SeamWrap {
    pub lo: f64,
    pub hi: f64,
    pub period: f64,
    pub reach: f64,
}

/// #3168: the same rows and rings under the wrap's seam law (`seam`:
/// the developed cover's ends, period and seam reach). See `SeamWrap`.
pub(crate) fn ringed_passes(
    center: &Region,
    step: f64,
    climb: bool,
    seam: Option<SeamWrap>,
) -> Result<Vec<Vec<Point2>>, GenError> {
    let rows = center.offset(-0.01_f64.min(step * 0.25))?;
    let mut passes = if rows.is_empty() { Vec::new() } else { zigzag_passes(&rows, step, climb)? };
    for p in passes.iter_mut().flatten() {
        *p = Point2::new(lattice(p.x), lattice(p.y));
    }
    for ring in center.loops() {
        passes.extend(boundary_rings(ring.points(), climb, seam));
    }
    Ok(passes)
}

/// One loop's ring passes (see `ringed_passes`).
fn boundary_rings(points: &[Point2], climb: bool, seam: Option<SeamWrap>) -> Vec<Vec<Point2>> {
    if points.len() < 2 {
        return Vec::new();
    }
    let mut pts = points.to_vec();
    if !climb {
        pts.reverse();
    }
    let Some(sm) = seam else {
        return closed_ring(pts);
    };
    let mut min = f64::INFINITY;
    let mut max = f64::NEG_INFINITY;
    for p in &pts {
        min = min.min(p.x);
        max = max.max(p.x);
    }
    // The gate is seam CONTACT, not mere span (#3168): a loop that
    // reaches a cover end within the seam's reach wraps the seam - its
    // cover-end edges are the seam's copies at ONE azimuth - and its
    // arcs arrive open. A loop that spans a period mid-cover touches no
    // cover end: it wraps an azimuth it never stands on, and closes as
    // the no-seam case does, or a period-spanning loop off the seam
    // would be trimmed by the wrong rule.
    let tol = 1e-6 * sm.period.abs().max(1.0) + 0.05 * sm.reach.abs();
    let at_end = min <= sm.lo + sm.reach + tol || max >= sm.hi - sm.reach - tol;
    if !at_end {
        return closed_ring(pts);
    }
    // The loop wraps the seam: its arcs arrive open. A closing chord
    // there would run one full period across the cover.
    let edge_tol = 1e-6 * (max - min).abs().max(1.0);
    let span_tol = 1e-6 * sm.period.abs().max(1.0);
    // A ring whose wall touches no cover bound owns no seam run to drop.
    // One that still spans a period wraps the seam through one period of
    // its own extent (`one_period`); a shorter one closes - only a real
    // seam run may leave a pass open, for an open pass drops its closing
    // wall edge (PR #3496 review).
    let has_seam_vertex =
        pts.iter().any(|p| (p.x - sm.lo).abs() <= edge_tol || (p.x - sm.hi).abs() <= edge_tol);
    if !has_seam_vertex && max - min < sm.period - span_tol {
        return closed_ring(pts);
    }
    let mut out = Vec::new();
    for arc in seam_open_arcs(&pts, sm.lo, sm.hi, edge_tol) {
        let lo = arc.iter().fold(f64::INFINITY, |m, p| m.min(p.x));
        let hi = arc.iter().fold(f64::NEG_INFINITY, |m, p| m.max(p.x));
        if hi - lo >= sm.period - span_tol {
            if let Some(trimmed) = one_period(&arc, sm.period) {
                out.push(trimmed);
            }
        } else {
            out.push(arc);
        }
    }
    out
}

/// The ring's closing point: the walk lands each closed ring back on its
/// own start (`level_ctx::walk_pass`).
fn closed_ring(mut pts: Vec<Point2>) -> Vec<Vec<Point2>> {
    if let Some(&first) = pts.first() {
        pts.push(first);
    }
    vec![pts]
}

/// The loop with its cover-end edge runs (points within `tol` of the
/// ends `a`/`b`) removed: each maximal run of seam points is dropped and
/// the leftover non-seam arcs come back open. A loop with no seam point
/// comes back whole and open; `boundary_rings` closes such a loop before
/// this helper runs, and keeps that branch for a direct caller.
fn seam_open_arcs(pts: &[Point2], a: f64, b: f64, tol: f64) -> Vec<Vec<Point2>> {
    let flags: Vec<bool> = pts.iter().map(|p| (p.x - a).abs() <= tol || (p.x - b).abs() <= tol).collect();
    if !flags.iter().any(|&f| f) {
        return vec![pts.to_vec()];
    }
    if flags.iter().all(|&f| f) {
        return Vec::new();
    }
    let n = pts.len();
    // Walk one lap from the head of a non-seam run and flush at each
    // seam run.
    let Some(start) = (0..n).find(|&i| !flags[i] && flags[(i + n - 1) % n]) else {
        return vec![pts.to_vec()];
    };
    let mut arcs: Vec<Vec<Point2>> = Vec::new();
    let mut cur: Vec<Point2> = Vec::new();
    for k in 0..n {
        let i = (start + k) % n;
        if flags[i] {
            if cur.len() >= 2 {
                arcs.push(std::mem::take(&mut cur));
            } else {
                cur.clear();
            }
        } else {
            cur.push(pts[i]);
        }
    }
    if cur.len() >= 2 {
        arcs.push(cur);
    }
    arcs
}

/// The arc's first period of x from its own start: a contiguous window
/// of width `period` the arc covers, with the far end's crossing
/// interpolated, so the arc ends on the azimuth it started from - the
/// second lap is the cover's duplicate of the first and adds no azimuth.
/// `None` when the window keeps fewer than two points.
fn one_period(arc: &[Point2], period: f64) -> Option<Vec<Point2>> {
    let first = *arc.first()?;
    let mut min = f64::INFINITY;
    let mut max = f64::NEG_INFINITY;
    for p in arc {
        min = min.min(p.x);
        max = max.max(p.x);
    }
    if !min.is_finite() || !max.is_finite() {
        return None;
    }
    // Saturating window head: no unchecked bound math.
    let lo = first.x.max(min).min((max - period).max(min));
    let hi = lo + period;
    let inside = |x: f64| x >= lo && x <= hi;
    let mut out = vec![first];
    for w in arc.windows(2) {
        let (p, q) = (w[0], w[1]);
        if inside(q.x) {
            out.push(q);
            continue;
        }
        if inside(p.x) && q.x != p.x {
            let bound = if q.x > hi { hi } else { lo };
            let t = ((bound - p.x) / (q.x - p.x)).clamp(0.0, 1.0);
            out.push(Point2::new(bound, p.y + t * (q.y - p.y)));
        }
        break;
    }
    (out.len() >= 2).then_some(out)
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

/// Test probe for the seam ring law (PR #3496's review): one boundary
/// loop's ring passes, exactly as the Zigzag level's planner takes them,
/// with the seam's `(lo, hi, period, reach)`. Not a public API.
#[doc(hidden)]
pub fn boundary_rings_probe(points: &[Point2], climb: bool, seam: Option<(f64, f64, f64, f64)>) -> Vec<Vec<Point2>> {
    let seam = seam.map(|(lo, hi, period, reach)| SeamWrap { lo, hi, period, reach });
    boundary_rings(points, climb, seam)
}
