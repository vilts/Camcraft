//! Shared fixture of the cross-drilling suites (issue #117): a closed
//! round bar about world Z with its radial blind holes MODELLED - each
//! hole's mouth cut out of the bar's face, its wall and flat floor
//! welded in - so a cross drill to the modelled depth leaves the
//! finished part untouched, the way the radial part law now demands.
//! Not a suite itself.
//!
//! Construction: the face is a (theta, z) grid; the block of cells
//! about each hole is replaced by a ring zipped between the block's
//! boundary vertices and the mouth curve (the hole cylinder meeting the
//! bar's, solved per angle), so every edge is shared by exactly two
//! facets. Provenance: our own fixture; line-cylinder intersection and
//! the two-loop zip triangulation are textbook. No vendor material.

#![allow(dead_code)]

use crate::drilling::CrossHole;
use crate::geometry::{Mesh, Point3, Triangle};

/// The drilled bar's Z span: the top stays at the suites' z20 (Top of
/// Part, the end face the detours swing over), the foot runs to z-40.
/// The suites' old r10 x 40 bar (L/D 2) sat exactly on the lathe axis
/// reader's isotropy door - its end caps' normal moment equal to half
/// fits" (#152); L/D 5 stands well clear of it, and keeps the holes'
/// walls and floors a small share of the surface the fit must find
/// revolved.
pub const FOOT: f64 = -80.0;
pub const TOP: f64 = 20.0;

const TAU: f64 = std::f64::consts::TAU;
const SEGS: usize = 64;
const MOUTH: usize = 32;

fn sub(a: Point3, b: Point3) -> Point3 {
    Point3::new(a.x - b.x, a.y - b.y, a.z - b.z)
}

fn cross3(a: Point3, b: Point3) -> Point3 {
    Point3::new(
        a.y * b.z - a.z * b.y,
        a.z * b.x - a.x * b.z,
        a.x * b.y - a.y * b.x,
    )
}

/// A facet turned so its normal leans along `want`.
fn facing(a: Point3, b: Point3, c: Point3, want: Point3) -> Triangle {
    if cross3(sub(b, a), sub(c, a)).dot(want) < 0.0 {
        Triangle::new(a, c, b)
    } else {
        Triangle::new(a, b, c)
    }
}

fn on_face(r: f64, th: f64, z: f64) -> Point3 {
    Point3::new(r * th.cos(), r * th.sin(), z)
}

/// The hole's frame: outward axis `a` (against the plunge), lateral `u`
/// in XY, and Z.
fn frame(h: &CrossHole) -> (Point3, Point3) {
    let a = Point3::new(-h.dir.x, -h.dir.y, 0.0);
    (a, Point3::new(-a.y, a.x, 0.0))
}

/// The hole cylinder's line at angle `phi` about the hole axis.
fn line_at(h: &CrossHole, phi: f64) -> (Point3, Point3) {
    let (a, u) = frame(h);
    let rh = h.radius;
    let p = Point3::new(
        h.origin.x + u.x * rh * phi.cos(),
        h.origin.y + u.y * rh * phi.cos(),
        h.origin.z + rh * phi.sin(),
    );
    (p, a)
}

/// Where that line (outward along `a`) meets the bar's face of radius `r`.
fn mouth_point(h: &CrossHole, phi: f64, r: f64) -> Point3 {
    let (p, a) = line_at(h, phi);
    // |p_xy + t a_xy| = r, the outer root.
    let (aa, bb, cc) = (
        a.x * a.x + a.y * a.y,
        2.0 * (p.x * a.x + p.y * a.y),
        p.x * p.x + p.y * p.y - r * r,
    );
    let t = (-bb + (bb * bb - 4.0 * aa * cc).max(0.0).sqrt()) / (2.0 * aa);
    Point3::new(p.x + a.x * t, p.y + a.y * t, p.z)
}

/// `holes`, with any two that coincide merged (the deeper kept), and
/// two from opposite faces on one line through the axis that run into
/// each other made one through hole. No mirror images: since #116 the
/// lathe axis fit reads a one-sided cross hole's bar on its true axis.
fn merged(holes: &[CrossHole]) -> Vec<CrossHole> {
    let mut all: Vec<CrossHole> = Vec::new();
    for h in holes.iter().copied() {
        let same = |g: &CrossHole| {
            sub(g.origin, h.origin).dot(sub(g.origin, h.origin)) < 1e-12 && g.dir.dot(h.dir) > 0.999
        };
        match all.iter_mut().find(|g| same(g)) {
            Some(g) => g.floor_d = g.floor_d.min(h.floor_d),
            None => all.push(h),
        }
    }
    // Two holes on one line through the axis from opposite faces that
    // run into each other are one through hole: both halves end, with
    // no floor, at the line's midpoint, where their walls weld vertex
    // for vertex (the half turn maps the mouth angles onto each other).
    let n = all.len();
    for i in 0..n {
        for j in 0..n {
            let (g, h) = (all[i], all[j]);
            let d = sub(g.origin, h.origin);
            let gap = d.dot(d).sqrt();
            let line = g.dir.dot(h.dir) < -0.999 && cross3(d, g.dir).dot(cross3(d, g.dir)) < 1e-12;
            if i != j && line && -g.floor_d - h.floor_d >= gap - 1e-9 {
                all[i].floor_d = -0.5 * gap;
                all[i].through = true;
            }
        }
    }
    all
}

/// The closed bar of radius `r`, z in [`z0`, `z1`], with `holes` (blind,
/// horizontal, mouths on the face) modelled to their `floor_d` ([`merged`]).
pub fn punched_bar(r: f64, z0: f64, z1: f64, holes: &[CrossHole]) -> Mesh {
    let holes = &merged(holes)[..];
    let nz = ((z1 - z0) / 2.0).ceil().max(1.0) as usize;
    let zk = |k: usize| z0 + (z1 - z0) * k as f64 / nz as f64;
    let thj = |j: usize| TAU * (j % SEGS) as f64 / SEGS as f64;
    let cell_th = TAU / SEGS as f64;
    let cell_z = (z1 - z0) / nz as f64;
    // Each hole's removed block of cells, [j0, j1) x [k0, k1) (j may
    // run past SEGS: it wraps).
    let blocks: Vec<(usize, usize, usize, usize)> = holes
        .iter()
        .map(|h| {
            let pts: Vec<Point3> = (0..MOUTH)
                .map(|i| mouth_point(h, TAU * i as f64 / MOUTH as f64, r))
                .collect();
            let c = mouth_point(h, 0.0, r);
            let thc = c.y.atan2(c.x).rem_euclid(TAU);
            let dth = pts.iter().map(|p| {
                let d = (p.y.atan2(p.x) - thc).rem_euclid(TAU);
                if d > std::f64::consts::PI {
                    TAU - d
                } else {
                    d
                }
            });
            let half = dth.fold(0.0, f64::max);
            let jlo = ((thc - half) / cell_th).floor() as i64 - 1;
            let jhi = ((thc + half) / cell_th).ceil() as i64 + 1;
            let j0 = jlo.rem_euclid(SEGS as i64) as usize;
            let j1 = j0 + (jhi - jlo) as usize;
            let k0 = (((h.origin.z - h.radius - z0) / cell_z).floor() as i64 - 1).max(0) as usize;
            let k1 = (((h.origin.z + h.radius - z0) / cell_z).ceil() as usize + 1).min(nz);
            (j0, j1, k0, k1)
        })
        .collect();
    let removed = |j: usize, k: usize| {
        blocks.iter().any(|&(j0, j1, k0, k1)| {
            let jj = if j < j0 { j + SEGS } else { j };
            jj >= j0 && jj < j1 && k >= k0 && k < k1
        })
    };
    let mut f = Vec::new();
    let out = |p: Point3| Point3::new(p.x, p.y, 0.0);
    for j in 0..SEGS {
        for k in 0..nz {
            if removed(j, k) {
                continue;
            }
            let (a, b) = (on_face(r, thj(j), zk(k)), on_face(r, thj(j + 1), zk(k)));
            let (c, d) = (
                on_face(r, thj(j + 1), zk(k + 1)),
                on_face(r, thj(j), zk(k + 1)),
            );
            let w = out(on_face(1.0, thj(j) + 0.5 * cell_th, 0.0));
            f.push(facing(a, b, c, w));
            f.push(facing(a, c, d, w));
        }
        let (a, b) = (on_face(r, thj(j), z0), on_face(r, thj(j + 1), z0));
        f.push(facing(
            Point3::new(0.0, 0.0, z0),
            a,
            b,
            Point3::new(0.0, 0.0, -1.0),
        ));
        let (a, b) = (on_face(r, thj(j), z1), on_face(r, thj(j + 1), z1));
        f.push(facing(
            Point3::new(0.0, 0.0, z1),
            a,
            b,
            Point3::new(0.0, 0.0, 1.0),
        ));
    }
    for (h, &(j0, j1, k0, k1)) in holes.iter().zip(&blocks) {
        ring(&mut f, h, r, (j0, j1, k0, k1), &thj, &zk);
        bore(&mut f, h, r);
    }
    Mesh::from_facets(f)
}

/// Zip the block's boundary loop to the mouth curve, both ordered by
/// angle about the hole centre in the face's (arc, z) chart.
fn ring(
    f: &mut Vec<Triangle>,
    h: &CrossHole,
    r: f64,
    (j0, j1, k0, k1): (usize, usize, usize, usize),
    thj: &dyn Fn(usize) -> f64,
    zk: &dyn Fn(usize) -> f64,
) {
    let c = mouth_point(h, 0.0, r);
    let (thc, zc) = (c.y.atan2(c.x), h.origin.z);
    let ang = |p: Point3| {
        let mut d = p.y.atan2(p.x) - thc;
        while d > std::f64::consts::PI {
            d -= TAU;
        }
        while d < -std::f64::consts::PI {
            d += TAU;
        }
        (p.z - zc).atan2(r * d)
    };
    let mut outer = Vec::new();
    for j in j0..j1 {
        outer.push(on_face(r, thj(j), zk(k0)));
    }
    for k in k0..k1 {
        outer.push(on_face(r, thj(j1), zk(k)));
    }
    for j in (j0 + 1..=j1).rev() {
        outer.push(on_face(r, thj(j), zk(k1)));
    }
    for k in (k0 + 1..=k1).rev() {
        outer.push(on_face(r, thj(j0), zk(k)));
    }
    let mut inner: Vec<Point3> = (0..MOUTH)
        .map(|i| mouth_point(h, TAU * i as f64 / MOUTH as f64, r))
        .collect();
    let sort = |v: &mut Vec<Point3>| {
        let start = (0..v.len())
            .min_by(|&a, &b| ang(v[a]).total_cmp(&ang(v[b])))
            .unwrap_or(0);
        v.rotate_left(start);
        if v.len() > 2 && ang(v[1]) < ang(v[0]) {
            v[1..].reverse();
        }
    };
    sort(&mut outer);
    sort(&mut inner);
    let (n, m) = (outer.len(), inner.len());
    let ao: Vec<f64> = (0..=n)
        .map(|i| {
            if i < n {
                ang(outer[i])
            } else {
                ang(outer[0]) + TAU
            }
        })
        .collect();
    let aq: Vec<f64> = (0..=m)
        .map(|j| {
            if j < m {
                ang(inner[j])
            } else {
                ang(inner[0]) + TAU
            }
        })
        .collect();
    let w = |p: Point3| Point3::new(p.x, p.y, 0.0);
    let (mut i, mut j) = (0, 0);
    while i < n || j < m {
        let (o0, q0) = (outer[i % n], inner[j % m]);
        if j == m || (i < n && ao[i + 1] <= aq[j + 1]) {
            f.push(facing(o0, outer[(i + 1) % n], q0, w(o0)));
            i += 1;
        } else {
            f.push(facing(o0, inner[(j + 1) % m], q0, w(o0)));
            j += 1;
        }
    }
}

/// The hole's wall from the mouth curve to its flat floor, and the floor
/// (none for a through half: its wall welds to its partner's).
fn bore(f: &mut Vec<Triangle>, h: &CrossHole, r: f64) {
    let (a, _) = frame(h);
    let floor = |phi: f64| {
        let (p, _) = line_at(h, phi);
        Point3::new(p.x + a.x * h.floor_d, p.y + a.y * h.floor_d, p.z)
    };
    let ctr = Point3::new(
        h.origin.x + a.x * h.floor_d,
        h.origin.y + a.y * h.floor_d,
        h.origin.z,
    );
    for i in 0..MOUTH {
        let (p0, p1) = (
            TAU * i as f64 / MOUTH as f64,
            TAU * (i + 1) as f64 / MOUTH as f64,
        );
        let (m0, m1) = (mouth_point(h, p0, r), mouth_point(h, p1, r));
        let (b0, b1) = (floor(p0), floor(p1));
        let inward = sub(ctr, b0);
        f.push(facing(m0, m1, b1, inward));
        f.push(facing(m0, b1, b0, inward));
        if !h.through {
            f.push(facing(ctr, b0, b1, a));
        }
    }
}
