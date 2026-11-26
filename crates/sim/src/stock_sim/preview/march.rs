//! The marching eye: what `preview` walks per pixel. Sphere tracing
//! (Hart, SIGGRAPH 1996) over a trilinear blend of the stock's own
//! lattice - the laws, and the sub-cell exception, are stated on the
//! module root (`super`); this file is their machinery.

use std::collections::HashMap;

use crate::geometry::Point3;
use crate::stock_sim::field::{center_of, signed, SdfField};

/// March budget, the same number as `trace.wgsl`'s `MAX_STEPS`. Over a
/// flat face each step is `RELAX` times the height, so a grazing ray
/// converges geometrically: 64 steps ran out below about 10 degrees
/// and holed a faced floor (#3612). 256 resolves a faced floor down to
/// 1 degree (measured: `a_grazing_faced_floor_resolves_and_stays_see_through`).
/// Only the rays that need the steps pay for them.
const MAX_STEPS: u32 = 256;

/// Bisections of the exhaustion bracket `[t, t + cell]`: 6 halvings
/// land within a 64th of a cell, the hit threshold `STEP_EPS`.
const BISECT: u32 = 6;

/// The hit threshold, in cells: below a 64th of a cell the probe is
/// at the surface's own resolution (a 64th of a cell). The
/// blend converges geometrically toward it; the residual is the
/// lattice's own slack, not the march's.
pub(super) const STEP_EPS: f64 = 1.0 / 64.0;

/// How much of a certified-empty step the march dares take. A raw
/// lattice value is only a lower bound after blending (a few
/// hundredths of a cell), and a relaxed bound relaxes forever short
/// of the surface, never past it - so 0.9 buys convergence at the
/// price of 10% of the steps. The AABB gap jump needs no relaxation:
/// it is exact, not interpolated.
const RELAX: f64 = 0.9;

/// The light this viewer shades by: fixed, up-and-forward, matching
/// the sheet render's studio so the two paths agree on which faces
/// catch the lamp.
const LIGHT: Point3 = Point3 {
    x: -0.35,
    y: -0.5,
    z: 0.79,
};

/// What one ray saw.
pub(super) enum Seen {
    Hit { t: f64, p: Point3 },
    /// Traced the whole world box with material never found.
    Empty,
    /// The march could not finish inside its budget: no verdict.
    Unreadable,
}

/// The per-frame march state: the field, the column-parity cache
/// (the render-side acceleration the field's header promised), and
/// the two boxes the trace spends.
pub(super) struct Eye<'a> {
    pub f: &'a SdfField,
    pub cols: HashMap<[i32; 2], Option<Vec<f64>>>,
    pub solid: (Point3, Point3),
    pub world: (Point3, Point3),
    pub cell: f64,
    pub eps: f64,
}

impl Eye<'_> {
    /// The marched distance at `p`: the field's stored values are per
    /// CELL, and a trace that trusts a piecewise-constant field
    /// orbits planes instead of landing on them - so the march walks
    /// the trilinear blend of the lattice (each corner its cell's
    /// signed magnitude wearing its own column's cached parity, the
    /// cut layer interpolated the same way with its no-claim floor
    /// held at `-4 cap`). A blend of near-exact samples stays within
    /// the field's own cell-scale slack, which is what the budget
    /// needs to converge and what the sub-cell exception is stated
    /// against.
    pub fn dist(&mut self, p: Point3) -> f64 {
        if !p.is_finite() {
            return f64::NAN;
        }
        let cell = self.cell;
        // Centers sit at (i + 0.5) * cell: corner coords, then blend.
        let (fx, fy, fz) = (p.x / cell - 0.5, p.y / cell - 0.5, p.z / cell - 0.5);
        let (ix, iy, iz) = (fx.floor() as i32, fy.floor() as i32, fz.floor() as i32);
        let (wx, wy, wz) = (fx - ix as f64, fy - iy as f64, fz - iz as f64);
        let mut corner = |dx: i32, dy: i32, dz: i32| -> (f64, f64) {
            let key = [ix + dx, iy + dy, iz + dz];
            let mag = signed(self.f.mag_at(key), self.parity(key, center_of(iz + dz, cell)));
            let cut = self.f.cut_at(key);
            (mag, if cut == f64::NEG_INFINITY { -4.0 * self.f.cap() } else { cut })
        };
        let mut c = [(0.0, 0.0); 8];
        for (n, (dx, dy, dz)) in c.iter_mut().zip(
            [(0, 0, 0), (1, 0, 0), (0, 1, 0), (1, 1, 0), (0, 0, 1), (1, 0, 1), (0, 1, 1), (1, 1, 1)],
        ) {
            *n = corner(dx, dy, dz);
        }
        let lerp = |a: f64, b: f64, w: f64| a + (b - a) * w;
        // A slot is dx + 2*dy + 4*dz (the corner order above): blend
        // z, then x, then y.
        let m = |i: usize| lerp(c[i].0, c[i + 4].0, wz);
        let k = |i: usize| lerp(c[i].1, c[i + 4].1, wz);
        let mesh = lerp(lerp(m(0), m(1), wx), lerp(m(2), m(3), wx), wy);
        let cut = lerp(lerp(k(0), k(1), wx), lerp(k(2), k(3), wx), wy);
        mesh.max(cut)
    }

    /// The cached crossings of the column at cell `(i, j)`, asked
    /// once per column per frame (the render-side acceleration the
    /// field's header deferred to this stage).
    fn parity(&mut self, key: [i32; 3], z: f64) -> bool {
        let ck = [key[0], key[1]];
        if !self.cols.contains_key(&ck) {
            let cx = self.f.shell_crossings(center_of(ck[0], self.cell), center_of(ck[1], self.cell));
            self.cols.insert(ck, cx);
        }
        // `get`, not `cloned`: the crossings vector rides the hot
        // loop and one column scan already fills a frame's cache.
        SdfField::parity_of(self.cols.get(&ck).unwrap_or(&None), z)
    }
}

/// Hart's march: from the world box entry, step by the lower bound
/// until the surface is within `eps`, the box is crossed, or the
/// budget says this field cannot answer the ray.
pub(super) fn trace(e: &mut Eye, o: Point3, d: Point3) -> Seen {
    let (mut t0, mut t1) = (f64::NEG_INFINITY, f64::INFINITY);
    for axis in 0..3 {
        let (od, oo) = (comp(&d, axis), comp(&o, axis));
        let (lo, hi) = (comp(&e.world.0, axis), comp(&e.world.1, axis));
        if od.abs() < 1e-12 {
            if oo < lo || oo > hi {
                return Seen::Empty;
            }
            continue;
        }
        let inv = 1.0 / od;
        let (mut ta, mut tb) = ((lo - oo) * inv, (hi - oo) * inv);
        if ta > tb {
            std::mem::swap(&mut ta, &mut tb);
        }
        t0 = t0.max(ta);
        t1 = t1.min(tb);
    }
    if t1 < t0.max(0.0) {
        return Seen::Empty;
    }
    let mut t = t0.max(0.0);
    for _ in 0..MAX_STEPS {
        let p = add(o, scale(d, t));
        let phi = e.dist(p);
        if phi.is_nan() {
            return Seen::Unreadable;
        }
        if phi <= e.eps {
            // A hit candidate. It stands iff material proves AT the
            // point or BEHIND the surface, deeper along the ray: `p`
            // itself negative, or the point one cell further in
            // inside - the same law the verdict march answers
            // heights by, standing at the ceiling and climbing down
            // to the topmost material. The live SIGN at `p` cannot
            // vote here: a column's parity counts crossings strictly
            // above z, so a probe standing exactly on the surface
            // reads air, and demanding otherwise would reject every
            // honest hit on a lattice plane. A phantom - blend
            // grazing zero inside a pocket of air, wall a full cell
            // further in - creeps forward by a few eps and marches
            // on, paying steps, never skipping material.
            if phi < 0.0 || e.dist(add(p, scale(d, e.cell))) < 0.0 {
                return Seen::Hit { t, p };
            }
            t += e.eps * 4.0;
            if t > t1 {
                return Seen::Empty;
            }
            continue;
        }
        // Air: the ball of radius `phi` is empty. Past the solid's
        // box, the box gap is empty too, exact and usually much
        // bigger - the same jump `march` trusts outside the shell.
        let gap = outside_gap(p, e.solid.0, e.solid.1);
        let nt = t + phi.max(gap) * RELAX;
        if nt <= t {
            return Seen::Unreadable; // the budget spent itself on rounding
        }
        if nt > t1 {
            return Seen::Empty; // the step crossed out of the world empty
        }
        t = nt;
    }
    exhausted(e, o, d, t)
}

/// The budget ran out at `t` (#3612). A verdict stands only where
/// material is PROVED on the ray: the last sample inside, or the
/// sample one cell further on inside - the hit law's own probe. Then
/// the surface lies in `[t, t + cell]` and a bisection of that bracket
/// lands within `cell / 2^BISECT` of it, on the material side. Any
/// other ray keeps no verdict: a near miss is never painted.
fn exhausted(e: &mut Eye, o: Point3, d: Point3, t: f64) -> Seen {
    let p = add(o, scale(d, t));
    let phi = e.dist(p);
    if phi.is_nan() {
        return Seen::Unreadable;
    }
    if phi < 0.0 {
        return Seen::Hit { t, p };
    }
    let mut hi = t + e.cell;
    let ahead = e.dist(add(o, scale(d, hi)));
    if ahead.is_nan() || ahead >= 0.0 {
        return Seen::Unreadable;
    }
    let mut lo = t;
    for _ in 0..BISECT {
        let mid = 0.5 * (lo + hi);
        if e.dist(add(o, scale(d, mid))) < 0.0 {
            hi = mid;
        } else {
            lo = mid;
        }
    }
    Seen::Hit { t: hi, p: add(o, scale(d, hi)) }
}

/// Lambert times a one-march ambient: the gradient is the normal (a
/// distance field's gradient IS the normal), and the occlusion asks
/// only whether anything stands over the point along it.
pub(super) fn shade(e: &mut Eye, p: Point3) -> f32 {
    let h = e.cell * 0.5;
    let mut g = |i: usize| {
        let k = axis(i, h);
        e.dist(add(p, k)) - e.dist(sub(p, k))
    };
    let n = match unit(Point3::new(g(0), g(1), g(2))) {
        Some(n) => n,
        None => return 0.35, // a flat or broken gradient: ambient only, honest either way
    };
    let mut ao = 1.0f64;
    let span = 4.0 * e.cell;
    let mut s = e.cell * 0.1 + 2.0 * e.eps;
    for _ in 0..16 {
        let d = e.dist(add(p, scale(n, s)));
        if d.is_nan() {
            break;
        }
        if d <= e.eps {
            ao = 1.0 - (s / span).clamp(0.0, 1.0);
            break;
        }
        s += d.max(e.cell * 0.01);
        if s > span {
            break;
        }
    }
    let light = unit(LIGHT).unwrap_or(LIGHT); // finite and nonzero by construction
    let lambert = dot(n, light).clamp(0.0, 1.0);
    ((0.18 + 0.82 * lambert) * (0.7 + 0.3 * ao)) as f32
}

/// `k` along axis `i`, zero elsewhere: the gradient stencil's steps.
fn axis(i: usize, k: f64) -> Point3 {
    Point3::new(
        if i == 0 { k } else { 0.0 },
        if i == 1 { k } else { 0.0 },
        if i == 2 { k } else { 0.0 },
    )
}

/// The gap from `p` to the box (0 inside): an exact empty-ball radius
/// while the remaining solid lives inside the box, which cuts do not
/// change - removal only moves the surface INTO the box.
fn outside_gap(p: Point3, lo: Point3, hi: Point3) -> f64 {
    let mut s2 = 0.0;
    for axis in 0..3 {
        let (q, a, b) = (comp(&p, axis), comp(&lo, axis), comp(&hi, axis));
        if q < a {
            s2 += (a - q) * (a - q);
        } else if q > b {
            s2 += (q - b) * (q - b);
        }
    }
    s2.sqrt()
}

// - Point3 carries Sub, Dot and Cross; the march needs the rest. -
pub(super) fn comp(p: &Point3, axis: usize) -> f64 {
    match axis {
        0 => p.x,
        1 => p.y,
        _ => p.z,
    }
}
pub(super) fn add(a: Point3, b: Point3) -> Point3 {
    Point3::new(a.x + b.x, a.y + b.y, a.z + b.z)
}
pub(super) fn sub(a: Point3, b: Point3) -> Point3 {
    Point3::new(a.x - b.x, a.y - b.y, a.z - b.z)
}
pub(super) fn scale(a: Point3, s: f64) -> Point3 {
    Point3::new(a.x * s, a.y * s, a.z * s)
}
pub(super) fn dot(a: Point3, b: Point3) -> f64 {
    a.x * b.x + a.y * b.y + a.z * b.z
}
pub(super) fn unit(a: Point3) -> Option<Point3> {
    if !a.is_finite() {
        return None;
    }
    let n = dot(a, a).sqrt();
    if n < 1e-30 {
        return None;
    }
    Some(scale(a, 1.0 / n))
}
