//! An edge that closed solids share (#917): two outward solids that touch
//! or overlap along a wall put one mesh edge into 2k facets, k in each
//! direction. The welders link facets into bodies over plain edges only
//! (one use each way); this test says when such a shared edge is a set of
//! solids that touch there, not a fault.
//!
//! Provenance: our own rule, after the radial edge structure of
//! non-manifold boundary representations (K. Weiler, "The radial edge
//! structure: a topological representation for non-manifold geometric
//! boundary modeling", in Geometric Modeling for CAD Applications,
//! North-Holland 1988): the faces about an edge in angular order, each
//! solid between two of them. No vendor material.
//!
//! Prime Directive: only the exact case passes. An odd or unbalanced use,
//! a body that uses the edge other than once each way (one shell that
//! meets itself, a fin on a shell), a facet whose apex lies on the edge
//! line, or solids that meet only along the line (no wall of one lies on
//! a wall of another there) - and the caller keeps its non-manifold
//! refusal.

use crate::geometry::Point3;

/// One facet on the edge: the body it belongs to (linked over plain
/// edges), its direction along the edge (`forward` = low -> high, as the
/// caller keys the edge) and its third corner.
#[derive(Clone, Copy, Debug)]
pub struct EdgeUse {
    pub body: usize,
    pub forward: bool,
    pub apex: Point3,
}

/// Two facets about the edge at one angle within this (radians) lie on
/// one plane through it: a wall of one solid on a wall of another.
const SAME_WALL_RAD: f64 = 1e-6;

/// True when the 2k uses of the edge `lo` -> `hi` (k >= 2, k each way)
/// are closed solids that touch or overlap along a wall there: each body
/// uses the edge once in each direction, and each body has a facet on the
/// plane of a facet of another body.
#[must_use]
pub fn solids_touch(lo: Point3, hi: Point3, uses: &[EdgeUse]) -> bool {
    let n = uses.len();
    let fwd = uses.iter().filter(|u| u.forward).count();
    if n < 4 || !n.is_multiple_of(2) || fwd * 2 != n {
        return false;
    }
    // Each body once each way.
    for u in uses {
        let mine = uses.iter().filter(|v| v.body == u.body);
        let (f, b) = mine.fold((0_usize, 0_usize), |(f, b), v| if v.forward { (f + 1, b) } else { (f, b + 1) });
        if f != 1 || b != 1 {
            return false;
        }
    }
    let Some(angle) = angles(lo, hi, uses) else { return false };
    // Each body has a wall on a wall of another body.
    let on = || uses.iter().map(|u| u.body).zip(angle.iter().copied());
    on().all(|(b, _)| on().filter(|&(m, _)| m == b).any(|(_, a)| on().any(|(o, c)| o != b && same_angle(a, c))))
}

fn same_angle(a: f64, b: f64) -> bool {
    let d = (a - b).rem_euclid(std::f64::consts::TAU);
    d.min(std::f64::consts::TAU - d) <= SAME_WALL_RAD
}

/// Repair a four-way contact whose faces did not link over plain edges.
/// Opposite edge directions can pair, except coincident opposed walls
/// (a zero-width wedge). Keep an existing two-use body together. Return
/// pairs of use indices only when exactly one complete matching remains.
/// This is only a proposed link: callers must recheck `solids_touch` on
/// every shared edge after all links, then check closure and volume.
#[must_use]
pub fn fragment_pairs(lo: Point3, hi: Point3, uses: &[EdgeUse]) -> Option<[(usize, usize); 2]> {
    if uses.len() != 4 {
        return None;
    }
    let f: Vec<_> = (0..4).filter(|&i| uses[i].forward).collect();
    let b: Vec<_> = (0..4).filter(|&i| !uses[i].forward).collect();
    let ([f0, f1], [b0, b1]) = (f.as_slice(), b.as_slice()) else { return None };
    let angle = angles(lo, hi, uses)?;
    let count = |i: usize| uses.iter().filter(|u| u.body == uses[i].body).count();
    let allowed = |i: usize, j: usize| {
        !same_angle(angle[i], angle[j])
            && ((uses[i].body == uses[j].body && count(i) == 2) || (count(i) == 1 && count(j) == 1))
    };
    let candidates = [[(*f0, *b0), (*f1, *b1)], [(*f0, *b1), (*f1, *b0)]];
    let mut valid = candidates.into_iter().filter(|pairs| pairs.iter().all(|&(i, j)| allowed(i, j)));
    let pairs = valid.next()?;
    valid.next().is_none().then_some(pairs)
}

/// The angle of each facet about the edge, or `None` for a zero edge or
/// an apex on the edge line (a sliver, not a wall).
fn angles(lo: Point3, hi: Point3, uses: &[EdgeUse]) -> Option<Vec<f64>> {
    let axis = hi - lo;
    let len = axis.dot(axis).sqrt();
    if !(len.is_finite() && len > 0.0) {
        return None;
    }
    let d = Point3::new(axis.x / len, axis.y / len, axis.z / len);
    let off = |r: Point3| {
        let along = r.dot(d);
        r - Point3::new(d.x * along, d.y * along, d.z * along)
    };
    // Any unit vector square to the edge, then the right-hand third axis.
    let seed = if d.x.abs() < 0.9 { Point3::new(1.0, 0.0, 0.0) } else { Point3::new(0.0, 1.0, 0.0) };
    let s = off(seed);
    let l = s.dot(s).sqrt();
    let e1 = Point3::new(s.x / l, s.y / l, s.z / l);
    let e2 = d.cross(e1);
    uses.iter()
        .map(|u| {
            let w = off(u.apex - lo);
            let (x, y) = (w.dot(e1), w.dot(e2));
            (x.is_finite() && y.is_finite() && x.hypot(y) > 1e-9 * len.max(1.0)).then(|| y.atan2(x))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn u(body: usize, forward: bool, x: f64, y: f64) -> EdgeUse {
        EdgeUse { body, forward, apex: Point3::new(x, y, 0.5) }
    }

    const LO: Point3 = Point3 { x: 0.0, y: 0.0, z: 0.0 };
    const HI: Point3 = Point3 { x: 0.0, y: 0.0, z: 1.0 };

    #[test]
    fn contact_fragments_have_exactly_one_nonzero_wedge_matching() {
        let uses = [u(0, false, 1.0, 0.0), u(1, true, 0.0, 1.0), u(2, true, 1.0, 0.0), u(3, false, 0.0, -1.0)];
        let pairs = fragment_pairs(LO, HI, &uses).expect("one complete matching");
        assert!(pairs.contains(&(1, 0)) && pairs.contains(&(2, 3)), "{pairs:?}");
        let mut linked = uses;
        for (i, j) in pairs {
            linked[j].body = linked[i].body;
        }
        assert!(solids_touch(LO, HI, &linked));
    }

    #[test]
    fn ambiguous_faulty_or_degenerate_fragments_never_link() {
        let line = [u(0, false, 1.0, 0.0), u(1, true, 0.0, 1.0), u(2, false, -1.0, 0.0), u(3, true, 0.0, -1.0)];
        assert!(fragment_pairs(LO, HI, &line).is_none(), "two matchings for line-only contact");
        let good = [u(0, false, 1.0, 0.0), u(1, true, 0.0, 1.0), u(2, true, 1.0, 0.0), u(3, false, 0.0, -1.0)];
        assert!(fragment_pairs(LO, HI, &good[..3]).is_none(), "odd edge use");
        assert!(fragment_pairs(LO, LO, &good).is_none(), "zero edge");
        let mut bad = good;
        bad[0].forward = true;
        assert!(fragment_pairs(LO, HI, &bad).is_none(), "unbalanced edge use");
        let mut bad = good;
        bad[3].apex = Point3::new(0.0, 0.0, 0.5);
        assert!(fragment_pairs(LO, HI, &bad).is_none(), "apex on edge line");
        let mut bad = good;
        bad[3].apex.x = f64::NAN;
        assert!(fragment_pairs(LO, HI, &bad).is_none(), "non-finite apex");
        let mut bad = good;
        bad[0].body = 1;
        assert!(fragment_pairs(LO, HI, &bad).is_some(), "keep an existing two-use body");
        bad[2].body = 1;
        assert!(fragment_pairs(LO, HI, &bad).is_none(), "a pinched three-use body cannot split");
    }

    /// Two boxes face to face (A in y > 0, B in y < 0, both x in 0..1):
    /// at the contact face's edge their walls y = 0 lie on one plane.
    #[test]
    fn solids_face_to_face_touch_at_the_contact_edge() {
        let uses = [u(0, false, 1.0, 0.0), u(0, true, 0.0, 1.0), u(1, true, 1.0, 0.0), u(1, false, 0.0, -1.0)];
        assert!(solids_touch(LO, HI, &uses));
    }

    /// The shipped sample's case: a flat wall of one solid crosses the
    /// curved wall of another, which runs on through the edge; the two
    /// walls on +y lie on one plane (the solids overlap there).
    #[test]
    fn overlapping_solids_with_one_wall_on_another_touch() {
        let uses = [u(0, true, 0.0, 1.0), u(0, false, 0.0, -1.0), u(1, true, 0.0, 2.0), u(1, false, 1.0, 0.0)];
        assert!(solids_touch(LO, HI, &uses));
    }

    /// What is not solids touching along a wall stays refused: odd use,
    /// unbalanced, one body twice (a shell that meets itself, or a fin),
    /// boxes that meet only along the line, an apex on the line, a zero
    /// edge, a plain edge.
    #[test]
    fn a_non_manifold_edge_never_passes() {
        let three = [u(0, false, 1.0, 0.0), u(0, true, 0.0, 1.0), u(1, true, 1.0, 0.0)];
        assert!(!solids_touch(LO, HI, &three), "odd count");
        let unbalanced = [u(0, false, 1.0, 0.0), u(0, true, 0.0, 1.0), u(1, true, 1.0, 0.0), u(1, true, 0.0, -1.0)];
        assert!(!solids_touch(LO, HI, &unbalanced), "three one way, one the other");
        let pinched = [u(0, false, 1.0, 0.0), u(0, true, 0.0, 1.0), u(0, true, 1.0, 0.0), u(0, false, 0.0, -1.0)];
        assert!(!solids_touch(LO, HI, &pinched), "one shell meets itself");
        let fin = [u(0, false, 1.0, 0.0), u(0, true, 0.0, 1.0), u(1, true, -1.0, 0.0), u(1, false, -1.0, 0.0)];
        assert!(!solids_touch(LO, HI, &fin), "a fin is no wall on a wall");
        let on_a_line = [u(0, false, 1.0, 0.0), u(0, true, 0.0, 1.0), u(1, false, -1.0, 0.0), u(1, true, 0.0, -1.0)];
        assert!(!solids_touch(LO, HI, &on_a_line), "boxes that meet only along the edge");
        let sliver = [u(0, false, 1.0, 0.0), u(0, true, 0.0, 1.0), u(1, true, 1.0, 0.0), u(1, false, 0.0, 0.0)];
        assert!(!solids_touch(LO, HI, &sliver), "apex on the edge line");
        let face = [u(0, false, 1.0, 0.0), u(0, true, 0.0, 1.0), u(1, true, 1.0, 0.0), u(1, false, 0.0, -1.0)];
        assert!(!solids_touch(LO, LO, &face), "a zero edge");
        assert!(!solids_touch(LO, HI, &face[..2]), "a plain edge is not this case");
    }
}
