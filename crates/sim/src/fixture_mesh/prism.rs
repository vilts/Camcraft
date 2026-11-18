//! The steel a closed fixture mesh puts inside one axis-aligned box
//! `x0..x1 × y0..y1 × lo..hi` (#1530 review): the carried-workholding
//! gate's reading of the material BETWEEN the carried grid's columns,
//! where a line per column cannot see a post or a lip narrower than the
//! pitch.
//!
//! Provenance: polygon clipping against axis-aligned half-planes
//! (Sutherland and Hodgman, "Reentrant Polygon Clipping", CACM 17(1),
//! 1974); the boundary of a bounded solid separating its inside from its
//! outside (Jordan-Brouwer, as used for point-in-solid in O'Rourke,
//! "Computational Geometry in C", 2nd ed., ch. 7). Our own code.
//!
//! ## Why the answer is conservative (the whole box, not sampled lines)
//!
//! Let `S` be the solid, `P` the vertical prism over the rectangle, and
//! `B = P ∩ {lo + tol < z < hi - tol}` the slab the caller calls
//! material (its contact band trimmed).
//! 1. If the surface meets `B` - some facet clipped to the rectangle has
//!    a Z range reaching into `(lo + tol, hi - tol)`; a clipped facet is
//!    connected and Z is continuous on it, so a point of it lies in `B`
//!    - steel stands in the material: [`PrismReading::Steel`].
//! 2. If no facet meets the rectangle at all, `S ∩ P` is empty: every
//!    point of `S` has its vertical line leave the bounded `S` through
//!    the surface at the same XY.
//! 3. Otherwise `B` is connected and holds no surface point, so it lies
//!    wholly inside `S` or wholly outside it: one line through `B`
//!    decides ([`FixtureMesh::steel_on_line`]; an unreadable or
//!    inconsistent line is [`PrismReading::Unread`]).
//!
//! No step samples, so no feature of any size inside the box is missed.
//! The depth reported is the overlap of the facets' Z envelope over the
//! rectangle with `lo..hi` - an upper bound on the steel's depth there.

use super::FixtureMesh;
use super::bvh::tri_box;
use super::column::line_along;
use super::gjk::V;

/// What a box holds of a fixture's steel.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PrismReading {
    /// No steel in the slab beyond the contact band.
    Clear,
    /// Steel in the slab; the depth (mm) it can reach into `lo..hi`.
    Steel(f64),
    /// Not readable (budget spent, input not finite, inconsistent
    /// winding): the caller keeps its box verdict.
    Unread,
}

/// Detailed box-vs-mesh classification used by sub-pitch boundary search.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum BoxSteel {
    Clear,
    /// No mesh facet enters `(z0 + tol, z1 - tol)`, and the box lies inside steel.
    WhollyInside(f64),
    /// At least one mesh facet crosses `(z0 + tol, z1 - tol)` inside the box.
    Boundary(f64),
    Unread,
}

impl BoxSteel {
    #[must_use]
    pub fn into_reading(self) -> PrismReading {
        match self {
            Self::Clear => PrismReading::Clear,
            Self::WhollyInside(d) | Self::Boundary(d) => PrismReading::Steel(d),
            Self::Unread => PrismReading::Unread,
        }
    }
}

/// The `[u, v, w]` spans of facet `t` clipped to the closed rectangle in `(u_ax, v_ax)`,
/// or `None` when it misses it.
fn clipped_range(t: &[V; 3], axes: (usize, usize, usize), u: (f64, f64), v: (f64, f64)) -> Option<[(f64, f64); 3]> {
    let (u_ax, v_ax, w_ax) = axes;
    let mut poly: Vec<V> = t.to_vec();
    let planes: [(usize, f64, f64); 4] = [(u_ax, u.0, 1.0), (u_ax, u.1, -1.0), (v_ax, v.0, 1.0), (v_ax, v.1, -1.0)];
    for (axis, at, sign) in planes {
        let mut next: Vec<V> = Vec::with_capacity(poly.len() + 1);
        let n = poly.len();
        for i in 0..n {
            let (Some(&p), Some(&q)) = (poly.get(i), poly.get((i + 1) % n)) else { continue };
            let (dp, dq) = (sign * (p[axis] - at), sign * (q[axis] - at));
            if dp >= 0.0 {
                next.push(p);
            }
            if (dp >= 0.0) != (dq >= 0.0) {
                let s = dp / (dp - dq);
                next.push([p[0] + s * (q[0] - p[0]), p[1] + s * (q[1] - p[1]), p[2] + s * (q[2] - p[2])]);
            }
        }
        if next.is_empty() {
            return None;
        }
        poly = next;
    }
    let span = |ax: usize| {
        (
            poly.iter().map(|p| p[ax]).fold(f64::INFINITY, f64::min),
            poly.iter().map(|p| p[ax]).fold(f64::NEG_INFINITY, f64::max),
        )
    };
    let (su, sv, sw) = (span(u_ax), span(v_ax), span(w_ax));
    // A clip that is not finite (a NaN through a degenerate edge) meets
    // the rectangle somewhere unknown: it spans every W, so the slab
    // reads as steel - never as a miss that could clear it.
    if !(sw.0.is_finite() && sw.1.is_finite() && poly.iter().all(|p| p.iter().all(|v| v.is_finite()))) {
        return Some([(u.0, u.1), (v.0, v.1), (f64::NEG_INFINITY, f64::INFINITY)]);
    }
    Some([su, sv, sw])
}

impl FixtureMesh {
    /// Collect triangle indices whose XY bounding boxes overlap `x × y`
    /// padded by `extra_pad`. Returns `None` if `budget` expires and
    /// `Some(None)` if the count exceeds `cap`.
    #[must_use]
    pub fn collect_tris_in_xy(
        &self,
        x: (f64, f64),
        y: (f64, f64),
        extra_pad: f64,
        cap: usize,
        budget: &mut u64,
    ) -> Option<Option<Vec<usize>>> {
        if !(x.0.is_finite() && x.1.is_finite() && y.0.is_finite() && y.1.is_finite() && extra_pad.is_finite()) {
            return None;
        }
        let pad = self.pad + extra_pad.max(0.0);
        let mut out = Vec::new();
        let mut unread = false;
        let mut over_cap = false;
        self.bvh.walk(
            |n| n.lo[0] - pad <= x.1 && x.0 <= n.hi[0] + pad && n.lo[1] - pad <= y.1 && y.0 <= n.hi[1] + pad,
            |t| {
                let Some(left) = budget.checked_sub(1) else {
                    unread = true;
                    return true;
                };
                *budget = left;
                let Some(tri) = self.tris.get(t) else {
                    unread = true;
                    return true;
                };
                let (tlo, thi) = tri_box(tri);
                if tlo[0] - pad <= x.1 && x.0 <= thi[0] + pad && tlo[1] - pad <= y.1 && y.0 <= thi[1] + pad {
                    out.push(t);
                    if out.len() > cap {
                        over_cap = true;
                        return true;
                    }
                }
                false
            },
        );
        if unread {
            None
        } else if over_cap {
            Some(None)
        } else {
            Some(Some(out))
        }
    }

    /// Classify steel inside an axis-aligned box `bounds` with contact
    /// tolerance `tol` along `tol_axis` (0=X, 1=Y, 2=Z). If `tris` is `Some`,
    /// `tol_axis` must be `2` and `tris` must cover the XY region.
    #[must_use]
    pub fn steel_in_axis_box(
        &self,
        tris: Option<&[usize]>,
        bounds: [(f64, f64); 3],
        tol_axis: usize,
        tol: f64,
        budget: &mut u64,
    ) -> BoxSteel {
        self.steel_in_axis_box_uv(tris, bounds, tol_axis, tol, budget).0
    }

    /// Like [`Self::steel_in_axis_box`], also returning the `(u_ax, v_ax)`
    /// bounding span of facets hitting `(w0 + tol, w1 - tol)` when `Boundary`.
    #[must_use]
    pub fn steel_in_axis_box_uv(
        &self,
        tris: Option<&[usize]>,
        bounds: [(f64, f64); 3],
        tol_axis: usize,
        tol: f64,
        budget: &mut u64,
    ) -> (BoxSteel, [(f64, f64); 2]) {
        let full_uv = [bounds.get((tol_axis + 1) % 3).copied().unwrap_or((0.0, 0.0)), bounds.get((tol_axis + 2) % 3).copied().unwrap_or((0.0, 0.0))];
        if tol_axis > 2
            || !bounds.iter().all(|&(a, b)| a.is_finite() && b.is_finite() && a <= b)
            || !(tol.is_finite() && tol >= 0.0)
        {
            return (BoxSteel::Unread, full_uv);
        }
        let (u_ax, v_ax, w_ax) = ((tol_axis + 1) % 3, (tol_axis + 2) % 3, tol_axis);
        let (u, v, w) = (bounds[u_ax], bounds[v_ax], bounds[w_ax]);
        let (a, b) = (w.0 + tol, w.1 - tol);
        if a >= b {
            return (BoxSteel::Clear, [u, v]);
        }
        let pad = self.pad;
        let (mut wmin, mut wmax) = (f64::INFINITY, f64::NEG_INFINITY);
        let (mut hu0, mut hu1, mut hv0, mut hv1) = (f64::INFINITY, f64::NEG_INFINITY, f64::INFINITY, f64::NEG_INFINITY);
        let (mut any, mut hit, mut unread) = (false, false, false);
        let mut check_tri = |t: usize| -> bool {
            let Some(left) = budget.checked_sub(1) else {
                unread = true;
                return true;
            };
            *budget = left;
            let Some(tri) = self.tris.get(t) else {
                unread = true;
                return true;
            };
            let Some([su, sv, (lo, hi)]) = clipped_range(tri, (u_ax, v_ax, w_ax), u, v) else {
                return false;
            };
            any = true;
            wmin = wmin.min(lo);
            wmax = wmax.max(hi);
            if hi > a && lo < b {
                hit = true;
                hu0 = hu0.min(su.0);
                hu1 = hu1.max(su.1);
                hv0 = hv0.min(sv.0);
                hv1 = hv1.max(sv.1);
            }
            false
        };
        match tris {
            Some(list) => {
                for &t in list {
                    if check_tri(t) {
                        break;
                    }
                }
            }
            None => {
                self.bvh.walk(
                    |n| {
                        n.lo[u_ax] - pad <= u.1
                            && u.0 <= n.hi[u_ax] + pad
                            && n.lo[v_ax] - pad <= v.1
                            && v.0 <= n.hi[v_ax] + pad
                    },
                    check_tri,
                );
            }
        }
        if unread {
            return (BoxSteel::Unread, [u, v]);
        }
        if !any {
            return (BoxSteel::Clear, [u, v]);
        }
        let depth = wmax.min(w.1) - wmin.max(w.0);
        if !depth.is_finite() {
            return (BoxSteel::Unread, [u, v]);
        }
        if hit {
            return (BoxSteel::Boundary(depth), [(hu0.max(u.0), hu1.min(u.1)), (hv0.max(v.0), hv1.min(v.1))]);
        }
        let (cu, cv, cw) = ((u.0 + u.1) * 0.5, (v.0 + v.1) * 0.5, (a + b) * 0.5);
        let kind = match line_along(self, tris, (u_ax, v_ax, w_ax), cu, cv, budget) {
            None => BoxSteel::Unread,
            Some(spans) if spans.iter().any(|&(s0, s1)| s0 <= cw && cw <= s1) => BoxSteel::WhollyInside(depth),
            Some(_) => BoxSteel::Clear,
        };
        (kind, [u, v])
    }

    /// The steel inside `x.0..x.1 × y.0..y.1 × z.0..z.1`, with `tol` of
    /// contact allowed at the slab's floor and top (see the module
    /// note). `budget` is shared with [`Self::steel_on_line`]: each facet
    /// visited takes one; when it runs out the box is
    /// [`PrismReading::Unread`].
    #[must_use]
    pub fn steel_in_prism(&self, x: (f64, f64), y: (f64, f64), z: (f64, f64), tol: f64, budget: &mut u64) -> PrismReading {
        self.steel_in_axis_box(None, [x, y, z], 2, tol, budget).into_reading()
    }
}
