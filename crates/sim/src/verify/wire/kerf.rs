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

use super::{dist, Product, WireFinding, WireUnder, KERF_TOL};

/// The wire as the laws read it: each vertex on the programmed plane
/// `z0` and the wire's lean there per unit height (0 when vertical).
pub(crate) struct Wire {
    pub(super) pts: Vec<Point2>,
    pub(super) lean: Vec<[f64; 2]>,
    z0: f64,
    /// Per segment `i` (`pts[i]` to `pts[i + 1]`): the move it belongs to
    /// (1-based) and how far the recorded curve may stand off it - 0 for
    /// a straight move, the chord's sagitta for a piece of an arc (#3937).
    pub(super) seg: Vec<(usize, f64)>,
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
        if self.has_arcs() {
            return None;
        }
        let seg = self.seg.get(..n.saturating_sub(1))?.to_vec();
        Some(Wire { pts: self.pts.get(..n)?.to_vec(), lean: self.lean.get(..n)?.to_vec(), z0: self.z0, seg })
    }

    /// Whether any move is an arc, read as chords.
    pub(super) fn has_arcs(&self) -> bool {
        self.seg.iter().any(|s| s.1 > 0.0) || self.seg.iter().enumerate().any(|(i, s)| s.0 != i + 1)
    }

    /// Segment `i`'s move (1-based) and the most its curve stands off it.
    pub(super) fn seg(&self, i: usize) -> (usize, f64) {
        self.seg.get(i).copied().unwrap_or((i + 1, 0.0))
    }
}

/// One height's kerf readings under the stated verify tolerance `tol`
/// (#1170): the moves that gouge, and the deepest reading the
/// tolerance let pass.
pub(super) struct Reading {
    tol: f64,
    first: Option<(usize, Point2, f64)>,
    moves: usize,
    under: Option<(f64, Point2)>,
    deepest: f64,
}

impl Reading {
    pub(super) fn new(tol: f64) -> Self {
        Reading { tol, first: None, moves: 0, under: None, deepest: 0.0 }
    }

    /// Move `move_no` (1-based): its kerf needs `need` and the product
    /// stands `d` from it, nearest at `at`.
    pub(super) fn take(&mut self, move_no: usize, d: f64, need: f64, at: Point2) {
        if d < need - KERF_TOL - self.tol {
            self.moves += 1;
            self.deepest = self.deepest.max(need - d);
            self.first.get_or_insert((move_no, at, need - d));
        } else if d < need - KERF_TOL && self.under.is_none_or(|(deep, _)| need - d > deep) {
            self.under = Some((need - d, at));
        }
    }

    /// The first gouge at height `z` with the count of moves that
    /// gouge, and the deepest reading passed under the tolerance.
    pub(super) fn close(self, z: f64) -> (Option<WireFinding>, Option<WireUnder>) {
        let (moves, tol, deepest) = (self.moves, self.tol, self.deepest);
        (
            self.first.map(|(move_no, at, depth)| WireFinding::Gouge { move_no, z, at, depth, moves, tol, deepest, chord: false }),
            self.under.map(|(depth, at)| WireUnder { depth, z, at }),
        )
    }
}

/// The first move whose kerf enters the product at height `z` by more
/// than the verify tolerance `tol`: the wire's segment there against
/// `k` widened by its lean toward the product (the ellipse's reach,
/// module head).
pub(super) fn gouge_at(p: &Product<'_>, wire: &Wire, k: f64, z: f64, tol: f64) -> (Option<WireFinding>, Option<WireUnder>) {
    let pts = wire.at(z);
    let mut read = Reading::new(tol);
    for (i, w) in pts.windows(2).enumerate() {
        let (d, at) = dist::entry(p, w[0], w[1]);
        let (n, sag) = wire.seg(i);
        read.take(n, d - sag, reach(k, w[0], w[1], wire.lean.get(i).copied(), wire.lean.get(i + 1).copied(), at), at);
    }
    read.close(z)
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

/// The path as the laws read it: moves on one plane, chained - straight
/// moves as they stand, the arcs of a vertical cut as chords (`arcs`,
/// #3937). A pose is read as the wire's lean only when the cut states a
/// taper; a wire leaning past 89 degrees is no wire; an arc under a
/// stated taper is a cone, which the laws do not read.
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
    let mut seg = Vec::with_capacity(path.moves.len());
    for (i, m) in path.moves.iter().enumerate() {
        let n = i + 1;
        if m.arc.is_some() && (taper || m.axis.is_some()) {
            return Err(format!("move {n} is an arc on a leaning wire - the wire laws read a cone's arc as no move; cut the taper with straight moves"));
        }
        let finite = [m.from.x, m.from.y, m.from.z, m.to.x, m.to.y, m.to.z].iter().all(|c| c.is_finite());
        if !finite || (m.from.z - m0.from.z).abs() > 1e-9 || (m.to.z - m0.from.z).abs() > 1e-9 {
            return Err(format!("move {n} leaves the programmed plane"));
        }
        let from = Point2::new(m.from.x, m.from.y);
        if pts.last().is_some_and(|l| dist::pp(*l, from) > 1e-6) {
            return Err(format!("move {n} does not start where the move before it ended"));
        }
        if m.arc.is_some() {
            let cs = super::arcs::chords(m).ok_or_else(|| format!("move {n} is an arc the wire laws cannot read (not a flat XY circle)"))?;
            for (q, sag) in cs {
                pts.push(q);
                lean.push([0.0; 2]);
                seg.push((n, sag));
            }
            continue;
        }
        pts.push(Point2::new(m.to.x, m.to.y));
        lean.push(lean_of(n, m.axis)?);
        seg.push((n, 0.0));
    }
    Ok(Wire { pts, lean, z0: m0.from.z, seg })
}
