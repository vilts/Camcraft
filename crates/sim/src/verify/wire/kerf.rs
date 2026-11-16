//! The wire as the kerf law reads it (#534, #536): the program read as
//! a polyline on its plane with each vertex's lean, the wire's section
//! at any judged height, and the kerf's reach toward the product - `k`
//! for a vertical wire, `k * sqrt(1 + (t.u)^2)` across a leaning one
//! (the ellipse a disc of radius `k` normal to the wire cuts from the
//! plane).
//!
//! Provenance: plain 2D distance geometry over the public wire EDM
//! practice the umbrella #290 cites; our own code, no vendor material.

use crate::cl::Toolpath;
use crate::geo2d::Point2;

use super::{dist, Product, WireFinding, KERF_TOL};

/// The wire as the laws read it: each vertex on the programmed plane
/// `z0` and the wire's lean there per unit height (0 when vertical).
pub(crate) struct Wire {
    pub(super) pts: Vec<Point2>,
    pub(super) lean: Vec<[f64; 2]>,
    z0: f64,
}

impl Wire {
    /// The wire's polyline in the plane at height `z`.
    pub(crate) fn at(&self, z: f64) -> Vec<Point2> {
        let dz = z - self.z0;
        self.pts.iter().zip(&self.lean).map(|(p, t)| Point2::new(p.x + t[0] * dz, p.y + t[1] * dz)).collect()
    }

    /// The wire through its first `n` points (#539: a tabbed cut's main
    /// cut, judged before its cut-off); `None` past the path's end.
    pub(super) fn prefix(&self, n: usize) -> Option<Wire> {
        Some(Wire { pts: self.pts.get(..n)?.to_vec(), lean: self.lean.get(..n)?.to_vec(), z0: self.z0 })
    }
}

/// The first move whose kerf enters the product at height `z`: the
/// wire's segment there against `k` widened by its lean toward the
/// product (the ellipse's reach, module head).
pub(super) fn gouge_at(p: &Product<'_>, wire: &Wire, k: f64, z: f64) -> Option<WireFinding> {
    let pts = wire.at(z);
    let mut first: Option<(usize, Point2, f64)> = None;
    let mut moves = 0;
    for (i, w) in pts.windows(2).enumerate() {
        let (d, at) = dist::clearance(p, w[0], w[1]);
        let need = reach(k, w[0], w[1], wire.lean.get(i).copied(), wire.lean.get(i + 1).copied(), at);
        if d < need - KERF_TOL {
            moves += 1;
            if first.is_none() {
                first = Some((i + 1, at, need - d));
            }
        }
    }
    first.map(|(move_no, at, depth)| WireFinding::Gouge { move_no, z, at, depth, moves })
}

/// How far the kerf of the leaning wire on `a`-`b` reaches toward the
/// product point `at`: `k` for a vertical wire, `k * sqrt(1 + (t.u)^2)`
/// for a lean `t` (interpolated at the wire's nearest point) and `u`
/// the direction from that point to `at`.
pub(super) fn reach(k: f64, a: Point2, b: Point2, ta: Option<[f64; 2]>, tb: Option<[f64; 2]>, at: Point2) -> f64 {
    let (ta, tb) = (ta.unwrap_or([0.0; 2]), tb.unwrap_or([0.0; 2]));
    if ta == [0.0; 2] && tb == [0.0; 2] {
        return k;
    }
    let (dx, dy) = (b.x - a.x, b.y - a.y);
    let l2 = dx * dx + dy * dy;
    let s = if l2 > 0.0 { (((at.x - a.x) * dx + (at.y - a.y) * dy) / l2).clamp(0.0, 1.0) } else { 0.0 };
    let w = Point2::new(a.x + s * dx, a.y + s * dy);
    let t = [ta[0] + s * (tb[0] - ta[0]), ta[1] + s * (tb[1] - ta[1])];
    let (ux, uy) = (at.x - w.x, at.y - w.y);
    let ul = ux.hypot(uy);
    // Standing on the product: any reach gouges; the widest is said.
    let tu = if ul > 0.0 { (t[0] * ux + t[1] * uy) / ul } else { t[0].hypot(t[1]) };
    k * tu.mul_add(tu, 1.0).sqrt()
}

/// The path as the laws read it: straight moves on one plane, chained.
/// A pose is read as the wire's lean only when the cut states a taper;
/// a wire leaning past 89 degrees is no wire.
pub(super) fn polyline(path: &Toolpath, taper: bool) -> Result<Wire, String> {
    let Some(m0) = path.moves.first() else {
        return Err("the path has no moves".into());
    };
    let lean_of = |n: usize, axis: Option<[f64; 3]>| -> Result<[f64; 2], String> {
        match axis {
            None => Ok([0.0; 2]),
            Some(_) if !taper => Err(format!(
                "move {n} states a pose, and the cut states no taper - the wire laws read a pose only as a stated taper's lean"
            )),
            Some(a) if a.iter().all(|c| c.is_finite()) && a[2] > 0.0175 => Ok([a[0] / a[2], a[1] / a[2]]),
            Some(_) => Err(format!("move {n} states a pose the wire cannot hold (it leans past 89 degrees or is not a direction)")),
        }
    };
    let mut pts = vec![Point2::new(m0.from.x, m0.from.y)];
    let mut lean = vec![[0.0; 2]];
    for (i, m) in path.moves.iter().enumerate() {
        let n = i + 1;
        if m.arc.is_some() {
            return Err(format!("move {n} is an arc - the wire laws read straight moves only"));
        }
        let finite = [m.from.x, m.from.y, m.from.z, m.to.x, m.to.y, m.to.z].iter().all(|c| c.is_finite());
        if !finite || (m.from.z - m0.from.z).abs() > 1e-9 || (m.to.z - m0.from.z).abs() > 1e-9 {
            return Err(format!("move {n} leaves the programmed plane"));
        }
        let from = Point2::new(m.from.x, m.from.y);
        if pts.last().is_some_and(|l| dist::pp(*l, from) > 1e-6) {
            return Err(format!("move {n} does not start where the move before it ended"));
        }
        pts.push(Point2::new(m.to.x, m.to.y));
        lean.push(lean_of(n, m.axis)?);
    }
    Ok(Wire { pts, lean, z0: m0.from.z })
}
