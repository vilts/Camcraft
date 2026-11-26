//! The exact facet-vs-swept-atom penetration of the solid law. A tool
//! atom A moved along a straight move d meets a facet T iff A meets the
//! convex polytope P = conv(T u (T - d)) (Minkowski: from + t d + a in T
//! iff a in T - t d) - so a whole straight move is one exact test, no
//! sampling. A is a convex solid of revolution with a concave radius
//! profile rho(z), and P's horizontal distance from the axis at height
//! z, f(z), is convex (partial minimization of a convex function over a
//! convex set, Boyd & Vandenberghe 3.2.5): g = f - rho is convex, its
//! minimum found by golden-section search (Kiefer 1953). A constant-
//! radius band needs no search - P clipped to the band's slab and
//! projected is the exact answer. P's slice at a height is the hull of
//! its vertex pairs' crossings (any convex combination of the vertices
//! at that height is one of pair points); the hull is Andrew's monotone
//! chain. Textbook geometry throughout; no vendor source.
//!
//! The penetration measured is RADIAL - how far inside rho(z) P reaches
//! at its worst height - which bounds the true depth from above: it can
//! over-report a graze by the profile's slope, never hide a cut.

use super::probe::Atom;

/// A facet swept back along a move, in the tool frame of its start.
pub(super) struct Body {
    p: [[f64; 3]; 6],
    n: usize,
    zlo: f64,
    zhi: f64,
}

impl Body {
    pub(super) fn new(t: &[[f64; 3]; 3], d: [f64; 3]) -> Self {
        let mut p = [[0.0; 3]; 6];
        p[..3].copy_from_slice(t);
        let n = if d == [0.0, 0.0, 0.0] {
            3
        } else {
            for k in 0..3 {
                p[3 + k] = [t[k][0] - d[0], t[k][1] - d[1], t[k][2] - d[2]];
            }
            6
        };
        Self::finish(p, n)
    }

    pub(super) fn from_edge(e: &[[f64; 3]; 2], d: [f64; 3]) -> Self {
        let mut p = [[0.0; 3]; 6];
        p[..2].copy_from_slice(e);
        let n = if d == [0.0, 0.0, 0.0] {
            2
        } else {
            for k in 0..2 {
                p[2 + k] = [e[k][0] - d[0], e[k][1] - d[1], e[k][2] - d[2]];
            }
            4
        };
        Self::finish(p, n)
    }

    fn finish(p: [[f64; 3]; 6], n: usize) -> Self {
        let (mut zlo, mut zhi) = (f64::INFINITY, f64::NEG_INFINITY);
        for q in &p[..n] {
            zlo = zlo.min(q[2]);
            zhi = zhi.max(q[2]);
        }
        Self { p, n, zlo, zhi }
    }

    /// Every vertex pair's strict crossing of the plane z = c.
    fn crossings(&self, c: f64, out: &mut Vec<[f64; 2]>) {
        let v = &self.p[..self.n];
        for i in 0..v.len() {
            for j in i + 1..v.len() {
                let (a, b) = (v[i], v[j]);
                if (a[2] - c) * (b[2] - c) < 0.0 {
                    let s = (c - a[2]) / (b[2] - a[2]);
                    out.push([a[0] + (b[0] - a[0]) * s, a[1] + (b[1] - a[1]) * s]);
                }
            }
        }
    }

    /// Horizontal distance from the axis to P within `lo <= z <= hi`.
    fn slab_dist(&self, lo: f64, hi: f64, pts: &mut Vec<[f64; 2]>) -> f64 {
        pts.clear();
        for q in &self.p[..self.n] {
            if q[2] >= lo && q[2] <= hi {
                pts.push([q[0], q[1]]);
            }
        }
        self.crossings(lo, pts);
        if hi > lo {
            self.crossings(hi, pts);
        }
        hull_dist(pts)
    }
}

/// Worst radial penetration of the swept facet into `atom` (grown by
/// `grow`) above local height `cut_lo`; `NEG_INFINITY` when they share
/// no height.
pub(super) fn pen(b: &Body, atom: &Atom, cut_lo: f64, allow: f64, grow: f64, pts: &mut Vec<[f64; 2]>) -> f64 {
    let lo = atom.z0.max(cut_lo).max(b.zlo);
    // #1444: a cutting top face gets the tip's allowance (`Atom::cap`).
    let top = if atom.cap { atom.z1 - cut_lo.max(0.0) } else { atom.z1 };
    let hi = top.min(b.zhi);
    if lo.partial_cmp(&hi).is_none_or(|o| o.is_gt()) {
        return f64::NEG_INFINITY;
    }
    let d = b.slab_dist(lo, hi, pts);
    if !d.is_finite() {
        return f64::NEG_INFINITY;
    }
    if hi - lo <= 1e-12 {
        return atom.rho(lo) + grow - d;
    }
    let bound = atom.rho_peak(lo, hi) + grow - d;
    if atom.is_flat() || bound <= allow {
        return bound; // exact for a band; clear of the allowance otherwise
    }
    golden_pen(lo, hi, |z| b.slab_dist(z, z, pts) - atom.rho(z) - grow)
}

/// Minkowski-expanded radius of `atom` grown in 3D Euclidean distance by `m`.
pub(super) fn rho_expanded(atom: &super::probe::Atom, w: f64, m: f64) -> f64 {
    use super::probe::Prof;
    let m = m.max(0.0);
    if w < atom.z0 - m || w > atom.z1 + m {
        return 0.0;
    }
    match atom.prof {
        // #1444: an undercut head is read as its circumscribed cylinder.
        // `Dove` is widest (r) at its bottom and `Ball` has radius r, so
        // the cylinder holds the head at each Z stretch of `avoid_poly`,
        // and the keep-out can only over-report, never under-report.
        Prof::Flat(r) | Prof::Dove { r, .. } | Prof::Ball(r) => {
            if w < atom.z0 {
                r + (m * m - (atom.z0 - w).powi(2)).max(0.0).sqrt()
            } else if w > atom.z1 {
                r + (m * m - (w - atom.z1).powi(2)).max(0.0).sqrt()
            } else {
                r + m
            }
        }
        Prof::Bull { flat, cr } => {
            let z_arc = atom.z0 + cr;
            if w < z_arc {
                flat + ((cr + m).powi(2) - (z_arc - w).powi(2)).max(0.0).sqrt()
            } else if w <= atom.z1 {
                flat + cr + m
            } else {
                flat + cr + (m * m - (w - atom.z1).powi(2)).max(0.0).sqrt()
            }
        }
        Prof::Cone { k, r } => {
            let top = (atom.z0 + r / k).min(atom.z1);
            let z_star = (w + m * k / (1.0 + k * k).sqrt()).clamp(atom.z0, top);
            let cone = (k * (z_star - atom.z0)).min(r) + (m * m - (w - z_star).powi(2)).max(0.0).sqrt();
            if w >= top && w <= atom.z1 {
                cone.max(r + m)
            } else if w > atom.z1 {
                cone.max(r + (m * m - (w - atom.z1).powi(2)).max(0.0).sqrt())
            } else {
                cone
            }
        }
    }
}

#[cfg(test)]
#[path = "swept_tests.rs"]
mod tests;

/// Exact 3D Euclidean penetration of swept `b` inside `atom` inflated by 3D
/// ball `m = clearance + grow` (`NEG_INFINITY` if disjoint in Z).
pub(super) fn pen_3d(b: &Body, atom: &Atom, m: f64, allow: f64, pts: &mut Vec<[f64; 2]>) -> f64 {
    let m = m.max(0.0);
    let lo = (atom.z0 - m).max(b.zlo);
    let hi = (atom.z1 + m).min(b.zhi);
    if lo.partial_cmp(&hi).is_none_or(|o| o.is_gt()) {
        return f64::NEG_INFINITY;
    }
    let d = b.slab_dist(lo, hi, pts);
    if !d.is_finite() {
        return f64::NEG_INFINITY;
    }
    let upper = atom.rho_max() + m - d;
    if upper <= allow {
        return upper;
    }
    if hi - lo <= 1e-12 {
        return rho_expanded(atom, lo, m) - d;
    }
    golden_pen(lo, hi, |z| b.slab_dist(z, z, pts) - rho_expanded(atom, z, m))
}

fn golden_pen(lo: f64, hi: f64, mut g: impl FnMut(f64) -> f64) -> f64 {
    const PHI: f64 = 0.618_033_988_749_894_8;
    let (mut a, mut c) = (lo, hi);
    let mut x = c - PHI * (c - a);
    let mut y = a + PHI * (c - a);
    let (mut gx, mut gy) = (g(x), g(y));
    let mut best = g(lo).min(g(hi)).min(gx).min(gy);
    for _ in 0..64 {
        if c - a <= 1e-9 {
            break;
        }
        if gx < gy {
            c = y;
            y = x;
            gy = gx;
            x = c - PHI * (c - a);
            gx = g(x);
            best = best.min(gx);
        } else {
            a = x;
            x = y;
            gx = gy;
            y = a + PHI * (c - a);
            gy = g(y);
            best = best.min(gy);
        }
    }
    -best
}

/// Distance from `o` to segment p-q.
pub(super) fn seg_dist(o: [f64; 2], p: [f64; 2], q: [f64; 2]) -> f64 {
    let (dx, dy) = (q[0] - p[0], q[1] - p[1]);
    let dd = dx * dx + dy * dy;
    let s = if dd > 0.0 { (((o[0] - p[0]) * dx + (o[1] - p[1]) * dy) / dd).clamp(0.0, 1.0) } else { 0.0 };
    (p[0] + dx * s - o[0]).hypot(p[1] + dy * s - o[1])
}

fn cross(o: [f64; 2], a: [f64; 2], b: [f64; 2]) -> f64 {
    (a[0] - o[0]) * (b[1] - o[1]) - (a[1] - o[1]) * (b[0] - o[0])
}

/// Distance from the origin to the convex hull of `pts` (0 inside);
/// `INFINITY` for no points. Reorders `pts`.
fn hull_dist(pts: &mut Vec<[f64; 2]>) -> f64 {
    let o = [0.0, 0.0];
    match pts.len() {
        0 => return f64::INFINITY,
        1 => return pts[0][0].hypot(pts[0][1]),
        _ => {}
    }
    pts.sort_by(|a, b| a[0].total_cmp(&b[0]).then(a[1].total_cmp(&b[1])));
    pts.dedup();
    let mut h: Vec<[f64; 2]> = Vec::with_capacity(pts.len() * 2);
    let m = pts.len();
    for pass in 0..2 {
        let start = h.len();
        for k in 0..m {
            let p = if pass == 0 { pts[k] } else { pts[m - 1 - k] };
            while h.len() >= start + 2 && cross(h[h.len() - 2], h[h.len() - 1], p) <= 0.0 {
                h.pop();
            }
            h.push(p);
        }
        h.pop();
    }
    let n = h.len();
    if n == 0 {
        return pts[0][0].hypot(pts[0][1]);
    }
    let mut d = f64::INFINITY;
    let mut inside = n >= 3;
    for i in 0..n {
        let (p, q) = (h[i], h[(i + 1) % n]);
        d = d.min(seg_dist(o, p, q));
        if cross(p, q, o) < 0.0 {
            inside = false;
        }
    }
    if inside { 0.0 } else { d }
}
