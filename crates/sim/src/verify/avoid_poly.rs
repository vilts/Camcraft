//! 2D Z-slab clipping and circumscribed Minkowski disk dilation for `verify::avoid` (#2055).
//! Provenance: textbook Sutherland-Hodgman Z-slab clipping, Andrew's monotone chain hull,
//! and circumscribed disk Minkowski sum over `geo2d::Region`; no vendor source consulted.

use std::f64::consts::PI;

use super::probe::Atom;
use super::swept;
use crate::geo2d::{Loop, Point2, Region};

/// Sagitta bound for circumscribed circular corners on keep-out polygons (mm).
const CIRC_SAG_MM: f64 = 0.001;

pub(super) fn add_poly_for_atoms(
    verts: &[[f64; 3]],
    z_lo: f64,
    z_hi: f64,
    atoms: &[Atom],
    req: f64,
    out: &mut Vec<Region>,
) -> Result<(), String> {
    let f_zlo = verts.iter().map(|v| v[2]).fold(f64::INFINITY, f64::min);
    let f_zhi = verts.iter().map(|v| v[2]).fold(f64::NEG_INFINITY, f64::max);
    for a in atoms {
        // #1444: no cap allowance - a protected item is never cut. (The
        // keep-out reads only `rho_expanded`, which never reads `cap`.)
        let swept_atom = Atom { z0: z_lo + a.z0, z1: z_hi + a.z1, prof: a.prof, cap: false };
        let slab_lo = swept_atom.z0 - req;
        let slab_hi = swept_atom.z1 + req;
        if f_zhi < slab_lo || f_zlo > slab_hi {
            continue;
        }
        let clipped = clip_z_slab(verts, slab_lo, slab_hi);
        if clipped.is_empty() {
            continue;
        }
        let c_zlo = clipped.iter().map(|v| v[2]).fold(f64::INFINITY, f64::min);
        let c_zhi = clipped.iter().map(|v| v[2]).fold(f64::NEG_INFINITY, f64::max);
        let rad = if c_zhi >= swept_atom.z0 && c_zlo <= swept_atom.z1 {
            a.rho_max() + req
        } else if c_zhi < swept_atom.z0 {
            swept::rho_expanded(&swept_atom, c_zhi, req)
        } else {
            swept::rho_expanded(&swept_atom, c_zlo, req)
        };
        if rad <= 0.0 {
            continue;
        }
        let mut xy: Vec<Point2> = clipped.iter().map(|v| Point2::new(v[0], v[1])).collect();
        let hull = convex_hull_xy(&mut xy);
        if let Some(poly) = dilate_convex_xy(&hull, rad)? {
            out.push(poly);
        }
    }
    Ok(())
}

/// Sutherland-Hodgman clip of a polygon (3+) or segment (2) to `slab_lo <= z <= slab_hi`.
fn clip_z_slab(verts: &[[f64; 3]], slab_lo: f64, slab_hi: f64) -> Vec<[f64; 3]> {
    if verts.len() == 2 {
        let (mut a, mut b) = (verts[0], verts[1]);
        if a[2] > b[2] {
            std::mem::swap(&mut a, &mut b);
        }
        if b[2] < slab_lo || a[2] > slab_hi {
            return Vec::new();
        }
        let dz = b[2] - a[2];
        if dz <= 1e-12 {
            return vec![a, b];
        }
        let t0 = ((slab_lo - a[2]) / dz).clamp(0.0, 1.0);
        let t1 = ((slab_hi - a[2]) / dz).clamp(0.0, 1.0);
        let lerp = |t: f64| [a[0] + t * (b[0] - a[0]), a[1] + t * (b[1] - a[1]), a[2] + t * dz];
        return vec![lerp(t0), lerp(t1)];
    }
    let clip_half = |pts: Vec<[f64; 3]>, keep: fn(f64, f64) -> bool, bound: f64| -> Vec<[f64; 3]> {
        let n = pts.len();
        if n == 0 {
            return pts;
        }
        let mut res = Vec::with_capacity(n + 2);
        for i in 0..n {
            let p = pts[i];
            let q = pts[(i + 1) % n];
            let ip = keep(p[2], bound);
            let iq = keep(q[2], bound);
            let cross = || {
                let t = (bound - p[2]) / (q[2] - p[2]);
                [p[0] + t * (q[0] - p[0]), p[1] + t * (q[1] - p[1]), bound]
            };
            if ip && iq {
                res.push(q);
            } else if ip && !iq {
                res.push(cross());
            } else if !ip && iq {
                res.push(cross());
                res.push(q);
            }
        }
        res
    };
    let s1 = clip_half(verts.to_vec(), |z, b| z >= b, slab_lo);
    clip_half(s1, |z, b| z <= b, slab_hi)
}

fn convex_hull_xy(pts: &mut Vec<Point2>) -> Vec<Point2> {
    pts.sort_by(|a, b| a.x.total_cmp(&b.x).then(a.y.total_cmp(&b.y)));
    pts.dedup_by(|a, b| (a.x - b.x).hypot(a.y - b.y) <= 1e-12);
    let m = pts.len();
    if m <= 2 {
        return pts.clone();
    }
    let cr = |o: Point2, a: Point2, b: Point2| (a.x - o.x) * (b.y - o.y) - (a.y - o.y) * (b.x - o.x);
    let mut h: Vec<Point2> = Vec::with_capacity(m * 2);
    for pass in 0..2 {
        let start = h.len();
        for k in 0..m {
            let p = if pass == 0 { pts[k] } else { pts[m - 1 - k] };
            while h.len() >= start + 2 && cr(h[h.len() - 2], h[h.len() - 1], p) <= 0.0 {
                h.pop();
            }
            h.push(p);
        }
        h.pop();
    }
    if h.is_empty() { vec![pts[0]] } else { h }
}

/// Circumscribed Minkowski sum of CCW convex hull (1, 2, or >=3 vertices) with disk `rad`.
fn dilate_convex_xy(hull: &[Point2], rad: f64) -> Result<Option<Region>, String> {
    if hull.is_empty() || rad <= 0.0 {
        return Ok(None);
    }
    let step_max = 2.0 * (1.0 - (CIRC_SAG_MM / rad).min(0.5)).acos().max(0.05);
    let n_circle = ((2.0 * PI / step_max).ceil() as usize).clamp(12, 64);
    let max_step = 2.0 * PI / n_circle as f64;
    let r_outer = rad / (0.5 * max_step).cos();
    if hull.len() == 1 {
        let c = hull[0];
        let ring = (0..n_circle)
            .map(|i| {
                let a = (i as f64) * max_step;
                Point2::new(c.x + r_outer * a.cos(), c.y + r_outer * a.sin())
            })
            .collect();
        return Loop::new(ring).and_then(|l| Region::new(vec![l])).map(Some).map_err(|e| e.to_string());
    }
    let m = hull.len();
    let mut out = Vec::new();
    for i in 0..m {
        let prev = hull[(i + m - 1) % m];
        let cur = hull[i];
        let next = hull[(i + 1) % m];
        let n0 = outward_angle(prev, cur);
        let n1 = outward_angle(cur, next);
        let mut sweep = (n1 - n0).rem_euclid(2.0 * PI);
        if m == 2 && sweep < 1e-12 {
            sweep = PI;
        } else if m >= 3 && sweep > PI + 1e-6 {
            sweep = 0.0;
        }
        let n = ((sweep / max_step).ceil() as usize).clamp(1, n_circle);
        for k in 0..=n {
            let a = n0 + sweep * (k as f64 / n as f64);
            let pt = Point2::new(cur.x + r_outer * a.cos(), cur.y + r_outer * a.sin());
            if out.last().is_none_or(|p: &Point2| (p.x - pt.x).hypot(p.y - pt.y) > 1e-9) {
                out.push(pt);
            }
        }
    }
    if out.len() < 3 {
        return Ok(None);
    }
    Loop::new(out).and_then(|l| Region::new(vec![l])).map(Some).map_err(|e| e.to_string())
}

fn outward_angle(a: Point2, b: Point2) -> f64 {
    let (dx, dy) = (b.x - a.x, b.y - a.y);
    dy.atan2(dx) - PI * 0.5
}

pub(super) fn union_all(mut polys: Vec<Region>) -> Result<Region, String> {
    if polys.is_empty() {
        return Ok(Region::empty());
    }
    while polys.len() > 1 {
        let mut next = Vec::with_capacity(polys.len().div_ceil(2));
        let mut it = polys.into_iter();
        while let Some(a) = it.next() {
            next.push(match it.next() {
                Some(b) => a.union(&b).map_err(|e| e.to_string())?,
                None => a,
            });
        }
        polys = next;
    }
    Ok(polys.into_iter().next().unwrap_or_else(Region::empty))
}

pub(super) fn xy_seg_poly_dist(a: [f64; 2], b: [f64; 2], poly3: &[[f64; 3]]) -> f64 {
    let mut best = f64::INFINITY;
    for p in poly3 {
        best = best.min(swept::seg_dist([p[0], p[1]], a, b));
    }
    let m = poly3.len();
    for i in 0..m {
        let p = [poly3[i][0], poly3[i][1]];
        let q = [poly3[(i + 1) % m][0], poly3[(i + 1) % m][1]];
        best = best.min(seg_seg_dist(a, b, p, q));
    }
    if m >= 3 && (pt_in_tri(a, poly3) || pt_in_tri(b, poly3)) {
        return 0.0;
    }
    best
}

fn seg_seg_dist(a0: [f64; 2], a1: [f64; 2], b0: [f64; 2], b1: [f64; 2]) -> f64 {
    let o = |p: [f64; 2], q: [f64; 2], r: [f64; 2]| (q[0] - p[0]) * (r[1] - p[1]) - (q[1] - p[1]) * (r[0] - p[0]);
    let (oa, ob, oc, od) = (o(a0, a1, b0), o(a0, a1, b1), o(b0, b1, a0), o(b0, b1, a1));
    if oa * ob < 0.0 && oc * od < 0.0 {
        return 0.0;
    }
    swept::seg_dist(a0, b0, b1)
        .min(swept::seg_dist(a1, b0, b1))
        .min(swept::seg_dist(b0, a0, a1))
        .min(swept::seg_dist(b1, a0, a1))
}

fn pt_in_tri(p: [f64; 2], tri: &[[f64; 3]]) -> bool {
    let o = |i: usize, j: usize| (tri[j][0] - tri[i][0]) * (p[1] - tri[i][1]) - (tri[j][1] - tri[i][1]) * (p[0] - tri[i][0]);
    let (s0, s1, s2) = (o(0, 1), o(1, 2), o(2, 0));
    (s0 >= 0.0 && s1 >= 0.0 && s2 >= 0.0) || (s0 <= 0.0 && s1 <= 0.0 && s2 <= 0.0)
}
