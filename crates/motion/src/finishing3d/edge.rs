//! Edge contact on convex folds (#36): the ball-against-segment arm of the
//! drop-cutter vertex/facet/edge contact decomposition, as published for
//! triangulated surfaces (Yau, Chuang & Lee, "Numerical control machining of
//! triangulated sculptured surfaces in a stereo lithography format with a
//! generalized cutter", Int. J. Prod. Res. 42(13), 2004; Hwang & Chang,
//! "Three-axis machining of compound surfaces using flat and filleted
//! endmills", CAD 30(8), 1998). Written from that geometry, not from any
//! vendor's code or output.
//!
//! Between two tessellated facets whose sky normals `m1`, `m2` differ, the
//! strict facet tangency leaves an annulus `r·|m1 − m2|` wide where no ball
//! centre is tangent to either facet: the ball rests on the shared EDGE
//! there. On a convex fold that edge contact is the true drop-cutter
//! answer: the ball touches the solid at the edge only, every other point
//! at least `r` away, so the march may ride it. A concave fold's edge-resting centre
//! sits inside the neighbour's ball (closer than `r` to its plane), and the
//! exact clearance law refuses it: such edges are not even flagged.

use std::collections::HashMap;

use crate::geometry::Point3;

use super::params::TOUCH_EPS;
use super::grid::SupSet;
use super::support::{ball_clear, Sup};

/// Mark, on ONE of its two facets, every edge shared by two shallow facets
/// that folds convexly (the neighbour's far vertex falls below the
/// flagged facet's plane). Boundary edges and non-manifold extras stay
/// unflagged: a ball rolling off an open sheet's rim is no fold contact.
/// Vertices match by exact bits (a tessellator's shared corner is one
/// point); a mesh whose seams disagree in the last bit simply keeps the
/// strict tangency there - the old law, never a looser one.
pub(super) fn flag_folds(sups: &mut [Sup]) {
    let key = |p: Point3| {
        let b = |v: f64| (v + 0.0).to_bits(); // -0.0 and 0.0 are one corner
        (b(p.x), b(p.y), b(p.z))
    };
    type K = (u64, u64, u64);
    let mut open: HashMap<(K, K), (usize, usize)> = HashMap::new();
    for i in 0..sups.len() {
        if !sups[i].shallow {
            continue;
        }
        for e in 0..3 {
            let (ka, kb) = (key(sups[i].tri[e]), key(sups[i].tri[(e + 1) % 3]));
            if ka == kb {
                continue;
            }
            let k = if ka < kb { (ka, kb) } else { (kb, ka) };
            match open.get(&k) {
                None => {
                    open.insert(k, (i, e));
                }
                Some(&(j, ej)) if j != i => {
                    if convex(&sups[j], &sups[i], ej) {
                        sups[j].folds[ej] = Some(sups[i].m);
                    }
                    open.remove(&k); // a third facet on the edge is non-manifold: not a fold
                }
                Some(_) => {}
            }
        }
    }
}

/// Is the fold across `a`'s edge `ea` onto `b` convex? `b`'s vertex off the
/// shared edge must lie strictly below `a`'s plane (both normals point to
/// the sky); coplanar seams have no gap to fill.
fn convex(a: &Sup, b: &Sup, ea: usize) -> bool {
    let (p, q) = (a.tri[ea], a.tri[(ea + 1) % 3]);
    let Some(far) = b.tri.iter().copied().find(|v| !same(*v, p) && !same(*v, q)) else {
        return false;
    };
    let o = a.tri[0];
    let h = a.m.x * (far.x - o.x) + a.m.y * (far.y - o.y) + a.m.z * (far.z - o.z);
    let span = (far.x - p.x).hypot(far.y - p.y).hypot(far.z - p.z);
    h.is_finite() && span.is_finite() && h < -1e-9 * span.max(1.0)
}

fn same(u: Point3, v: Point3) -> bool {
    u.x == v.x && u.y == v.y && u.z == v.z
}

/// The drop of a ball of radius `r` along the column at XY `(x, y)` onto
/// the segment `p0 p1`: the highest centre at distance exactly `r` from
/// the segment, with the contact strictly inside it (its ends are vertex
/// contacts, not this arm's). Returns the centre height and its gradient
/// along the XY unit ray `(ux, uy)`. In the vertical plane through the
/// edge the ball's section is a circle of radius `s = √(r² − d²)` (`d` the
/// XY offset from the edge's line), tangent from above to the line
/// `z = z0 + k·u`: centre `z0 + k·u_c + s·√(1 + k²)`, contact at
/// `u_c + s·k/√(1 + k²)`. Closed form, no iteration. The third value is
/// the contact normal `(centre − contact)/r`.
pub(super) fn edge_drop(
    p0: Point3,
    p1: Point3,
    x: f64,
    y: f64,
    r: f64,
    ray: (f64, f64),
) -> Option<(f64, f64, Point3)> {
    let (ex, ey, ez) = (p1.x - p0.x, p1.y - p0.y, p1.z - p0.z);
    let len = ex.hypot(ey);
    if !(len.is_finite() && len > 1e-12) {
        return None;
    }
    let (ux, uy, k) = (ex / len, ey / len, ez / len);
    let (dx, dy) = (x - p0.x, y - p0.y);
    let uc = dx * ux + dy * uy;
    let d = -dx * uy + dy * ux;
    let s2 = r * r - d * d;
    if s2.is_nan() || s2 <= 0.0 {
        return None;
    }
    let s = s2.sqrt();
    let g = (1.0 + k * k).sqrt();
    let ut = uc + s * k / g;
    if !(ut > 0.0 && ut < len) {
        return None;
    }
    let cz = p0.z + k * uc + s * g;
    let (vx, vy) = ray;
    let grad = k * (vx * ux + vy * uy) - g * d * (-vx * uy + vy * ux) / s;
    let t = ut / len;
    let n = Point3::new(
        (x - (p0.x + ex * t)) / r,
        (y - (p0.y + ey * t)) / r,
        (cz - (p0.z + ez * t)) / r,
    );
    (cz.is_finite() && grad.is_finite() && n.is_finite()).then_some((cz, grad, n))
}

fn cross(a: Point3, b: Point3) -> Point3 {
    Point3::new(a.y * b.z - a.z * b.y, a.z * b.x - a.x * b.z, a.x * b.y - a.y * b.x)
}

fn dot(a: Point3, b: Point3) -> f64 {
    a.x * b.x + a.y * b.y + a.z * b.z
}

/// Does the contact normal `n` lie on the arc from `m1` to `m2` (all three
/// perpendicular to the edge)? That arc IS the fold's edge-contact wedge
/// for two up-facing facets. A knife rim whose underside only LOOKS
/// shallow once its normal is flipped to the sky (a cap's base disc) has
/// its real wedge outside that arc, so it is not ridden here - the ball
/// would roll over the rim into the steep side's jurisdiction.
fn on_arc(m1: Point3, m2: Point3, n: Point3) -> bool {
    let c = cross(m1, m2);
    dot(cross(m1, n), c) >= -1e-12 && dot(cross(n, m2), c) >= -1e-12 && dot(n, m1) + dot(n, m2) > 0.0
}

/// Every flagged fold of `s` dropped onto at `(x, y)`: the candidate
/// centres (with their gradients along `ray`) the clearance law then
/// judges exactly like a facet tangent.
fn fold_drops(s: &Sup, x: f64, y: f64, r: f64, ray: (f64, f64)) -> impl Iterator<Item = (f64, f64)> + '_ {
    (0..3).filter_map(move |e| {
        let m2 = s.folds[e]?;
        let (cz, g, n) = edge_drop(s.tri[e], s.tri[(e + 1) % 3], x, y, r, ray)?;
        on_arc(s.m, m2, n).then_some((cz, g))
    })
}

/// The highest flagged-fold edge contact above `(x, y)` that beats
/// `above` (the best facet tangent so far), keeps the floor, and passes
/// the exact clearance `ball_clear` - the facet candidates' own law.
pub(super) fn fold_best(
    sups: &SupSet,
    x: f64,
    y: f64,
    r: f64,
    floor_tip: f64,
    ray: (f64, f64),
    above: Option<f64>,
) -> Option<(f64, f64)> {
    let mut best: Option<(f64, f64)> = None;
    for s in sups.near(x, y) {
        if !s.shallow || x < s.xmin || x > s.xmax || y < s.ymin || y > s.ymax {
            continue;
        }
        for (cz, g) in fold_drops(s, x, y, r, ray) {
            let floor = best.map(|b| b.0).or(above);
            if cz - r < floor_tip - TOUCH_EPS || floor.is_some_and(|b| b >= cz) {
                continue;
            }
            if ball_clear(Point3::new(x, y, cz), sups, r) {
                best = Some((cz, g));
            }
        }
    }
    best
}
