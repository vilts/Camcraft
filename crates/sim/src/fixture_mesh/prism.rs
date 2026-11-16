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

/// The Z range of facet `t` clipped to the closed rectangle, or `None`
/// when it misses it.
fn clipped_z(t: &[V; 3], x0: f64, x1: f64, y0: f64, y1: f64) -> Option<(f64, f64)> {
    // Up to 3 + 4 vertices after four half-plane clips.
    let mut poly: Vec<V> = t.to_vec();
    let planes: [(usize, f64, f64); 4] = [(0, x0, 1.0), (0, x1, -1.0), (1, y0, 1.0), (1, y1, -1.0)];
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
    let lo = poly.iter().map(|p| p[2]).fold(f64::INFINITY, f64::min);
    let hi = poly.iter().map(|p| p[2]).fold(f64::NEG_INFINITY, f64::max);
    // A clip that is not finite (a NaN through a degenerate edge) meets
    // the rectangle somewhere unknown: it spans every Z, so the slab
    // reads as steel - never as a miss that could clear it.
    if !(lo.is_finite() && hi.is_finite() && poly.iter().all(|p| p.iter().all(|v| v.is_finite()))) {
        return Some((f64::NEG_INFINITY, f64::INFINITY));
    }
    Some((lo, hi))
}

impl FixtureMesh {
    /// The steel inside `x.0..x.1 × y.0..y.1 × z.0..z.1`, with `tol` of
    /// contact allowed at the slab's floor and top (see the module
    /// note). `budget` is shared with [`Self::steel_on_line`]: each facet
    /// visited takes one; when it runs out the box is
    /// [`PrismReading::Unread`].
    #[must_use]
    pub fn steel_in_prism(&self, x: (f64, f64), y: (f64, f64), z: (f64, f64), tol: f64, budget: &mut u64) -> PrismReading {
        let all = [x.0, x.1, y.0, y.1, z.0, z.1, tol];
        if !all.iter().all(|v| v.is_finite()) || x.0 > x.1 || y.0 > y.1 || tol < 0.0 {
            return PrismReading::Unread;
        }
        let (a, b) = (z.0 + tol, z.1 - tol);
        if a >= b {
            return PrismReading::Clear; // material no thicker than the contact band
        }
        let pad = self.pad;
        let (mut zmin, mut zmax) = (f64::INFINITY, f64::NEG_INFINITY);
        let (mut any, mut hit, mut unread) = (false, false, false);
        self.bvh.walk(
            |n| n.lo[0] - pad <= x.1 && x.0 <= n.hi[0] + pad && n.lo[1] - pad <= y.1 && y.0 <= n.hi[1] + pad,
            |t| {
                let Some(left) = budget.checked_sub(1) else {
                    unread = true;
                    return true;
                };
                *budget = left;
                // A facet index the mesh does not hold is unread, never
                // skipped: a skipped facet could clear the box.
                let Some(tri) = self.tris.get(t) else {
                    unread = true;
                    return true;
                };
                let Some((lo, hi)) = clipped_z(tri, x.0, x.1, y.0, y.1) else { return false };
                any = true;
                zmin = zmin.min(lo);
                zmax = zmax.max(hi);
                if hi > a && lo < b {
                    hit = true;
                }
                false
            },
        );
        if unread {
            return PrismReading::Unread;
        }
        if !any {
            return PrismReading::Clear;
        }
        let depth = zmax.min(z.1) - zmin.max(z.0);
        if !depth.is_finite() {
            return PrismReading::Unread;
        }
        if hit {
            return PrismReading::Steel(depth);
        }
        // No surface in the slab: it is all steel or all air - one line
        // through its middle decides.
        let (cx, cy, cz) = ((x.0 + x.1) * 0.5, (y.0 + y.1) * 0.5, (a + b) * 0.5);
        match self.steel_on_line(cx, cy, budget) {
            None => PrismReading::Unread,
            Some(spans) if spans.iter().any(|&(s0, s1)| s0 <= cz && cz <= s1) => PrismReading::Steel(depth),
            Some(_) => PrismReading::Clear,
        }
    }
}
