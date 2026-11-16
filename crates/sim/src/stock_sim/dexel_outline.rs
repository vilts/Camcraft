//! The outline atom (#1152): a sinker EDM electrode BODY - a closed
//! outline extruded along Z - grown by the spark gap and swept along
//! straight chords through the column grid: the electrode as the tool
//! solid (#290 D7), for a section the box atom (#541) cannot state.
//! Everything above the face is the electrode, so a column the grown
//! section covers is lowered to the face's lowest height over the time
//! it spends there.
//!
//! Provenance: the Minkowski sum of a polygon and a disc is the
//! polygon's inside joined with a stadium (a segment grown by the
//! radius) on each edge - de Berg, Cheong, van Kreveld & Overmars,
//! "Computational Geometry: Algorithms and Applications", ch. 13
//! (Minkowski sums). A stadium is convex, so a straight chord meets it
//! in ONE interval, found in closed form: a quadratic for each end
//! disc and two linear windows for the strip between them. The inside
//! test is the crossing-number rule (Shimrat, "Algorithm 112: Position
//! of point relative to polygon", CACM 5(8), 1962). Our own design
//! otherwise. No vendor material.
//!
//! The law per column (the sim's standing rule: a column is judged at
//! its centre): the set of chord parameters `t` at which the centre is
//! within the gap of the section is a union of intervals. The face's
//! height is linear in `t`, so its lowest value over that set is at
//! the set's first or last `t` - and both are exact: the first is 0
//! when the centre starts inside the section, else the least start of
//! an edge's interval (a centre can only come inside across an edge).
//! Nothing is sampled, so the atom never burns less than the electrode
//! does at a centre, and never more.

use super::dexel::Dexel;
use super::kernel::StockError;

use crate::cl::{Move, MoveKind};
use crate::geometry::Point3;

/// The refusal of an arc under the outline atom, said once.
pub const OUTLINE_ARC_REFUSAL: &str =
    "a sinker electrode's outline atom sweeps straight moves only - an arc under an outline electrode is not modelled";

type P = [f64; 2];

/// Is `p` inside the closed outline (crossing number)? A point on the
/// boundary may read either way: every edge's stadium covers it.
fn inside(o: &[P], p: P) -> bool {
    let n = o.len();
    let mut odd = false;
    for i in 0..n {
        let (Some(a), Some(b)) = (o.get(i), o.get((i + 1) % n)) else { continue };
        if (a[1] > p[1]) != (b[1] > p[1]) && p[0] < a[0] + (p[1] - a[1]) * (b[0] - a[0]) / (b[1] - a[1]) {
            odd = !odd;
        }
    }
    odd
}

/// The `t` (all reals) where `lo <= k - t*m <= hi`; `None` when never.
fn band(k: f64, m: f64, lo: f64, hi: f64) -> Option<(f64, f64)> {
    if m.abs() <= f64::EPSILON {
        return (k >= lo && k <= hi).then_some((f64::NEG_INFINITY, f64::INFINITY));
    }
    let (a, b) = ((k - lo) / m, (k - hi) / m);
    Some((a.min(b), a.max(b)))
}

/// The `t` (all reals) where `w - t*d` is within `g` of the point `c`.
fn disc(w: P, d: P, dd: f64, c: P, g: f64) -> Option<(f64, f64)> {
    let r = [w[0] - c[0], w[1] - c[1]];
    let b = r[0] * d[0] + r[1] * d[1];
    let q = b * b - dd * (r[0] * r[0] + r[1] * r[1] - g * g);
    if q.is_nan() || q < 0.0 {
        return None;
    }
    let s = q.sqrt();
    Some(((b - s) / dd, (b + s) / dd))
}

/// The window of `t` in `[0, 1]` where the point `w - t*d` is within
/// `g` of the segment `a`-`b` (its stadium), or `None` when never. The
/// stadium is convex, so the window is the hull of its three parts.
fn edge_window(w: P, d: P, a: P, b: P, g: f64) -> Option<(f64, f64)> {
    let e = [b[0] - a[0], b[1] - a[1]];
    let len = e[0].hypot(e[1]);
    let dd = d[0] * d[0] + d[1] * d[1];
    if dd <= f64::EPSILON * f64::EPSILON {
        // The centre does not move against the section: one distance.
        let r = [w[0] - a[0], w[1] - a[1]];
        let t = if len > 0.0 { ((r[0] * e[0] + r[1] * e[1]) / (len * len)).clamp(0.0, 1.0) } else { 0.0 };
        let off = (r[0] - t * e[0]).hypot(r[1] - t * e[1]);
        return (off <= g).then_some((0.0, 1.0));
    }
    let mut hull: Option<(f64, f64)> = None;
    let mut join = |part: Option<(f64, f64)>| {
        if let Some((lo, hi)) = part {
            hull = Some(hull.map_or((lo, hi), |(l, h)| (l.min(lo), h.max(hi))));
        }
    };
    join(disc(w, d, dd, a, g));
    join(disc(w, d, dd, b, g));
    if len > 0.0 {
        let (u, r) = ([e[0] / len, e[1] / len], [w[0] - a[0], w[1] - a[1]]);
        // Along the edge within its length, across it within the gap.
        let along = band(r[0] * u[0] + r[1] * u[1], d[0] * u[0] + d[1] * u[1], 0.0, len);
        let across = band(u[0] * r[1] - u[1] * r[0], u[0] * d[1] - u[1] * d[0], -g, g);
        if let (Some((a0, a1)), Some((c0, c1))) = (along, across) {
            let (lo, hi) = (a0.max(c0), a1.min(c1));
            join((lo <= hi).then_some((lo, hi)));
        }
    }
    let (lo, hi) = hull?;
    let (lo, hi) = (lo.max(0.0), hi.min(1.0));
    (lo <= hi).then_some((lo, hi))
}

impl Dexel {
    /// Lower every column the section `outline` (mm about the burn
    /// point), grown by `g`, covers while the burn point rides the
    /// chord `a -> b`, to the face's lowest height over that time.
    pub fn cut_outline(&mut self, a: Point3, b: Point3, outline: &[P], g: f64) {
        let Some((lo, hi)) = camcraft_tools::electrode_outline::bounds(outline) else { return };
        if !(a.is_finite() && b.is_finite() && g.is_finite() && g >= 0.0 && lo.iter().chain(hi.iter()).all(|v| v.is_finite())) {
            return; // the gate in `simulate_outline` refuses before this
        }
        let d = [b.x - a.x, b.y - a.y];
        let fx = |v: f64| ((v - self.x0) / self.cell).floor();
        let fy = |v: f64| ((v - self.y0) / self.cell).floor();
        let lo_x = fx(a.x.min(b.x) + lo[0] - g).max(0.0) as usize;
        let hi_x = fx(a.x.max(b.x) + hi[0] + g).clamp(0.0, self.nx as f64 - 1.0) as usize;
        let lo_y = fy(a.y.min(b.y) + lo[1] - g).max(0.0) as usize;
        let hi_y = fy(a.y.max(b.y) + hi[1] + g).clamp(0.0, self.ny as f64 - 1.0) as usize;
        if lo_x >= self.nx || lo_y >= self.ny {
            return;
        }
        let n = outline.len();
        for iy in lo_y..=hi_y {
            for ix in lo_x..=hi_x {
                let q = self.center(ix, iy);
                // The centre in the electrode's frame: it starts at `w`
                // and ends at `w - d`.
                let w = [q.x - a.x, q.y - a.y];
                let mut first = if inside(outline, w) { 0.0 } else { f64::INFINITY };
                let mut last = if inside(outline, [w[0] - d[0], w[1] - d[1]]) { 1.0 } else { f64::NEG_INFINITY };
                for i in 0..n {
                    let (Some(&p0), Some(&p1)) = (outline.get(i), outline.get((i + 1) % n)) else { continue };
                    if let Some((t0, t1)) = edge_window(w, d, p0, p1, g) {
                        first = first.min(t0);
                        last = last.max(t1);
                    }
                }
                if first > last {
                    continue;
                }
                // The chord's own end heights are read, not recomputed.
                let z = |t: f64| if t >= 1.0 { b.z } else { a.z + (b.z - a.z) * t };
                self.lower(ix + iy * self.nx, z(first).min(z(last)));
            }
        }
    }
}

/// Run one body electrode's moves through the grid: the section
/// `outline` (mm about the burn point, which rides the moves), burning
/// a spark `gap` beyond it on every side and below its face - the
/// cavity a sinker leaves is the electrode grown by its condition's gap
/// (#531). Rapids touch nothing (RS-274 G0, as for every atom); an
/// outline that is no section, a non-finite move, a posed cutting move
/// or an arc is refused by name BEFORE any column is lowered, so a
/// refusal never leaves a half-run grid.
pub fn simulate_outline(dex: &mut Dexel, moves: &[Move], outline: &[P], gap: f64) -> Result<(), StockError> {
    if let Some(why) = camcraft_tools::electrode_outline::shape_refusal(outline) {
        return Err(StockError::Invalid(why));
    }
    if !(gap.is_finite() && gap >= 0.0) {
        return Err(StockError::Invalid(format!("electrode spark gap {gap} is not finite and non-negative")));
    }
    if !moves.iter().all(|m| m.is_finite()) {
        return Err(StockError::Invalid(
            "an electrode move is non-finite - the sim refuses instead of cutting nothing silently".into(),
        ));
    }
    if let Some(k) = super::cut::first_posed(moves) {
        return Err(StockError::Invalid(format!("electrode move {k}: {}", super::cut::POSED_REFUSAL)));
    }
    if let Some(k) = moves.iter().position(|m| m.kind != MoveKind::Rapid && m.arc.is_some()) {
        return Err(StockError::Invalid(format!("electrode move {k}: {OUTLINE_ARC_REFUSAL}")));
    }
    let down = |p: Point3| Point3::new(p.x, p.y, p.z - gap);
    for m in moves.iter().filter(|m| m.kind != MoveKind::Rapid) {
        dex.cut_outline(down(m.from), down(m.to), outline, gap);
    }
    Ok(())
}
