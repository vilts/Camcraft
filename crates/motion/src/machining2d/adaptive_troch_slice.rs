//! Scanline slicing and polyline geometry helpers for `adaptive_troch` (#1587);
//! Douglas & Peucker (1973) line simplification and oriented bounding scanlines.

use std::f64::consts::FRAC_PI_2;

use crate::cl::{PathBuilder, lattice};
use crate::geo2d::{Loop, Point2, Region};
use crate::geometry::Point3;

use super::adaptive::boundary_dist;
use super::face::GenError;
use super::pocket::PocketParams;
use super::trochoid_slot::{LaneLayout, MIN_BAND_PER_D, emit_level};
use super::waterscan::scan_row;

const WALL_MARGIN: f64 = 0.01;
const SAMPLE: f64 = 0.25;
const LANE_GAP: f64 = 0.25;

pub(super) type SlicedBand = (f64, Vec<[Point2; 2]>);

pub(super) fn slice_angle(
    sub: &Region,
    theta: f64,
    b_cap: f64,
    r: f64,
    d: f64,
    allow_short: bool,
) -> Result<Option<SlicedBand>, GenError> {
    let rot = rotate_region(sub, -theta)?;
    let eroded0 = rot.offset(-(b_cap + WALL_MARGIN))?;
    if eroded0.is_empty() {
        return Ok(None);
    }
    let Some((b0_lo, b0_hi)) = eroded0.bounds() else {
        return Ok(None);
    };
    let (min_v, max_v) = (b0_lo.y, b0_hi.y);
    let span = (max_v - min_v).max(0.0);
    let target_pitch = 2.0 * b_cap + 0.45 * r;
    let m = if span < 2.0 * b_cap + LANE_GAP {
        1
    } else {
        ((span / target_pitch).ceil() as usize + 1).max(2)
    };
    let (b_use, eroded) = if m > 1 {
        let dv = span / (m - 1) as f64;
        if dv < 2.0 * b_cap + LANE_GAP {
            let shrunk = ((dv - LANE_GAP) * 0.5).min(b_cap);
            if shrunk <= MIN_BAND_PER_D * d {
                (b_cap, eroded0)
            } else {
                (shrunk, rot.offset(-(shrunk + WALL_MARGIN))?)
            }
        } else {
            (b_cap, eroded0)
        }
    } else {
        (b_cap, eroded0)
    };
    let Some((b_lo, b_hi)) = eroded.bounds() else {
        return Ok(None);
    };
    let (ev0, ev1) = (b_lo.y, b_hi.y);
    let e_span = ev1 - ev0;
    let vs: Vec<f64> = if e_span <= 2e-3 {
        vec![0.5 * (ev0 + ev1)]
    } else {
        let count = ((e_span / (2.0 * b_use + LANE_GAP)).floor() as usize + 1).min(m.max(2));
        if count <= 1 {
            vec![0.5 * (ev0 + ev1)]
        } else {
            let (lo, hi) = (ev0 + 1e-3, ev1 - 1e-3);
            (0..count).map(|k| lo + (hi - lo) * k as f64 / (count - 1) as f64).collect()
        }
    };
    let min_keep = if allow_short { 0.25 } else { 2.2 * b_use };
    let mut segs = Vec::new();
    let mut all_segs = Vec::new();
    for v in vs {
        for (u0, u1) in scan_row(&eroded, v, true) {
            let len = u1 - u0;
            if len < 0.25 {
                continue;
            }
            let trim = 0.02_f64.min(0.1 * len);
            let a = rotate_pt(Point2::new(u0 + trim, v), theta);
            let b = rotate_pt(Point2::new(u1 - trim, v), theta);
            all_segs.push([a, b]);
            if len >= min_keep {
                segs.push([a, b]);
            }
        }
    }
    if segs.is_empty() {
        segs = all_segs;
    }
    Ok((!segs.is_empty()).then_some((b_use, segs)))
}

pub(super) fn candidate_angles(reg: &Region) -> Vec<f64> {
    let mut angles = vec![0.0, FRAC_PI_2];
    let mut edges: Vec<(f64, f64)> = Vec::new();
    for lp in reg.loops() {
        let pts = lp.points();
        for i in 0..pts.len() {
            let (a, b) = (pts[i], pts[(i + 1) % pts.len()]);
            let (dx, dy) = (b.x - a.x, b.y - a.y);
            let l = dx.hypot(dy);
            if l > 1e-3 {
                edges.push((l, dy.atan2(dx).rem_euclid(std::f64::consts::PI)));
            }
        }
    }
    edges.sort_by(|x, y| y.0.total_cmp(&x.0));
    for (_, a) in edges {
        if angles.iter().all(|&ex| (ex - a).abs() > 0.05 && (ex - a).abs() < std::f64::consts::PI - 0.05) {
            angles.push(a);
            if angles.len() >= 4 {
                break;
            }
        }
    }
    angles
}

fn rotate_pt(q: Point2, a: f64) -> Point2 {
    let (s, c) = a.sin_cos();
    Point2::new(q.x * c - q.y * s, q.x * s + q.y * c)
}

fn rotate_region(reg: &Region, a: f64) -> Result<Region, GenError> {
    let mut loops = Vec::with_capacity(reg.loops().len());
    for lp in reg.loops() {
        let pts = lp.points().iter().map(|&q| rotate_pt(q, a)).collect();
        loops.push(Loop::new(pts)?);
    }
    Ok(Region::new(loops)?)
}

pub(super) fn rotate_to_longest_edge(pts: &[Point2]) -> Vec<Point2> {
    if pts.len() < 3 {
        return pts.to_vec();
    }
    let open_n = if pts.first() == pts.last() { pts.len() - 1 } else { pts.len() };
    let mut best = (0.0, 0usize);
    for i in 0..open_n {
        let (a, b) = (pts[i], pts[(i + 1) % open_n]);
        let l = (b.x - a.x).hypot(b.y - a.y);
        if l > best.0 {
            best = (l, i);
        }
    }
    let i = best.1;
    let (a, b) = (pts[i], pts[(i + 1) % open_n]);
    let mid = Point2::new(0.5 * (a.x + b.x), 0.5 * (a.y + b.y));
    let mut out = Vec::with_capacity(open_n + 2);
    out.push(mid);
    for k in 1..=open_n {
        out.push(pts[(i + k) % open_n]);
    }
    out.push(mid);
    out
}

pub(super) fn dedup_open(pts: &[Point2]) -> Vec<Point2> {
    let mut out: Vec<Point2> = Vec::with_capacity(pts.len());
    for &q in pts {
        if out.last().is_none_or(|p| (p.x - q.x).hypot(p.y - q.y) > 1e-6) {
            out.push(q);
        }
    }
    out
}

pub(super) fn layout_confined(
    lay: &LaneLayout,
    center: &Region,
    check_cross: bool,
    p: &PocketParams,
) -> Result<bool, GenError> {
    let mut bld = PathBuilder::new(Point3::new(lattice(lay.entry.x), lattice(lay.entry.y), p.top_z));
    emit_level(&mut bld, &lay.lay, &lay.tp, p.bottom_z, p.top_z, false, lay.open)
        .map_err(super::adaptive_troch::renamed)?;
    let path = bld.finish();
    if super::adaptive_troch::confined(&path, center)?.is_some() {
        return Ok(false);
    }
    Ok(!check_cross || crate::verify::crossed_pairs(&path, super::ARC_FIT_TOL).is_empty())
}

/// The share of `reach`'s area outside `swept`.
pub(super) fn rest_share(reach: &Region, swept: &Region) -> f64 {
    let all = reach.area();
    if !(all.is_finite() && all > 0.0) {
        return 0.0;
    }
    let rem = reach.difference(swept).map_or_else(|_| (all - swept.area()).max(0.0), |r| r.area());
    (rem / all).clamp(0.0, 1.0)
}

/// The least distance from the chain to `center`'s boundary (0 where outside).
pub(super) fn clearance(center: &Region, chain: &[Point2]) -> f64 {
    let mut least = f64::INFINITY;
    for w in chain.windows(2) {
        let len = (w[1].x - w[0].x).hypot(w[1].y - w[0].y);
        let n = ((len / SAMPLE).ceil() as usize).clamp(1, 100_000);
        for k in 0..=n {
            let t = k as f64 / n as f64;
            let q = Point2::new(w[0].x + (w[1].x - w[0].x) * t, w[0].y + (w[1].y - w[0].y) * t);
            least = least.min(if center.contains(q) { boundary_dist(center, q) } else { 0.0 });
        }
    }
    if least.is_finite() { least } else { 0.0 }
}

/// Douglas & Peucker (1973) simplification within `tol`, ends preserved.
pub(super) fn straightened(pts: &[Point2], tol: f64) -> Vec<Point2> {
    if pts.len() < 3 {
        return pts.to_vec();
    }
    let mut keep = vec![false; pts.len()];
    keep[0] = true;
    keep[pts.len() - 1] = true;
    let mut todo = vec![(0usize, pts.len() - 1)];
    while let Some((i, j)) = todo.pop() {
        let (a, b) = (pts[i], pts[j]);
        let (dx, dy) = (b.x - a.x, b.y - a.y);
        let l = dx.hypot(dy);
        let off = |q: Point2| if l > 1e-12 { ((q.x - a.x) * dy - (q.y - a.y) * dx).abs() / l } else { (q.x - a.x).hypot(q.y - a.y) };
        if let Some((dist, k)) = (i + 1..j).map(|k| (off(pts[k]), k)).max_by(|x, y| x.0.total_cmp(&y.0))
            && dist > tol
        {
            keep[k] = true;
            todo.push((i, k));
            todo.push((k, j));
        }
    }
    dedup_open(&pts.iter().zip(&keep).filter(|(_, k)| **k).map(|(&q, _)| q).collect::<Vec<_>>())
}
