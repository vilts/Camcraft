//! The solid-law harness's finished parts (#862 split it out of
//! solid_law_goldens.rs, which stood at the 400-line law): ccw loops,
//! regions over them, and closed plate shells - walls, faces fanned
//! from the hole's centroid, hole walls and floors - modelled as an
//! import would hand them over (round holes as inscribed n-gons).
//!
//! Provenance: our own fixtures over our own goldens; no vendor
//! material consulted.

use camcraft_app::geo2d::{Loop, Point2, Region};
use camcraft_app::geometry::{Point3, Triangle};
use camcraft_app::verify::ToolGeom;

pub type P2 = (f64, f64);

pub fn rect(x0: f64, y0: f64, x1: f64, y1: f64) -> Vec<P2> {
    vec![(x0, y0), (x1, y0), (x1, y1), (x0, y1)]
}

pub fn circle(c: P2, r: f64, n: usize) -> Vec<P2> {
    (0..n).map(|k| {
        let t = std::f64::consts::TAU * k as f64 / n as f64;
        (c.0 + r * t.cos(), c.1 + r * t.sin())
    }).collect()
}

pub fn region(loops: &[Vec<P2>]) -> Region {
    Region::new(loops.iter().map(|l| Loop::new(l.iter().map(|&(x, y)| Point2::new(x, y)).collect()).expect("loop")).collect())
        .expect("region")
}

/// Walls over a ccw loop, facing out of it (`out`) or into it (a hole).
pub fn walls(l: &[P2], z0: f64, z1: f64, out: bool, f: &mut Vec<Triangle>) {
    let mut v = l.to_vec();
    if !out {
        v.reverse();
    }
    for k in 0..v.len() {
        let (a, b) = (v[k], v[(k + 1) % v.len()]);
        let q = |p: P2, z: f64| Point3::new(p.0, p.1, z);
        f.push(Triangle::new(q(a, z0), q(b, z0), q(b, z1)));
        f.push(Triangle::new(q(a, z0), q(b, z1), q(a, z1)));
    }
}

/// A horizontal face: the convex ccw loop `o`, less the convex ccw loop
/// `i` inside it. Each sector between two rays from `i`'s centroid
/// through consecutive `i` vertices is a convex polygon (inner edge, ray,
/// outer boundary with its corners, ray), fanned; every triangle is
/// turned ccw and the total checked against the area difference - the
/// harness proves its own fixture tiles the face.
pub fn cap(o: &[P2], i: Option<&[P2]>, z: f64, up: bool, f: &mut Vec<Triangle>) {
    let area = |l: &[P2]| (0..l.len()).map(|k| { let (a, b) = (l[k], l[(k + 1) % l.len()]); a.0 * b.1 - b.0 * a.1 }).sum::<f64>() * 0.5;
    let p = |q: P2| Point3::new(q.0, q.1, z);
    let mut tris: Vec<[P2; 3]> = Vec::new();
    match i {
        None => (1..o.len() - 1).for_each(|k| tris.push([o[0], o[k], o[k + 1]])),
        Some(i) => {
            let c = i.iter().fold((0.0, 0.0), |s, q| (s.0 + q.0, s.1 + q.1));
            let c = (c.0 / i.len() as f64, c.1 / i.len() as f64);
            let tau = std::f64::consts::TAU;
            let ang = |q: P2| (q.1 - c.1).atan2(q.0 - c.0).rem_euclid(tau);
            // Where the ray from c through q leaves `o`.
            let hit = |q: P2| -> P2 {
                let (dx, dy) = (q.0 - c.0, q.1 - c.1);
                let mut best = f64::INFINITY;
                for k in 0..o.len() {
                    let (a, b) = (o[k], o[(k + 1) % o.len()]);
                    let (ex, ey) = (b.0 - a.0, b.1 - a.1);
                    let den = dx * ey - dy * ex;
                    if den.abs() < 1e-15 {
                        continue;
                    }
                    let t = ((a.0 - c.0) * ey - (a.1 - c.1) * ex) / den;
                    let u = ((a.0 - c.0) * dy - (a.1 - c.1) * dx) / den;
                    if t > 0.0 && (-1e-12..=1.0 + 1e-12).contains(&u) {
                        best = best.min(t);
                    }
                }
                (c.0 + dx * best, c.1 + dy * best)
            };
            for k in 0..i.len() {
                let (ia, ib) = (i[k], i[(k + 1) % i.len()]);
                let (t0, mut t1) = (ang(ia), ang(ib));
                if t1 <= t0 {
                    t1 += tau;
                }
                let mut corners: Vec<(f64, P2)> = o.iter().map(|&q| {
                    let mut t = ang(q);
                    if t <= t0 {
                        t += tau;
                    }
                    (t, q)
                }).filter(|&(t, _)| t < t1).collect();
                corners.sort_by(|a, b| a.0.total_cmp(&b.0));
                let mut poly = vec![ia, hit(ia)];
                poly.extend(corners.into_iter().map(|(_, q)| q));
                poly.push(hit(ib));
                poly.push(ib);
                for j in 1..poly.len() - 1 {
                    tris.push([ia, poly[j], poly[j + 1]]);
                }
            }
        }
    }
    let want = area(o) - i.map_or(0.0, area);
    let mut got = 0.0;
    for [a, b, c] in tris {
        let s = ((b.0 - a.0) * (c.1 - a.1) - (b.1 - a.1) * (c.0 - a.0)) * 0.5;
        if s.abs() < 1e-12 {
            continue; // a sliver where a ray meets a corner
        }
        let (b, c) = if s > 0.0 { (b, c) } else { (c, b) };
        got += s.abs();
        f.push(if up { Triangle::new(p(a), p(b), p(c)) } else { Triangle::new(p(a), p(c), p(b)) });
    }
    assert!((got - want).abs() < 1e-6 * want.abs().max(1.0), "cap tiles its face: {got} vs {want}");
}

/// A plate `o` from z0 to z1 with at most one hole `h` from `hz` (z0
/// for through) to the top; a blind hole gets its floor.
pub fn plate(o: &[P2], h: Option<(&[P2], f64)>, z0: f64, z1: f64, f: &mut Vec<Triangle>) {
    walls(o, z0, z1, true, f);
    plate_cell(o, h, z0, z1, f);
}

/// One cell of a plate's faces (caps, hole walls and floor) - cells
/// tiling a plate share its outer walls, never walls of their own, so
/// the shell stays one closed surface.
pub fn plate_cell(o: &[P2], h: Option<(&[P2], f64)>, z0: f64, z1: f64, f: &mut Vec<Triangle>) {
    match h {
        None => {
            cap(o, None, z0, false, f);
            cap(o, None, z1, true, f);
        }
        Some((h, hz)) => {
            cap(o, Some(h), z1, true, f);
            walls(h, hz, z1, false, f);
            if hz > z0 {
                cap(h, None, hz, true, f);
                cap(o, None, z0, false, f);
            } else {
                cap(o, Some(h), z0, false, f);
            }
        }
    }
}

pub fn mill(d: f64) -> ToolGeom {
    ToolGeom { radius: d * 0.5, nose_radius: 0.0, flute_len: 25.0, shank_radius: d * 0.5, shank_len: 30.0, holder_radius: 16.0, holder_len: 40.0 }
}
