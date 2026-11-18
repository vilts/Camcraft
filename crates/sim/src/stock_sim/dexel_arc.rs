//! The column engine's exact XY-arc sweep (#402): every atom - disc,
//! ball, cone, bull nose - swept along the true circular arc of a G2 /
//! G3, the lowest steel above every column found from the arc itself,
//! never from chords. Split beside `dexel.rs` by the 400-line law.
//!
//! Provenance: the circle-point distance law of elementary plane
//! geometry (`d^2 = (R - rho)^2 + 4 R rho sin^2(u/2)` for a point at
//! `rho` from the centre and angle `u` off the arc point), RS-274's
//! helical interpolation (Z linear in the swept angle), the cutter
//! profiles of the machine-tool handbooks, and elementary convex
//! analysis for the search law below. Nothing from vendor material.
//!
//! ## The law here
//!
//! - **Why not chords.** A chord drawn inside the arc stands one
//!   sagitta nearer the arc's centre than the arc. On a convex corner
//!   the centre IS the part's corner, so the chord's disc reached a
//!   column the real tool never touches (a false gouge the post gate
//!   refused, #402); on a concave corner the part lies outside the
//!   arc, and the same chord stayed a sagitta short of the wall (a
//!   real gouge hidden). No chord placement is right on both sides at
//!   once - circumscribed chords just swap which side lies - so the
//!   sweep reads the arc exactly.
//! - **The reach window is closed-form.** A column at `rho` from the
//!   centre is within `r` of the arc point at angle `u` off its own
//!   bearing iff `4 R rho sin^2(u/2) <= r^2 - (R - rho)^2`: one window
//!   `|u| <= h` centred on the column's bearing, met by the travelled
//!   sweep `[0, span]` in up to two pieces (three bearings tried, so a
//!   window straddling the sweep's start angle is never lost). The
//!   arc's end points are judged as discs too, so an end whose radius
//!   disagrees with the arc's by rounding still cuts what it touches.
//! - **The bottom is exact.** Over a window the lowest steel is
//!   `z(tau) + lift(d(tau))`, `lift` nondecreasing in `d` and `d`
//!   nondecreasing in `|u|`. A level arc (and every disc, whose lift is
//!   zero) has its minimum at the clamped bearing or a window end -
//!   evaluated directly. A helical non-disc arc is split where the
//!   bottom's curvature can change sign (the distance's own bend at
//!   `cos u = min(R,rho)/max(R,rho)`, the ball's at its closed-form
//!   root, the bull's flat-disc edge), each piece searched by samples
//!   and a golden section (`dexel_bull::golden`). Every value is a
//!   real evaluation, so a search can only stop at or ABOVE the true
//!   minimum: the column is never lowered past where the tool went.

use super::dexel::Dexel;
use super::dexel_bull::golden;

use crate::cl::{ArcMove, Move};
use crate::geometry::Point3;
use std::f64::consts::{FRAC_PI_2, PI, TAU};

/// Interior samples per search piece before the golden section: enough
/// that the piece's minimum is bracketed beside the best sample for
/// any piece that is convex or concave, as the split law makes them.
pub(super) const SAMPLES: usize = 8;

/// Which steel sweeps the arc. Moves carry TIP positions for all four
/// (the codebase's convention); `lift` is the profile above the tip.
#[derive(Clone, Copy, Debug)]
pub(super) enum Atom {
    /// The flat end mill's disc.
    Flat,
    /// The ball nose: a sphere of the tool radius, centre `r` above the tip.
    Ball,
    /// The chamfer cone: `cot` of its half-angle from the axis.
    Cone(f64),
    /// The bull nose: its corner radius `c`, `0 < c <= r`.
    Bull(f64),
}

impl Atom {
    /// Height of the lowest steel above the tip at horizontal distance
    /// `d` (`0 <= d <= r`) from the axis.
    fn lift(self, d: f64, r: f64) -> f64 {
        match self {
            Atom::Flat => 0.0,
            Atom::Ball => r - (r * r - d * d).max(0.0).sqrt(),
            Atom::Cone(cot) => cot * d,
            Atom::Bull(c) => {
                let e = (d - (r - c)).max(0.0);
                c - (c * c - e * e).max(0.0).sqrt()
            }
        }
    }

    /// The `|u|` values (angle off the column's bearing) inside which
    /// the bottom along a helical arc may change curvature: the split
    /// points that leave each search piece convex or concave.
    fn bends(self, rad: f64, rho: f64, r: f64) -> [Option<f64>; 2] {
        let (lo, hi) = (rad.min(rho), rad.max(rho));
        // The distance to a circle point is convex in u up to here,
        // concave past it.
        let dist_bend = (hi > 0.0).then(|| (lo / hi).clamp(-1.0, 1.0).acos());
        match self {
            Atom::Flat => [None, None],
            Atom::Cone(_) => [dist_bend, None],
            Atom::Ball => {
                // z - sqrt(C + B cos u): convex wherever
                // B cos^2 u + 2 C cos u + B >= 0, which only fails
                // (for cos u below one root in (-1, 0)) when C > B.
                let (b, c) = (2.0 * rad * rho, r * r - rad * rad - rho * rho);
                let root = (b > 0.0 && c > b).then(|| ((-c + (c * c - b * b).sqrt()) / b).clamp(-1.0, 1.0).acos());
                [root, None]
            }
            Atom::Bull(c) => {
                let flat = r - c;
                let s = (flat * flat - (rad - rho).powi(2)) / (4.0 * rad * rho);
                let edge = (s > 0.0 && s < 1.0 && s.is_finite()).then(|| 2.0 * s.sqrt().asin());
                [dist_bend, edge]
            }
        }
    }
}

/// One XY arc read as a circle: centre, radius, start bearing, travel
/// direction and swept angle, with its tip end points.
pub(super) struct ArcSweep {
    pub(super) ox: f64,
    pub(super) oy: f64,
    pub(super) rad: f64,
    pub(super) a0: f64,
    pub(super) dir: f64,
    pub(super) span: f64,
    pub(super) from: Point3,
    pub(super) to: Point3,
}

impl ArcSweep {
    /// The arc of `m`, read the way `cl::arc_chords` reads it (the
    /// larger end radius, the sweep in `(0, 2 pi]` in `arc.ccw`'s
    /// direction); `None` for the degenerate arcs `arc_chords` hands
    /// back as one straight chord.
    pub(super) fn of(m: &Move, arc: ArcMove) -> Option<Self> {
        let o = arc.center;
        if !(o.x.is_finite() && o.y.is_finite()) {
            return None;
        }
        let rad = (m.from.x - o.x).hypot(m.from.y - o.y).max((m.to.x - o.x).hypot(m.to.y - o.y));
        if !(rad > f64::EPSILON && rad.is_finite()) {
            return None;
        }
        let a0 = (m.from.y - o.y).atan2(m.from.x - o.x);
        let a1 = (m.to.y - o.y).atan2(m.to.x - o.x);
        let d = if arc.ccw { a1 - a0 } else { a0 - a1 };
        if !d.is_finite() {
            return None;
        }
        let span = d.rem_euclid(TAU);
        if span <= 0.0 {
            return None;
        }
        let dir = if arc.ccw { 1.0 } else { -1.0 };
        Some(ArcSweep { ox: o.x, oy: o.y, rad, a0, dir, span, from: m.from, to: m.to })
    }

    /// Tip Z after `tau` radians of travel (the helix law).
    fn z(&self, tau: f64) -> f64 {
        self.from.z + (self.to.z - self.from.z) * (tau / self.span)
    }

    /// The XY box the arc's path covers: its ends and every axis
    /// extreme the sweep passes.
    pub(super) fn bbox(&self) -> (f64, f64, f64, f64) {
        let (mut x0, mut x1) = (self.from.x.min(self.to.x), self.from.x.max(self.to.x));
        let (mut y0, mut y1) = (self.from.y.min(self.to.y), self.from.y.max(self.to.y));
        for (k, ang) in [0.0, FRAC_PI_2, PI, 3.0 * FRAC_PI_2].into_iter().enumerate() {
            if (self.dir * (ang - self.a0)).rem_euclid(TAU) <= self.span {
                match k {
                    0 => x1 = x1.max(self.ox + self.rad),
                    1 => y1 = y1.max(self.oy + self.rad),
                    2 => x0 = x0.min(self.ox - self.rad),
                    _ => y0 = y0.min(self.oy - self.rad),
                }
            }
        }
        (x0, x1, y0, y1)
    }
}

impl Dexel {
    /// Sweep `atom` (tool radius `r`) along the XY arc of `m`, exactly.
    /// A degenerate arc is the straight chord `arc_chords` would have
    /// made of it, cut by the atom's own straight kernel.
    pub(super) fn cut_xy_arc(&mut self, m: &Move, arc: ArcMove, r: f64, atom: Atom) {
        if !(r.is_finite() && r >= 0.0) {
            return; // the cut_chord gate: validation lives upstream
        }
        let Some(sw) = ArcSweep::of(m, arc) else {
            match atom {
                Atom::Flat => self.cut_chord(m.from, m.to, r),
                Atom::Ball => self.cut_ball(
                    Point3::new(m.from.x, m.from.y, m.from.z + r),
                    Point3::new(m.to.x, m.to.y, m.to.z + r),
                    r,
                ),
                Atom::Cone(cot) => self.cut_cone(m.from, m.to, r, cot),
                Atom::Bull(c) => self.cut_bull(m.from, m.to, r, c),
            }
            return;
        };
        let (bx0, bx1, by0, by1) = sw.bbox();
        let fx = |v: f64| ((v - self.x0) / self.cell).floor();
        let fy = |v: f64| ((v - self.y0) / self.cell).floor();
        let lo_x = fx(bx0 - r).max(0.0) as usize;
        let hi_x = fx(bx1 + r).clamp(0.0, self.nx as f64 - 1.0) as usize;
        let lo_y = fy(by0 - r).max(0.0) as usize;
        let hi_y = fy(by1 + r).clamp(0.0, self.ny as f64 - 1.0) as usize;
        if lo_x >= self.nx || lo_y >= self.ny {
            return;
        }
        for iy in lo_y..=hi_y {
            for ix in lo_x..=hi_x {
                let q = self.center(ix, iy);
                let cut = column_cut(&sw, q.x, q.y, r, atom);
                if cut.is_finite() {
                    self.lower(ix + iy * self.nx, cut);
                }
            }
        }
    }
}

/// The lowest steel the arc sweep puts above the column at `(qx, qy)`,
/// or `+inf` when the tool never reaches it.
fn column_cut(sw: &ArcSweep, qx: f64, qy: f64, r: f64, atom: Atom) -> f64 {
    let mut cut = f64::INFINITY;
    // The end discs, judged from the move's own end points.
    for (p, tau) in [(sw.from, 0.0), (sw.to, sw.span)] {
        let d = (qx - p.x).hypot(qy - p.y);
        if d <= r {
            cut = cut.min(sw.z(tau) + atom.lift(d, r));
        }
    }
    let (wx, wy) = (qx - sw.ox, qy - sw.oy);
    let rho = wx.hypot(wy);
    let gap = sw.rad - rho;
    if gap.abs() > r {
        return cut; // the whole circle is out of reach
    }
    let rr = 4.0 * sw.rad * rho;
    let s2 = if rr > 0.0 { (r * r - gap * gap) / rr } else { f64::INFINITY };
    // Half-width of the reach window about the column's bearing.
    let h = if s2 >= 1.0 { PI } else { 2.0 * s2.max(0.0).sqrt().asin() };
    // The column's bearing, as travel angle from the arc's start.
    let u0 = if rho > 0.0 { (sw.dir * (wy.atan2(wx) - sw.a0)).rem_euclid(TAU) } else { 0.0 };
    let dist = |tau: f64, cc: f64| {
        let s = ((tau - cc) * 0.5).sin();
        (gap * gap + rr * s * s).max(0.0).sqrt()
    };
    for k in [-1.0, 0.0, 1.0] {
        let cc = u0 + k * TAU;
        let (lo, hi) = ((cc - h).max(0.0), (cc + h).min(sw.span));
        if lo > hi {
            continue;
        }
        let f = |tau: f64| sw.z(tau) + atom.lift(dist(tau, cc).min(r), r);
        cut = cut.min(f(lo)).min(f(hi)).min(f(cc.clamp(lo, hi)));
        let level = sw.to.z == sw.from.z;
        if level || matches!(atom, Atom::Flat) {
            // Disc: Z linear, lift zero - the ends hold the minimum.
            // Level: lift grows with |u| - the clamped bearing does.
            continue;
        }
        let mut cuts = vec![lo, hi];
        if cc > lo && cc < hi {
            cuts.push(cc);
        }
        for b in atom.bends(sw.rad, rho, r).into_iter().flatten() {
            for t in [cc - b, cc + b] {
                if t > lo && t < hi {
                    cuts.push(t);
                }
            }
        }
        cuts.sort_by(f64::total_cmp);
        for w in cuts.windows(2) {
            cut = cut.min(piece_min(&f, w[0], w[1]));
        }
    }
    cut
}

/// The minimum of `f` on `[lo, hi]` for a piece that is convex or
/// concave: its ends, then a golden section bracketed around the best
/// of `SAMPLES` interior samples. Only real evaluations are returned.
pub(super) fn piece_min(f: &impl Fn(f64) -> f64, lo: f64, hi: f64) -> f64 {
    let mut best = f(lo).min(f(hi));
    if hi <= lo || hi.is_nan() || lo.is_nan() {
        return best;
    }
    let step = (hi - lo) / (SAMPLES + 1) as f64;
    let (mut bi, mut bv) = (1usize, f64::INFINITY);
    for i in 1..=SAMPLES {
        let v = f(lo + step * i as f64);
        if v < bv {
            (bi, bv) = (i, v);
        }
    }
    best = best.min(bv);
    let a = lo + step * (bi - 1) as f64;
    let b = (lo + step * (bi + 1) as f64).min(hi);
    best.min(golden(f, a, b))
}
