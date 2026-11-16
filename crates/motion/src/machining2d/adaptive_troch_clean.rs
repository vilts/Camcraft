//! Wall and corner clean-up peels for `adaptive_troch` (#1587): expands
//! the already-cleared safe-air region outward in engagement-bounded steps
//! inside the cutter-centre area and cuts only the exposed uncut arcs.
//! Clean-room concentric boundary peeling from offset fronts (Held 1991).

use crate::cl::{MoveKind, PathBuilder, lattice};
use crate::geo2d::{Loop, Point2, Region};
use crate::geometry::Point3;
use crate::load::load_step;

use super::adaptive::corner_radius;
use super::adaptive_troch::AdaptiveTrochParams;
use super::face::GenError;
use super::pocket::PocketParams;

const MIN_REMOVED_AREA: f64 = 0.02;
const UNCUT_SKIN_ERODE: f64 = 0.025;
const MAX_PEEL_STEPS: usize = 64;

/// One clean-up chain at a level: plunge into cut air at `pts[0]` and feed along `pts`.
pub(super) struct CleanChain {
    pub(super) pts: Vec<Point2>,
}

/// Peel outward from `cleared.offset(-(r + 0.02))` inside `center` up to `step <= 0.6*r`
/// per ring, keeping only chains that remove real uncut stock inside `reach`.
#[allow(clippy::too_many_arguments)]
pub(super) fn plan_cleanup(
    center: &Region,
    reach: &Region,
    used_center: &Region,
    cleared: &Region,
    d: f64,
    optimal_load: f64,
    climb: bool,
    t: &AdaptiveTrochParams,
) -> Result<(Vec<CleanChain>, Region), GenError> {
    let r = 0.5 * d;
    let raw_center = center.offset(-0.5e-3)?;
    if raw_center.is_empty() {
        return Ok((Vec::new(), cleared.clone()));
    }
    let mut cur_cleared = cleared.clone();
    let mut cur_used = used_center.clone();
    let mut uncut = reach.difference(&cur_cleared)?;
    if uncut.offset(-UNCUT_SKIN_ERODE)?.is_empty() {
        return Ok((Vec::new(), cur_cleared));
    }
    let air0 = cur_cleared.offset(-(r + 0.02))?.intersect(&raw_center)?;
    let air0 = if air0.is_empty() { cur_used.intersect(&raw_center)? } else { air0 };
    if air0.is_empty() {
        return Ok((Vec::new(), cur_cleared));
    }
    // Round acute (< ~81 deg) convex corners by corner_radius and 90-deg corners lightly (0.15*r)
    // so sharp corner cusps stay under 90 deg while narrow bridges stay open and rest matches HSM.
    let rho_acute = corner_radius(center, r);
    let safe_center = round_acute_corners(&raw_center, rho_acute, 0.20 * r)?.union(&air0)?.intersect(&raw_center)?;
    let mut prev_safe = air0.clone();
    // 0.18*r peel steps with 0.20*r 90-deg rounding hold <= 90 deg even into tight neck corners.
    let step = load_step(d, optimal_load).min(0.18 * r);
    // A custom contact-angle limit must also bound the cleanup front.
    let angle_scale = if t.max_engagement < 90.0 { 1.0 - t.max_engagement.to_radians().cos() } else { 1.0 };
    let engagement_step = 0.18 * r * angle_scale;
    let step = step.min(engagement_step);
    let mut chains = Vec::new();

    for k in 1..=MAX_PEEL_STEPS {
        if uncut.offset(-UNCUT_SKIN_ERODE)?.is_empty() || safe_center.difference(&prev_safe)?.area() < 0.02 {
            break;
        }
        let a_k = air0.offset(k as f64 * step)?.intersect(&safe_center)?;
        if a_k.is_empty() {
            break;
        }
        let mask = prev_safe.offset(0.05)?.union(&cur_used.offset(0.03)?)?;
        let mut step_used = Vec::new();

        for lp in a_k.loops() {
            let mut ring = lp.points().to_vec();
            if ring.len() < 3 {
                continue;
            }
            if climb {
                ring.reverse();
            }
            let sampled = densify_ring(&ring, 1.2);
            for pts in extract_open_chains(&sampled, &mask) {
                if chain_len(&pts) < 0.05 {
                    continue;
                }
                let sweep = Region::sweep(std::slice::from_ref(&pts), r)?;
                if sweep.intersect(&uncut)?.area() < MIN_REMOVED_AREA {
                    continue;
                }
                step_used.push(pts.clone());
                chains.push(CleanChain { pts });
            }
        }
        if !step_used.is_empty() {
            cur_cleared = cur_cleared.union(&Region::sweep(&step_used, r)?)?;
            cur_used = cur_used.union(&Region::sweep(&step_used, 0.02)?)?;
            uncut = reach.difference(&cur_cleared)?;
        }
        prev_safe = a_k;
    }
    Ok((chains, cur_cleared))
}

/// Emit `chains` at level `z`: rapid over cut stock (`near >= above`) to `pts[0]`,
/// plunge vertically to `z` outside prior cutter-centre sweeps, feed along `pts`, and retract.
pub(super) fn emit_cleanup(bld: &mut PathBuilder, chains: &[CleanChain], p: &PocketParams, z: f64, above: f64, hop: f64) {
    let p3 = |q: Point2, zz: f64| Point3::new(lattice(q.x), lattice(q.y), lattice(zz));
    let near = (above + crate::approach::DEFAULT_PLUNGE_HEIGHT).min(hop).max(above);
    for ch in chains {
        let cur = bld.at();
        if (cur.z - hop).abs() > 1e-6 {
            bld.push(MoveKind::Rapid, Point3::new(cur.x, cur.y, hop), None);
        }
        bld.push(MoveKind::Rapid, p3(ch.pts[0], hop), None);
        bld.push(MoveKind::Rapid, p3(ch.pts[0], near), None);
        bld.push(MoveKind::Feed, p3(ch.pts[0], z), Some(p.plunge_feed));
        for &pt in ch.pts.iter().skip(1) {
            bld.push(MoveKind::Feed, p3(pt, z), Some(p.feed));
        }
        let end = bld.at();
        bld.push(MoveKind::Rapid, Point3::new(end.x, end.y, hop), None);
    }
}

fn densify_ring(pts: &[Point2], max_seg: f64) -> Vec<Point2> {
    let n = pts.len();
    let mut out = Vec::with_capacity(n * 2);
    for i in 0..n {
        let (a, b) = (pts[i], pts[(i + 1) % n]);
        let l = (b.x - a.x).hypot(b.y - a.y);
        if l < 1e-6 {
            continue;
        }
        let k = (l / max_seg).ceil() as usize;
        for j in 0..k {
            let t = j as f64 / k as f64;
            out.push(Point2::new(a.x + (b.x - a.x) * t, a.y + (b.y - a.y) * t));
        }
    }
    out
}

fn extract_open_chains(ring: &[Point2], mask: &Region) -> Vec<Vec<Point2>> {
    let n = ring.len();
    if n < 3 {
        return Vec::new();
    }
    let out: Vec<bool> = ring.iter().map(|&q| !mask.contains(q)).collect();
    if out.iter().all(|&o| !o) {
        return Vec::new();
    }
    if out.iter().all(|&o| o) {
        let mut closed = ring.to_vec();
        closed.push(ring[0]);
        return vec![closed];
    }
    let Some(start_edge) = (0..n).find(|&i| !out[i] && out[(i + 1) % n]) else {
        return Vec::new();
    };
    let mut chains = Vec::new();
    let mut cur: Vec<Point2> = Vec::new();
    for step in 1..=n {
        let idx = (start_edge + step) % n;
        let prev = (idx + n - 1) % n;
        if out[idx] {
            if cur.is_empty() {
                cur.push(bisect_outside(ring[prev], ring[idx], mask));
            }
            push_distinct(&mut cur, ring[idx]);
        } else if !cur.is_empty() {
            let exit = bisect_outside(ring[idx], ring[prev], mask);
            push_distinct(&mut cur, exit);
            if cur.len() >= 2 {
                chains.push(std::mem::take(&mut cur));
            } else {
                cur.clear();
            }
        }
    }
    if cur.len() >= 2 {
        chains.push(cur);
    }
    chains
}

fn bisect_outside(inside: Point2, outside: Point2, mask: &Region) -> Point2 {
    let (mut a, mut b) = (inside, outside);
    for _ in 0..10 {
        let mid = Point2::new(0.5 * (a.x + b.x), 0.5 * (a.y + b.y));
        if mask.contains(mid) { a = mid } else { b = mid }
    }
    b
}

fn push_distinct(pts: &mut Vec<Point2>, q: Point2) {
    if pts.last().is_none_or(|p| (p.x - q.x).hypot(p.y - q.y) > 1e-4) {
        pts.push(q);
    }
}

fn chain_len(pts: &[Point2]) -> f64 {
    pts.windows(2).map(|w| (w[1].x - w[0].x).hypot(w[1].y - w[0].y)).sum()
}

fn round_acute_corners(reg: &Region, rho_acute: f64, rho_right: f64) -> Result<Region, GenError> {
    if rho_acute <= 0.0 && rho_right <= 0.0 {
        return Ok(reg.clone());
    }
    let mut loops = Vec::with_capacity(reg.loops().len());
    for lp in reg.loops() {
        let pts = lp.points();
        let n = pts.len();
        if n < 3 {
            continue;
        }
        let mut ring = Vec::with_capacity(n * 2);
        for i in 0..n {
            let (p, v, q) = (pts[(i + n - 1) % n], pts[i], pts[(i + 1) % n]);
            let d1 = (v.x - p.x).hypot(v.y - p.y);
            let d2 = (q.x - v.x).hypot(q.y - v.y);
            if d1 < 1e-6 || d2 < 1e-6 {
                continue;
            }
            let (ux, uy) = ((v.x - p.x) / d1, (v.y - p.y) / d1);
            let (wx, wy) = ((q.x - v.x) / d2, (q.y - v.y) / d2);
            let cross = ux * wy - uy * wx;
            let cos = (ux * wx + uy * wy).clamp(-1.0, 1.0);
            if cross <= 1e-9 || cos > 0.25 {
                ring.push(v);
                continue;
            }
            let rho = if cos < -0.15 { rho_acute } else { rho_right };
            let room = 0.45 * d1.min(d2);
            let rr = rho.min(room * (1.0 + cos) / cross.max(1e-12));
            if rr <= 1e-3 {
                ring.push(v);
                continue;
            }
            let t = rr * cross / (1.0 + cos).max(1e-12);
            let t1 = Point2::new(v.x - ux * t, v.y - uy * t);
            let t2 = Point2::new(v.x + wx * t, v.y + wy * t);
            let c = Point2::new(t1.x - uy * rr, t1.y + ux * rr);
            let a0 = (t1.y - c.y).atan2(t1.x - c.x);
            let mut a1 = (t2.y - c.y).atan2(t2.x - c.x);
            while a1 <= a0 {
                a1 += std::f64::consts::TAU;
            }
            let steps = ((a1 - a0) / 0.1).ceil().max(2.0) as usize;
            ring.push(t1);
            for s in 1..steps {
                let a = a0 + (a1 - a0) * (s as f64 / steps as f64);
                ring.push(Point2::new(c.x + rr * a.cos(), c.y + rr * a.sin()));
            }
            ring.push(t2);
        }
        if ring.len() >= 3 {
            loops.push(Loop::new(ring)?);
        }
    }
    Ok(Region::new(loops)?)
}
