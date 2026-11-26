//! 2D Trace / Engrave path planning with per-level "Do not touch" keep-outs (#3349).
//! Provenance: our own polyline-vs-keepout clipping and level-walk over
//! `slots::trace`, `levels`, and `verify::avoid`; no vendor material consulted.

use camcraft_sim::verify::{AVOID_PAD, AvoidSpec};

use crate::cl::{PathBuilder, Toolpath};
use crate::geo2d::{Loop, Point2, Region};
use crate::geometry::Point3;

use super::GenError;
use super::levels::{Motion, WalkState, cut_levels_under, walk_one_level};
use super::ring::{BOUNDARY_TOL, dist, on_boundary};
use super::slots::{TraceParams, trace};

const EPS_LEN: f64 = 1e-9;
const MIN_RUN_LEN: f64 = 1e-6;

struct KoIndex {
    bounds: Option<(Point2, Point2)>,
    segs: Vec<(Point2, Point2)>,
}

impl KoIndex {
    fn of(ko: &Region) -> Self {
        let mut segs = Vec::new();
        for l in ko.loops() {
            let pts = l.points();
            let n = pts.len();
            for i in 0..n {
                segs.push((pts[i], pts[(i + 1) % n]));
            }
        }
        Self { bounds: ko.bounds(), segs }
    }
}

fn lerp2(a: Point2, b: Point2, t: f64) -> Point2 {
    Point2::new(a.x + (b.x - a.x) * t, a.y + (b.y - a.y) * t)
}

fn blocked_xy(ko: &Region, p: Point2) -> bool {
    !ko.is_empty() && ko.contains(p) && !on_boundary(ko, p)
}

fn run_length(pts: &[Point2]) -> f64 {
    pts.windows(2).map(|w| dist(w[0], w[1])).sum()
}

fn push_seg(runs: &mut Vec<Vec<Point2>>, a: Point2, b: Point2) {
    if dist(a, b) <= EPS_LEN {
        return;
    }
    if let Some(last) = runs.last_mut()
        && let Some(&end) = last.last()
        && dist(end, a) <= EPS_LEN
    {
        if dist(end, b) > EPS_LEN {
            last.push(b);
        }
        return;
    }
    runs.push(vec![a, b]);
}

/// Sub-segments of straight segment `a -> b` lying on or outside `ko`.
fn clip_outside(ko: &Region, idx: &KoIndex, a: Point2, b: Point2) -> Vec<(Point2, Point2)> {
    let dx = b.x - a.x;
    let dy = b.y - a.y;
    let len = dx.hypot(dy);
    if !len.is_finite() || len <= EPS_LEN {
        return Vec::new();
    }
    if ko.is_empty() {
        return vec![(a, b)];
    }
    let Some((lo, hi)) = idx.bounds else {
        return vec![(a, b)];
    };
    let (sx0, sx1) = (a.x.min(b.x), a.x.max(b.x));
    let (sy0, sy1) = (a.y.min(b.y), a.y.max(b.y));
    if sx1 < lo.x - EPS_LEN || sx0 > hi.x + EPS_LEN || sy1 < lo.y - EPS_LEN || sy0 > hi.y + EPS_LEN {
        return vec![(a, b)];
    }

    let dd = dx * dx + dy * dy;
    let mut ts = vec![0.0_f64, 1.0_f64];
    for &(p, q) in &idx.segs {
        let vx = p.x - a.x;
        let vy = p.y - a.y;
        let h = vx * dy - vy * dx;
        if h * h <= BOUNDARY_TOL * BOUNDARY_TOL * dd {
            let t = (vx * dx + vy * dy) / dd;
            if t > 0.0 && t < 1.0 {
                ts.push(t);
            }
        }
        let (ex0, ex1) = (p.x.min(q.x), p.x.max(q.x));
        let (ey0, ey1) = (p.y.min(q.y), p.y.max(q.y));
        if sx1 < ex0 - EPS_LEN || sx0 > ex1 + EPS_LEN || sy1 < ey0 - EPS_LEN || sy0 > ey1 + EPS_LEN {
            continue;
        }
        let ex = q.x - p.x;
        let ey = q.y - p.y;
        let den = dx * ey - dy * ex;
        if den == 0.0 {
            continue;
        }
        let wx = p.x - a.x;
        let wy = p.y - a.y;
        let t = (wx * ey - wy * ex) / den;
        let u = (wx * dy - wy * dx) / den;
        let u_tol = EPS_LEN / ex.hypot(ey).max(EPS_LEN);
        let t_tol = EPS_LEN / len;
        if (-u_tol..=1.0 + u_tol).contains(&u) && t.is_finite() && (-t_tol..=1.0 + t_tol).contains(&t) {
            ts.push(t.clamp(0.0, 1.0));
        }
    }

    ts.sort_by(f64::total_cmp);
    ts.dedup_by(|x, y| (*x - *y).abs() * len <= EPS_LEN);
    if let Some(first) = ts.first_mut() {
        *first = 0.0;
    }
    if let Some(last) = ts.last_mut() {
        *last = 1.0;
    }

    let mut out: Vec<(Point2, Point2)> = Vec::new();
    for w in ts.windows(2) {
        let p0 = if w[0] == 0.0 { a } else { lerp2(a, b, w[0]) };
        let p1 = if w[1] == 1.0 { b } else { lerp2(a, b, w[1]) };
        if dist(p0, p1) <= EPS_LEN {
            continue;
        }
        let mid = lerp2(a, b, 0.5 * (w[0] + w[1]));
        if !blocked_xy(ko, mid) {
            match out.last_mut() {
                Some(last) if dist(last.1, p0) <= EPS_LEN => last.1 = p1,
                _ => out.push((p0, p1)),
            }
        }
    }
    out
}

fn rotate_closed_loop_outside(
    run: Vec<Point2>,
    ko_clr: &Region,
    clr_idx: &KoIndex,
) -> Vec<Vec<Point2>> {
    if run.len() < 3 {
        return Vec::new();
    }
    if ko_clr.is_empty() || !blocked_xy(ko_clr, run[0]) {
        return vec![run];
    }
    let m = run.len() - 1;
    if let Some(k) = (1..m).find(|&i| !blocked_xy(ko_clr, run[i])) {
        let mut rot = Vec::with_capacity(m + 1);
        rot.extend_from_slice(&run[k..m]);
        rot.extend_from_slice(&run[..=k]);
        return vec![rot];
    }
    for i in 0..m {
        let segs = clip_outside(ko_clr, clr_idx, run[i], run[(i + 1) % m]);
        if let Some(&(p, q)) = segs.first() {
            let safe = lerp2(p, q, 0.5);
            let mut rot = Vec::with_capacity(m + 2);
            rot.push(safe);
            for step in 1..=m {
                let pt = run[(i + step) % m];
                if rot.last().is_none_or(|prev| dist(*prev, pt) > EPS_LEN) {
                    rot.push(pt);
                }
            }
            if rot.last().is_none_or(|prev| dist(*prev, safe) > EPS_LEN) {
                rot.push(safe);
            }
            return vec![rot];
        }
    }
    Vec::new()
}

fn trim_open_run(run: &[Point2], ko_clr: &Region, clr_idx: &KoIndex) -> Vec<Vec<Point2>> {
    if run.len() < 2 {
        return Vec::new();
    }
    let mut out = Vec::new();
    for w in run.windows(2) {
        for (p, q) in clip_outside(ko_clr, clr_idx, w[0], w[1]) {
            push_seg(&mut out, p, q);
        }
    }
    out
}

fn plan_loop_at_level(
    pts: &[Point2],
    ko_cut: &Region,
    cut_idx: &KoIndex,
    ko_clr: &Region,
    clr_idx: &KoIndex,
) -> Vec<Vec<Point2>> {
    let n = pts.len();
    if n < 2 {
        return Vec::new();
    }
    let mut runs: Vec<Vec<Point2>> = Vec::new();
    for i in 0..n {
        let a = pts[i];
        let b = pts[(i + 1) % n];
        for (p, q) in clip_outside(ko_cut, cut_idx, a, b) {
            push_seg(&mut runs, p, q);
        }
    }
    if runs.len() >= 2
        && let (Some(&first_pt), Some(&last_pt)) = (runs.first().and_then(|r| r.first()), runs.last().and_then(|r| r.last()))
        && dist(last_pt, first_pt) <= EPS_LEN
        && let Some(mut tail) = runs.pop()
        && let Some(head) = runs.first_mut()
    {
        tail.extend_from_slice(&head[1..]);
        *head = tail;
    }
    let closed_single = runs.len() == 1
        && runs[0].len() >= 3
        && runs[0].first().zip(runs[0].last()).is_some_and(|(&a, &b)| dist(a, b) <= EPS_LEN);
    let checked = if closed_single {
        if let Some(only) = runs.into_iter().next() {
            rotate_closed_loop_outside(only, ko_clr, clr_idx)
        } else {
            Vec::new()
        }
    } else if ko_clr.is_empty() {
        runs
    } else {
        let mut kept = Vec::with_capacity(runs.len());
        for r in runs {
            let endpoints_blocked = r.first().is_some_and(|&p| blocked_xy(ko_clr, p))
                || r.last().is_some_and(|&p| blocked_xy(ko_clr, p));
            if endpoints_blocked {
                kept.extend(trim_open_run(&r, ko_clr, clr_idx));
            } else {
                kept.push(r);
            }
        }
        kept
    };
    checked.into_iter().filter(|r| r.len() >= 2 && run_length(r) > MIN_RUN_LEN).collect()
}

/// Trace/Engrave every loop of `region` while keeping the full tool assembly
/// (flutes, shank, and holder) clear of `avoid`.
pub fn trace_avoid(region: &Region, p: &TraceParams, avoid: &AvoidSpec) -> Result<Toolpath, GenError> {
    if avoid.is_empty() {
        return trace(region, p);
    }
    p.validate()?;
    if region.is_empty() {
        return Err(GenError::Empty("empty trace region".into()));
    }
    let mut loops: Vec<&Loop> = region.loops().iter().collect();
    loops.sort_by(|x, y| y.area().abs().total_cmp(&x.area().abs()));
    let ordered_loops: Vec<Vec<Point2>> = loops
        .into_iter()
        .filter_map(|l| {
            let pts: Vec<Point2> = if p.climb {
                l.points().to_vec()
            } else {
                l.points().iter().rev().copied().collect()
            };
            (pts.len() >= 2).then_some(pts)
        })
        .collect();
    if ordered_loops.is_empty() {
        return Err(GenError::Empty("empty trace region".into()));
    }

    let zs = cut_levels_under(p.top_z, p.stock_top, p.bottom_z, p.stepdown)?;
    let mut level_plans: Vec<(f64, Vec<Vec<Point2>>)> = Vec::with_capacity(zs.len());
    let mut blocked_levels: Vec<f64> = Vec::new();
    for &z in &zs {
        let ko_cut = avoid.keepout_region(z, z, AVOID_PAD).map_err(GenError::Invalid)?;
        let top_clr = p.clearance_z.max(z);
        let ko_clr = if (top_clr - z).abs() <= 1e-12 {
            ko_cut.clone()
        } else {
            avoid.keepout_region(z, top_clr, AVOID_PAD).map_err(GenError::Invalid)?
        };
        let cut_idx = KoIndex::of(&ko_cut);
        let clr_idx = KoIndex::of(&ko_clr);
        let mut passes = Vec::new();
        for pts in &ordered_loops {
            passes.extend(plan_loop_at_level(pts, &ko_cut, &cut_idx, &ko_clr, &clr_idx));
        }
        if passes.is_empty() {
            blocked_levels.push(z);
        } else {
            level_plans.push((z, passes));
        }
    }

    let Some(first_pt) = level_plans
        .first()
        .and_then(|(_, ps)| ps.first())
        .and_then(|pass| pass.first())
        .copied()
    else {
        return Err(GenError::Invalid(format!(
            "cannot cut clear of protected 'Do not touch' {}: no safe cutting room remains",
            avoid.names_list()
        )));
    };

    let mut b = PathBuilder::approached(Point3::new(first_pt.x, first_pt.y, p.clearance_z));
    let motion = Motion::new(p.clearance_z, p.retract_z, p.feed, p.plunge_feed).with_top(p.top_z);
    let mut ws = WalkState::new(motion.top_z);
    for (z, passes) in &level_plans {
        walk_one_level(&mut b, *z, passes, None, &motion, None, None, None, &mut ws);
    }
    ws.finish(&mut b, &motion);
    let cur = b.at();
    b.push(crate::cl::MoveKind::Rapid, Point3::new(cur.x, cur.y, p.clearance_z), None);
    if !blocked_levels.is_empty() {
        b.note(format!(
            "Do not touch ({}) blocked {} cut level(s); remaining levels stayed clear",
            avoid.names_list(),
            blocked_levels.len()
        ));
    }

    let mut path = b.finish();
    let _ = camcraft_sim::verify::elevate_rapid_links(&mut path, avoid, None, AVOID_PAD);
    if let Some(why) = crate::approach::refusal(&path) {
        return Err(GenError::Invalid(why).with_ghost(&path));
    }
    if !path.all_finite() {
        return Err(GenError::Invalid("path left the finite horizon".into()));
    }
    Ok(path)
}
