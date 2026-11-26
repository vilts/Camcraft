//! The sheet contours' roles and nesting (#670): which picked ring is
//! a part's outer edge and which a hole, which contour each lies in,
//! and the cut order that never lets a part drop before its holes are
//! cut - every contour after everything nested inside it.
//!
//! Provenance: our own, from plasma shop practice as Tormach's 1300PL
//! manual and the LinuxCNC Plasma Primer state it (cut the holes before
//! the outline that frees the part; an island in a hole before the hole
//! that frees the island) and the even-odd containment of the geo2d
//! region model.

use crate::cl::ContourRole;
use crate::geo2d::{Loop, Point2, Region};
use crate::machining2d::GenError;

use super::Voice;

/// The smallest ring area (mm²) that bounds anything: below it a
/// closed contour is a zero-length or collinear ring.
const MIN_AREA: f64 = 1e-6;
/// How far inside a ring (mm) its role is sampled against the part.
const SAMPLE_IN: f64 = 0.01;

/// One contour to cut. `pts` is the ring counter-clockwise without a
/// closing repeat (closed contours) or the run as picked (open ones);
/// `parent` indexes the contour list.
#[derive(Clone, Debug, PartialEq)]
pub struct Contour {
    pub role: ContourRole,
    pub pts: Vec<Point2>,
    pub parent: Option<usize>,
    /// |area| of a closed ring, 0 for a run.
    pub area: f64,
    /// The ring as a region (closed only): its containment test.
    region: Option<Region>,
}

/// Read the picks into contours with roles and parents. A closed
/// contour is an outer edge when the part stands just inside it, a
/// hole when air does; with no part to ask, nesting parity decides.
pub fn contours(loops: &[Loop], chains: &[Vec<Point2>], part: &Region, v: Voice) -> Result<Vec<Contour>, GenError> {
    let mut out: Vec<Contour> = Vec::new();
    for (i, l) in loops.iter().enumerate() {
        let n = i + 1;
        let region = Region::new(vec![l.clone()]).map_err(|e| match e {
            crate::geo2d::GeoError::Invalid(w) if w.contains("crosses itself") => {
                v.invalid(format!("picked contour {n} crosses itself - a cut path cannot cross its own kerf"))
            }
            e => v.invalid(format!("picked contour {n} is unusable: {e}")),
        })?;
        let [ring] = region.loops() else {
            return Err(v.invalid(format!(
                "picked contour {n} has no area (a zero-length or collinear ring) - there is nothing to cut around"
            )));
        };
        // #801: the region keeps the picked contour's AREA, so a
        // zero-width spike or slit (out along a line and straight back)
        // falls out of it silently. A plasma contour is a cut path, not
        // only an area: a picked vertex the ring no longer reaches is
        // refused by name, never cut as the plain outline.
        if let Some(q) = dropped_vertex(l.points(), ring.points()) {
            return Err(v.invalid(format!(
                "picked contour {n} has a picked point the contour no longer reaches, X{:.3} Y{:.3} - a zero-width slit or spike encloses no area, so its kerf would never cut it; pick it as an open run",
                q.x, q.y
            )));
        }
        let area = ring.area().abs();
        if !(area.is_finite() && area > MIN_AREA) {
            return Err(v.invalid(format!(
                "picked contour {n} has no area (a zero-length or collinear ring) - there is nothing to cut around"
            )));
        }
        out.push(Contour {
            role: ContourRole::Outer,
            pts: ring.points().to_vec(),
            parent: None,
            area,
            region: Some(region.clone()),
        });
    }
    for (j, c) in chains.iter().enumerate() {
        out.push(Contour { role: ContourRole::Open, pts: open_run(j + 1, c, v)?, parent: None, area: 0.0, region: None });
    }
    // Parents: the smallest closed ring standing around each contour.
    let closed: Vec<usize> = (0..out.len()).filter(|&k| out[k].region.is_some()).collect();
    for k in 0..out.len() {
        let probe: Vec<Point2> = if out[k].role == ContourRole::Open {
            let p = &out[k].pts;
            vec![p[0], p[p.len() / 2], p[p.len() - 1]]
        } else {
            out[k].pts.clone()
        };
        let mut best: Option<(usize, f64)> = None;
        for &j in &closed {
            if j == k || (out[k].region.is_some() && out[j].area <= out[k].area) {
                continue;
            }
            let Some(rj) = &out[j].region else { continue };
            if probe.iter().all(|p| rj.contains(*p)) && best.is_none_or(|(_, a)| out[j].area < a) {
                best = Some((j, out[j].area));
            }
        }
        out[k].parent = best.map(|(j, _)| j);
    }
    // Roles: the part answers; parity when there is no part.
    for k in 0..out.len() {
        if out[k].role == ContourRole::Open {
            continue;
        }
        let inner = if part.is_empty() {
            let mut depth = 0usize;
            let mut at = out[k].parent;
            while let Some(p) = at {
                depth += 1;
                if depth > out.len() {
                    break;
                }
                at = out[p].parent;
            }
            depth % 2 == 1
        } else {
            !part.contains(inside_point(&out[k].pts))
        };
        out[k].role = if inner { ContourRole::Inner } else { ContourRole::Outer };
    }
    Ok(out)
}

/// How far (mm) a picked vertex may stand from the normalised ring and
/// still be on it: the region's 1 µm lattice, with room to spare.
const ON_RING: f64 = 1e-3;

/// The first picked vertex farther than [`ON_RING`] from every edge of
/// the ring the region kept, or `None` when the ring still reaches them
/// all (a dropped collinear vertex lies on an edge and is no loss).
fn dropped_vertex(picked: &[Point2], ring: &[Point2]) -> Option<Point2> {
    let n = ring.len();
    picked.iter().copied().find(|q| (0..n).all(|k| super::laws::pt_seg(*q, ring[k], ring[(k + 1) % n]) > ON_RING))
}

/// An open run, deduplicated and checked: at least one segment of real
/// length, every point finite, never crossing itself.
fn open_run(n: usize, c: &[Point2], v: Voice) -> Result<Vec<Point2>, GenError> {
    let mut pts: Vec<Point2> = Vec::new();
    for p in c {
        if !(p.x.is_finite() && p.y.is_finite()) {
            return Err(v.invalid(format!("open run {n} carries a non-finite point")));
        }
        if pts.last().is_none_or(|q| (q.x - p.x).hypot(q.y - p.y) > 1e-9) {
            pts.push(*p);
        }
    }
    let len: f64 = pts.windows(2).map(|w| (w[1].x - w[0].x).hypot(w[1].y - w[0].y)).sum();
    if pts.len() < 2 || len <= 1e-6 {
        return Err(v.invalid(format!("open run {n} has no length - there is nothing to cut along")));
    }
    for i in 0..pts.len() - 1 {
        for j in i + 2..pts.len() - 1 {
            if segments_cross(pts[i], pts[i + 1], pts[j], pts[j + 1]) {
                return Err(v.invalid(format!("open run {n} crosses itself - a cut path cannot cross its own kerf")));
            }
        }
    }
    Ok(pts)
}

fn cross(o: Point2, a: Point2, b: Point2) -> f64 {
    (a.x - o.x) * (b.y - o.y) - (a.y - o.y) * (b.x - o.x)
}

/// Proper or touching intersection of two segments.
fn segments_cross(a: Point2, b: Point2, c: Point2, d: Point2) -> bool {
    let (d1, d2) = (cross(c, d, a), cross(c, d, b));
    let (d3, d4) = (cross(a, b, c), cross(a, b, d));
    if ((d1 > 0.0 && d2 < 0.0) || (d1 < 0.0 && d2 > 0.0)) && ((d3 > 0.0 && d4 < 0.0) || (d3 < 0.0 && d4 > 0.0)) {
        return true;
    }
    let on = |p: Point2, q: Point2, r: Point2, v: f64| {
        v.abs() <= 1e-12 && r.x >= p.x.min(q.x) && r.x <= p.x.max(q.x) && r.y >= p.y.min(q.y) && r.y <= p.y.max(q.y)
    };
    on(c, d, a, d1) || on(c, d, b, d2) || on(a, b, c, d3) || on(a, b, d, d4)
}

/// A point just inside a counter-clockwise ring: off the middle of its
/// longest edge, a hair to the left.
fn inside_point(pts: &[Point2]) -> Point2 {
    let n = pts.len();
    let mut best = (0usize, -1.0f64);
    for i in 0..n {
        let (a, b) = (pts[i], pts[(i + 1) % n]);
        let l = (b.x - a.x).hypot(b.y - a.y);
        if l > best.1 {
            best = (i, l);
        }
    }
    let (a, b) = (pts[best.0], pts[(best.0 + 1) % n]);
    let l = best.1.max(1e-12);
    let d = SAMPLE_IN.min(l * 0.1);
    Point2::new((a.x + b.x) * 0.5 - (b.y - a.y) / l * d, (a.y + b.y) * 0.5 + (b.x - a.x) / l * d)
}

/// The cut order: every contour after everything nested inside it (an
/// island before its hole, a hole before its part's outer edge), runs
/// and siblings by their lowest-then-leftmost point so the order is
/// deterministic.
pub fn cut_order(cs: &[Contour]) -> Vec<usize> {
    let key = |k: usize| {
        let lo = cs[k].pts.iter().fold((f64::INFINITY, f64::INFINITY), |m, p| (m.0.min(p.y), m.1.min(p.x)));
        (lo, k)
    };
    let sorted = |mut v: Vec<usize>| {
        v.sort_by(|a, b| {
            let (ka, kb) = (key(*a), key(*b));
            ka.0.0.total_cmp(&kb.0.0).then(ka.0.1.total_cmp(&kb.0.1)).then(ka.1.cmp(&kb.1))
        });
        v
    };
    let children = |p: Option<usize>| sorted((0..cs.len()).filter(|&k| cs[k].parent == p).collect());
    let mut out = Vec::with_capacity(cs.len());
    // Iterative post-order: (node, children pushed?).
    let mut stack: Vec<(usize, bool)> = children(None).into_iter().rev().map(|k| (k, false)).collect();
    while let Some((k, done)) = stack.pop() {
        if done {
            out.push(k);
            continue;
        }
        stack.push((k, true));
        for c in children(Some(k)).into_iter().rev() {
            stack.push((c, false));
        }
    }
    out
}
