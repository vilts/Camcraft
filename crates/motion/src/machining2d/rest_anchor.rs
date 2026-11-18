//! Re-anchoring the first rest-pocket pass into nearby dive room (#1816).
//!
//! Provenance: planar entry clearance law (`entry::disc_clears`,
//! `entry::dive_room`) and `descent_dive`'s wall margin; clean-room, no
//! vendor material consulted.

use crate::cl::lattice;
use crate::entry::{Entry3D, chord_hits_plan, disc_clears, dive_room};
use crate::geo2d::{Loop, Point2, Region};

use super::pocket::PocketParams;
use super::rest2d::RestPlan;
use super::ring::{dist, on_boundary};
use super::slots::chord_clears;

const EPS: f64 = 1e-9;

/// If `passes[0]` starts at a pinched tip (where no nearby dive point can link
/// without feeding across cleared floor), re-anchor it inside nearby dive room
/// of its own connected component by walking out to the start tip and turning
/// back along the wall pass before continuing to the end. Closed first passes
/// rotate their seam to the vertex nearest dive room when needed.
pub(super) fn anchor_first_pass(passes: &mut [Vec<Point2>], plan: &RestPlan, p: &PocketParams) {
    let Some(first) = passes.first() else {
        return;
    };
    if first.len() < 2 {
        return;
    }
    let r = p.tool_diameter * 0.5;
    let margin = (p.tool_diameter * 0.05).max(0.01);
    let comp = component_for(&plan.center, first);

    if dist(first[0], first[first.len() - 1]) <= EPS {
        if disc_clears(&comp, first[0], margin) {
            return;
        }
        let Some(d) = dive_room_for(&comp, p, margin) else {
            return;
        };
        let n = first.len() - 1;
        if let Some(best) = closest_reachable(first[..n].iter().enumerate(), d, &comp, passes) {
            let mut rotated = Vec::with_capacity(first.len());
            rotated.extend_from_slice(&first[best..n]);
            rotated.extend_from_slice(&first[..=best]);
            passes[0] = rotated;
        }
        return;
    }

    let e = first[0];
    if disc_clears(&comp, e, margin) {
        return;
    }
    let rest_reach = plan.rest.offset(r + 0.005).unwrap_or_else(|_| plan.center.clone());
    if near_dive_ok(&comp, &rest_reach, first, passes, margin, r) {
        return;
    }
    let Some(d) = dive_room_for(&comp, p, margin) else {
        return;
    };

    if first.len() == 2 {
        if let Some(proj) = project_anchor(first[0], first[1], d, &comp, &rest_reach, passes) {
            passes[0] = vec![proj, first[0], first[1]];
        }
        return;
    }

    let max_hop = (r * 0.75).max(margin * 3.0);
    let anchor = pick_open_anchor(first, d, &comp, &rest_reach, passes, p.entry == Entry3D::Helix, max_hop);
    let Some(idx) = anchor else {
        return;
    };
    let mut out = Vec::with_capacity(idx + first.len());
    for &pt in first[..=idx].iter().rev() {
        if out.last().is_none_or(|&prev| prev != pt) {
            out.push(pt);
        }
    }
    for &pt in &first[1..] {
        if out.last().is_none_or(|&prev| prev != pt) {
            out.push(pt);
        }
    }
    if out.len() >= 2 {
        passes[0] = out;
    }
}

fn dive_room_for(comp: &Region, p: &PocketParams, margin: f64) -> Option<Point2> {
    if p.entry == Entry3D::Helix
        && let Some(d) = dive_room(comp, p.helix_radius + 1e-3)
    {
        return Some(d);
    }
    dive_room(comp, margin).or_else(|| dive_room(comp, 0.0))
}

fn component_for(region: &Region, pass: &[Point2]) -> Region {
    let probe = pass[pass.len() / 2];
    let mut outers: Vec<&Loop> = Vec::new();
    let mut holes: Vec<&Loop> = Vec::new();
    for l in region.loops() {
        if l.is_ccw() {
            outers.push(l);
        } else {
            holes.push(l);
        }
    }
    if outers.len() <= 1 {
        return region.clone();
    }
    let mut chosen = None;
    let mut best_d = f64::INFINITY;
    for (idx, &outer) in outers.iter().enumerate() {
        if let Ok(r) = Region::new(vec![outer.clone()])
            && (r.contains(probe) || on_boundary(&r, probe))
        {
            chosen = Some(idx);
            break;
        }
        let d = loop_dist(outer.points(), probe);
        if d < best_d {
            best_d = d;
            chosen = Some(idx);
        }
    }
    let Some(oi) = chosen else {
        return region.clone();
    };
    let outer_loop = outers[oi];
    let Ok(outer_reg) = Region::new(vec![outer_loop.clone()]) else {
        return region.clone();
    };
    let mut loops = vec![outer_loop.clone()];
    for &h in &holes {
        if h.points()
            .iter()
            .all(|&pt| outer_reg.contains(pt) || on_boundary(&outer_reg, pt))
        {
            loops.push(h.clone());
        }
    }
    Region::new(loops).unwrap_or_else(|_| region.clone())
}

fn loop_dist(pts: &[Point2], q: Point2) -> f64 {
    let n = pts.len();
    (0..n)
        .map(|i| seg_dist(q, pts[i], pts[(i + 1) % n]))
        .fold(f64::INFINITY, f64::min)
}

fn seg_dist(q: Point2, a: Point2, b: Point2) -> f64 {
    let (dx, dy) = (b.x - a.x, b.y - a.y);
    let dd = dx * dx + dy * dy;
    let t = if dd > 0.0 {
        (((q.x - a.x) * dx + (q.y - a.y) * dy) / dd).clamp(0.0, 1.0)
    } else {
        0.0
    };
    ((q.x - a.x - t * dx).powi(2) + (q.y - a.y - t * dy).powi(2)).sqrt()
}

fn near_dive_ok(
    comp: &Region,
    rest_reach: &Region,
    first: &[Point2],
    passes: &[Vec<Point2>],
    margin: f64,
    r: f64,
) -> bool {
    let e = first[0];
    let step = 0.5_f64.min((r * 0.25).max(0.02));
    let off = margin * 1.5;
    let mut run = 0.0;
    for w in first.windows(2) {
        let d = dist(w[0], w[1]);
        if !(d.is_finite() && d > 0.0) {
            continue;
        }
        let (nx, ny) = (-(w[1].y - w[0].y) / d * off, (w[1].x - w[0].x) / d * off);
        let mut t = step;
        while t <= d && run + t <= r {
            let f = t / d;
            let (px, py) = (w[0].x + (w[1].x - w[0].x) * f, w[0].y + (w[1].y - w[0].y) * f);
            for (dx, dy) in [(0.0, 0.0), (nx, ny), (-nx, -ny)] {
                let q = Point2::new(lattice(px + dx), lattice(py + dy));
                if dist(q, e) <= r
                    && disc_clears(comp, q, margin)
                    && chord_clears(comp, q, e)
                    && !chord_hits_plan(q, e, passes)
                {
                    let near_end = Point2::new(q.x + (e.x - q.x) * 0.9, q.y + (e.y - q.y) * 0.9);
                    if segment_in_region(rest_reach, q, near_end) {
                        return true;
                    }
                }
            }
            t += step;
        }
        run += d;
        if run > r {
            break;
        }
    }
    false
}

fn pick_open_anchor(
    first: &[Point2],
    d: Point2,
    comp: &Region,
    rest_reach: &Region,
    passes: &[Vec<Point2>],
    prefer_closest: bool,
    max_hop: f64,
) -> Option<usize> {
    let interior = 1..first.len() - 1;
    let clear = |pt: Point2| chord_clears(comp, d, pt) && !chord_hits_plan(d, pt, passes);
    if !prefer_closest
        && let Some(idx) = interior.clone().find(|&i| {
            let pt = first[i];
            dist(d, pt) <= max_hop && clear(pt) && segment_in_region(rest_reach, d, pt)
        })
    {
        return Some(idx);
    }
    let rest_best = interior
        .clone()
        .filter(|&i| clear(first[i]) && segment_in_region(rest_reach, d, first[i]))
        .min_by(|&a, &b| dist(d, first[a]).total_cmp(&dist(d, first[b])));
    if rest_best.is_some() {
        return rest_best;
    }
    closest_reachable(
        first[1..first.len() - 1].iter().enumerate().map(|(k, pt)| (k + 1, pt)),
        d,
        comp,
        passes,
    )
}

fn closest_reachable<'a>(
    pts: impl Iterator<Item = (usize, &'a Point2)> + Clone,
    d: Point2,
    comp: &Region,
    passes: &[Vec<Point2>],
) -> Option<usize> {
    pts.clone()
        .filter(|(_, pt)| chord_clears(comp, d, **pt) && !chord_hits_plan(d, **pt, passes))
        .min_by(|(_, a), (_, b)| dist(d, **a).total_cmp(&dist(d, **b)))
        .or_else(|| pts.min_by(|(_, a), (_, b)| dist(d, **a).total_cmp(&dist(d, **b))))
        .map(|(i, _)| i)
}

fn project_anchor(
    a: Point2,
    b: Point2,
    d: Point2,
    comp: &Region,
    rest_reach: &Region,
    passes: &[Vec<Point2>],
) -> Option<Point2> {
    let (dx, dy) = (b.x - a.x, b.y - a.y);
    let dd = dx * dx + dy * dy;
    if !(dd.is_finite() && dd > 0.0) {
        return None;
    }
    let t = (((d.x - a.x) * dx + (d.y - a.y) * dy) / dd).clamp(0.05, 0.95);
    let proj = Point2::new(lattice(a.x + dx * t), lattice(a.y + dy * t));
    (proj != a
        && proj != b
        && chord_clears(comp, d, proj)
        && !chord_hits_plan(d, proj, passes)
        && segment_in_region(rest_reach, d, proj))
    .then_some(proj)
}

fn segment_in_region(region: &Region, a: Point2, b: Point2) -> bool {
    let len = dist(a, b);
    let n = (len / 0.25).ceil().max(2.0) as usize;
    (0..=n).all(|k| {
        let t = k as f64 / n as f64;
        let pt = Point2::new(a.x + (b.x - a.x) * t, a.y + (b.y - a.y) * t);
        region.contains(pt) || on_boundary(region, pt)
    })
}
