//! Shared fixtures of the #35 pocket / slot recognition suites
//! (`pocket_recognition`, `mcp_pockets`): watertight, outward-wound
//! blocks built in code - a convex pocket of any outline sunk in a
//! block (rectangle, rounded rectangle, stadium), and an axis-aligned
//! height field for slots, steps and bosses. Not a suite itself.

#![allow(dead_code, clippy::needless_range_loop)]

use camcraft_app::geometry::{Mesh, Point3, Triangle};
use std::path::Path;

/// The stock turn that makes the program sim refuse: [`OBLIQUE`] rad
/// about Z when `rotated`, else none. The same device as
/// `area_support::stock_turn`. #2177: a quarter turn is no such device -
/// the sim judges a square-turned box.
pub fn stock_turn(rotated: bool) -> [f64; 4] {
    if rotated { camcraft_app::geometry::quat_from_yaw(OBLIQUE) } else { camcraft_app::schema::Location::default().rot }
}

/// A turn about Z off the machine axes (rad); a point 40 mm from the
/// turn's centre moves 4e-5 mm.
pub const OBLIQUE: f64 = 1e-6;

/// The device holds: the program sim refuses this stock by name
/// (`stock_follow::OBLIQUE_STOCK`).
pub fn oblique_refused(world: &flecs_ecs::prelude::World) -> bool {
    camcraft_app::stock_sim::simulate_document(world).is_err_and(|e| e.contains(camcraft_app::stock_follow::OBLIQUE_STOCK))
}

/// One facet, wound so its normal agrees with `n`; a sliver is dropped.
fn tri(out: &mut Vec<Triangle>, a: Point3, b: Point3, c: Point3, n: [f64; 3]) {
    let (u, v) = (b - a, c - a);
    let k = [u.y * v.z - u.z * v.y, u.z * v.x - u.x * v.z, u.x * v.y - u.y * v.x];
    let along = k[0] * n[0] + k[1] * n[1] + k[2] * n[2];
    if along.abs() < 1e-12 {
        return;
    }
    out.push(if along > 0.0 { Triangle::new(a, b, c) } else { Triangle::new(a, c, b) });
}

fn quad(out: &mut Vec<Triangle>, q: [Point3; 4], n: [f64; 3]) {
    tri(out, q[0], q[1], q[2], n);
    tri(out, q[0], q[2], q[3], n);
}

fn p3(p: (f64, f64), z: f64) -> Point3 {
    Point3::new(p.0, p.1, z)
}

/// A rounded rectangle's ring, counter-clockwise, centred on the
/// origin: half sizes `a` x `b`, corner radius `r` (0 = sharp; `r = b`
/// is a stadium), `seg` chords per quarter arc.
pub fn rounded_rect(a: f64, b: f64, r: f64, seg: usize) -> Vec<(f64, f64)> {
    let mut out: Vec<(f64, f64)> = Vec::new();
    let centres = [(a - r, b - r), (-(a - r), b - r), (-(a - r), -(b - r)), (a - r, -(b - r))];
    for (q, c) in centres.iter().enumerate() {
        for k in 0..=seg {
            let t = (q as f64 + k as f64 / seg as f64) * std::f64::consts::FRAC_PI_2;
            let p = (c.0 + r * t.cos(), c.1 + r * t.sin());
            if out.last().is_none_or(|l| (l.0 - p.0).hypot(l.1 - p.1) > 1e-9) {
                out.push(p);
            }
        }
    }
    if out.len() > 1 && out.first().zip(out.last()).is_some_and(|(f, l)| (f.0 - l.0).hypot(f.1 - l.1) <= 1e-9) {
        out.pop();
    }
    out
}

/// A `2A x 2B x [0, top]` block with a blind pocket of the convex,
/// counter-clockwise `ring` (around the origin) sunk to `floor`.
pub fn block_with_pocket(big_a: f64, big_b: f64, ring: &[(f64, f64)], floor: f64, top: f64) -> Mesh {
    let mut f = Vec::new();
    let n = ring.len();
    let hit = |p: (f64, f64)| {
        let t = (big_a / p.0.abs()).min(big_b / p.1.abs());
        (p.0 * t, p.1 * t)
    };
    let ang = |p: (f64, f64)| p.1.atan2(p.0).rem_euclid(std::f64::consts::TAU);
    let corners = [(big_a, big_b), (-big_a, big_b), (-big_a, -big_b), (big_a, -big_b)];
    let mut outer: Vec<(f64, f64)> = Vec::new();
    for i in 0..n {
        let (p, q) = (ring[i], ring[(i + 1) % n]);
        let (qi, qn) = (hit(p), hit(q));
        let (a0, mut a1) = (ang(qi), ang(qn));
        if a1 <= a0 {
            a1 += std::f64::consts::TAU;
        }
        let mut between: Vec<(f64, (f64, f64))> = corners
            .iter()
            .filter_map(|&c| {
                let mut ac = ang(c);
                if ac <= a0 {
                    ac += std::f64::consts::TAU;
                }
                (ac > a0 + 1e-9 && ac < a1 - 1e-9).then_some((ac, c))
            })
            .collect();
        between.sort_by(|x, y| x.0.total_cmp(&y.0));
        // The top face's sector: inner chord, ray, rim, ray - convex.
        let mut poly = vec![p3(p, top), p3(qi, top)];
        poly.extend(between.iter().map(|(_, c)| p3(*c, top)));
        poly.push(p3(qn, top));
        poly.push(p3(q, top));
        for k in 1..poly.len() - 1 {
            tri(&mut f, poly[0], poly[k], poly[k + 1], [0.0, 0.0, 1.0]);
        }
        outer.push(qi);
        outer.extend(between.iter().map(|(_, c)| *c));
    }
    let m = outer.len();
    for i in 0..m {
        let (a, b) = (outer[i], outer[(i + 1) % m]);
        quad(&mut f, [p3(a, 0.0), p3(b, 0.0), p3(b, top), p3(a, top)], [b.1 - a.1, a.0 - b.0, 0.0]);
        tri(&mut f, p3((0.0, 0.0), 0.0), p3(a, 0.0), p3(b, 0.0), [0.0, 0.0, -1.0]);
    }
    for i in 0..n {
        let (a, b) = (ring[i], ring[(i + 1) % n]);
        quad(&mut f, [p3(a, floor), p3(b, floor), p3(b, top), p3(a, top)], [a.1 - b.1, b.0 - a.0, 0.0]);
        tri(&mut f, p3((0.0, 0.0), floor), p3(a, floor), p3(b, floor), [0.0, 0.0, 1.0]);
    }
    Mesh::from_facets(f)
}

/// The 40 x 40 x `top` block on [-20, 20]^2 with a radius-`rp` round
/// pocket (128 chords) sunk to `floor` - the #1585 / #1593 / #1595
/// fixture.
pub fn round_pocket_block(rp: f64, floor: f64, top: f64) -> Mesh {
    let ring: Vec<(f64, f64)> = (0..128)
        .map(|i| {
            let a = std::f64::consts::TAU * f64::from(i) / 128.0;
            (rp * a.cos(), rp * a.sin())
        })
        .collect();
    block_with_pocket(20.0, 20.0, &ring, floor, top)
}

/// An axis-aligned height field: cell `(i, j)` spans `xs[i]..xs[i+1]` x
/// `ys[j]..ys[j+1]` and stands `h(i, j)` tall (> 0). Every vertical wall
/// is split at every height in the field so no edge ends mid-edge
/// (watertight without T-junctions).
pub fn height_field(xs: &[f64], ys: &[f64], h: impl Fn(usize, usize) -> f64) -> Mesh {
    let (nx, ny) = (xs.len() - 1, ys.len() - 1);
    let mut levels = vec![0.0];
    for i in 0..nx {
        for j in 0..ny {
            levels.push(h(i, j));
        }
    }
    levels.sort_by(f64::total_cmp);
    levels.dedup();
    let cell = |i: isize, j: isize| -> f64 {
        if i < 0 || j < 0 || i as usize >= nx || j as usize >= ny { 0.0 } else { h(i as usize, j as usize) }
    };
    let mut f = Vec::new();
    for i in 0..nx {
        for j in 0..ny {
            let (x0, x1, y0, y1, z) = (xs[i], xs[i + 1], ys[j], ys[j + 1], h(i, j));
            quad(&mut f, [p3((x0, y0), z), p3((x1, y0), z), p3((x1, y1), z), p3((x0, y1), z)], [0.0, 0.0, 1.0]);
            quad(&mut f, [p3((x0, y0), 0.0), p3((x1, y0), 0.0), p3((x1, y1), 0.0), p3((x0, y1), 0.0)], [0.0, 0.0, -1.0]);
        }
    }
    let mut wall = |a: (f64, f64), b: (f64, f64), lo: f64, hi: f64, n: [f64; 3]| {
        let cuts: Vec<f64> = levels.iter().copied().filter(|z| *z >= lo && *z <= hi).collect();
        for w in cuts.windows(2) {
            quad(&mut f, [p3(a, w[0]), p3(b, w[0]), p3(b, w[1]), p3(a, w[1])], n);
        }
    };
    for i in 0..=nx {
        for j in 0..ny {
            let (l, r) = (cell(i as isize - 1, j as isize), cell(i as isize, j as isize));
            if l != r {
                let n = if l > r { [1.0, 0.0, 0.0] } else { [-1.0, 0.0, 0.0] };
                wall((xs[i], ys[j]), (xs[i], ys[j + 1]), l.min(r), l.max(r), n);
            }
        }
    }
    for j in 0..=ny {
        for i in 0..nx {
            let (d, u) = (cell(i as isize, j as isize - 1), cell(i as isize, j as isize));
            if d != u {
                let n = if d > u { [0.0, 1.0, 0.0] } else { [0.0, -1.0, 0.0] };
                wall((xs[i], ys[j]), (xs[i + 1], ys[j]), d.min(u), d.max(u), n);
            }
        }
    }
    Mesh::from_facets(f)
}

/// `mesh` as ASCII STL at `path`, for the MCP importer.
pub fn write_stl(mesh: &Mesh, path: &Path) {
    let mut s = String::from("solid part\n");
    for t in mesh.facets.iter() {
        s.push_str(" facet normal 0 0 0\n  outer loop\n");
        for p in [t.a, t.b, t.c] {
            s.push_str(&format!("   vertex {} {} {}\n", p.x, p.y, p.z));
        }
        s.push_str("  endloop\n endfacet\n");
    }
    s.push_str("endsolid part\n");
    std::fs::write(path, s).expect("write stl");
}
