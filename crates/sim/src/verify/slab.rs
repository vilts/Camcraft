//! The prismatic Z slabs of a mesh (#3831): the Z ranges between two neighbouring vertex levels where every facet that crosses is vertical, so the plane section is one point set at every height - the extrusion (prism) property of solid geometry textbooks, on our own mesh model; no vendor source.
//!
//! The verifier sections the model at each move's plane. A ramp ends
//! every move at a Z of its own, so a ramped path asked for one section
//! per move - each a walk over every facet - although a plate with
//! through holes has ONE section from its bottom to its top. [`Slabs`]
//! says when two planes must give the same section, so the rest model
//! ([`super::rest::Rest`]) sections such a slab once.
//!
//! The claim is exact, not a tolerance:
//! - no vertex stands strictly inside a slab, so the facets a plane
//!   crosses are the same at every height of it;
//! - a slab is prismatic only when each of those facets has two vertices
//!   on one vertical line (bit-equal XY) - it lies in a vertical plane;
//! - a solid bounded only by vertical faces between two heights is the
//!   same in every plane between them.
//!
//! Anything else - a sloped or curved face in the slab, a plane too
//! near a vertex level, an unreadable mesh - is not placed, and the
//! caller sections that plane itself, as it always did.

use crate::geometry::Triangle;

/// How far above the asked plane the slicer may probe, mm
/// (`slice::material_above` tries three planes a millionth apart), with
/// one probe of margin: a plane is placed only when every probe stays
/// in its slab.
const PROBE_REACH: f64 = 4.0e-6;

#[derive(Debug, Default)]
pub(super) struct Slabs {
    /// The distinct vertex levels, ascending. Slab `k` is
    /// `zs[k] ..= zs[k + 1]`.
    zs: Vec<f64>,
    /// Whether slab `k` is prismatic.
    prismatic: Vec<bool>,
}

impl Slabs {
    /// The slabs of `facets`. A mesh with a non-finite vertex has none:
    /// every plane is sectioned on its own and fails closed there.
    pub(super) fn new(facets: &[Triangle]) -> Self {
        let mut zs: Vec<f64> = Vec::with_capacity(facets.len() * 3);
        for t in facets {
            for p in [t.a, t.b, t.c] {
                if !p.is_finite() {
                    return Self::default();
                }
                zs.push(p.z);
            }
        }
        zs.sort_by(f64::total_cmp);
        zs.dedup();
        let n = zs.len().saturating_sub(1);
        // A difference array over the slabs: +1 where a facet that is
        // not vertical starts to cross, -1 where it stops.
        let mut delta = vec![0i64; n + 1];
        for t in facets {
            let (lo, hi) = (t.a.z.min(t.b.z).min(t.c.z), t.a.z.max(t.b.z).max(t.c.z));
            if lo >= hi || vertical(t) {
                continue; // a flat facet crosses no slab
            }
            // Both levels are in `zs`: the facet crosses slabs k0..k1.
            let k0 = zs.partition_point(|v| *v < lo);
            let k1 = zs.partition_point(|v| *v < hi);
            if let Some(d) = delta.get_mut(k0) {
                *d += 1;
            }
            if let Some(d) = delta.get_mut(k1) {
                *d -= 1;
            }
        }
        let mut open = 0i64;
        let prismatic: Vec<bool> = delta
            .iter()
            .take(n)
            .map(|d| {
                open += d;
                open == 0
            })
            .collect();
        Self { zs, prismatic }
    }

    /// The prismatic slab that holds plane `z` and every plane the
    /// slicer may probe above it; `None` when there is none.
    pub(super) fn of(&self, z: f64) -> Option<usize> {
        if !z.is_finite() {
            return None;
        }
        // zs[k] <= z < zs[k + 1]
        let k = self.zs.partition_point(|v| *v <= z).checked_sub(1)?;
        let top = *self.zs.get(k + 1)?;
        (z + PROBE_REACH <= top && self.prismatic.get(k).copied().unwrap_or(false)).then_some(k)
    }
}

/// Whether `t` lies in a vertical plane by its own vertices: two of
/// them share their XY bit for bit. No arithmetic, so no rounding can
/// call a sloped facet vertical.
fn vertical(t: &Triangle) -> bool {
    let same = |p: crate::geometry::Point3, q: crate::geometry::Point3| p.x == q.x && p.y == q.y;
    same(t.a, t.b) || same(t.b, t.c) || same(t.c, t.a)
}

#[cfg(test)]
mod tests {
    // Provenance: our own analytic fixtures (a cube, a cube under a
    // pyramid roof) for issue #3831.
    use super::*;
    use crate::geometry::{Point3, cube};

    fn block() -> Vec<Triangle> {
        cube(Point3::new(0.0, 0.0, 0.0), Point3::new(10.0, 10.0, 10.0)).facets.to_vec()
    }

    /// The block with a four-sided roof from Z 10 up to an apex at Z 15.
    fn roofed() -> Vec<Triangle> {
        let mut f = block();
        let apex = Point3::new(5.0, 5.0, 15.0);
        let c = [(0.0, 0.0), (10.0, 0.0), (10.0, 10.0), (0.0, 10.0)];
        for k in 0..4 {
            let (a, b) = (c[k], c[(k + 1) % 4]);
            f.push(Triangle::new(Point3::new(a.0, a.1, 10.0), Point3::new(b.0, b.1, 10.0), apex));
        }
        f
    }

    #[test]
    fn a_prism_is_one_slab_from_bottom_to_top() {
        let s = Slabs::new(&block());
        assert_eq!(s.of(0.0), Some(0), "the bottom plane itself: the slicer probes above it");
        assert_eq!(s.of(2.5), Some(0));
        assert_eq!(s.of(9.99), Some(0));
    }

    #[test]
    fn planes_outside_the_mesh_or_near_a_vertex_level_are_not_placed() {
        let s = Slabs::new(&block());
        assert_eq!(s.of(-0.001), None, "below the lowest vertex");
        assert_eq!(s.of(10.0), None, "at the top: nothing above");
        assert_eq!(s.of(10.0 - 1.0e-6), None, "a probe could pass the top level");
        assert_eq!(s.of(f64::NAN), None);
        assert_eq!(s.of(f64::INFINITY), None);
    }

    #[test]
    fn a_slab_with_a_sloped_facet_is_not_prismatic() {
        let s = Slabs::new(&roofed());
        assert_eq!(s.of(5.0), Some(0), "the walls under the roof are a prism");
        assert_eq!(s.of(12.0), None, "the roof section shrinks with height");
    }

    #[test]
    fn a_sloped_facet_that_spans_several_levels_closes_each_of_them() {
        // A tall sloped facet beside the block: it crosses Z 0..10 and
        // adds no vertex level between them.
        let mut f = block();
        f.push(Triangle::new(Point3::new(20.0, 0.0, 0.0), Point3::new(21.0, 0.0, 0.0), Point3::new(22.0, 1.0, 10.0)));
        assert_eq!(Slabs::new(&f).of(5.0), None);
        // A vertical one changes nothing.
        let mut f = block();
        f.push(Triangle::new(Point3::new(20.0, 0.0, 0.0), Point3::new(21.0, 0.0, 0.0), Point3::new(21.0, 0.0, 10.0)));
        assert_eq!(Slabs::new(&f).of(5.0), Some(0));
    }

    #[test]
    fn an_unreadable_or_empty_mesh_has_no_slab() {
        let mut f = block();
        f[0].a.z = f64::NAN;
        assert_eq!(Slabs::new(&f).of(5.0), None);
        assert_eq!(Slabs::new(&[]).of(5.0), None);
    }
}
