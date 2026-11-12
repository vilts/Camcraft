//! Issue #1091 review: the closed-shell gate of the stock-top re-reading - our own exact test (each edge shared by exactly two facets, once each way; one connected, outward shell), from the standard mesh-topology definition of a closed oriented 2-manifold surface; no vendor material consulted.
//!
//! The re-reading's bound ("the material over a plan point stands no
//! higher than a facet whose plan covers it") is a ray argument: a point
//! enclosed by a closed surface sends its upward ray across the surface
//! an odd number of times, so some facet covers its plan above it. A
//! hole in the shell lets the ray escape, and the bound is lost - a boss
//! without its top facets reads as air. So the re-reading may only use a
//! solid this gate passes; anything it refuses keeps the conviction.
//!
//! What passes: a non-empty facet list, every vertex finite, no facet
//! degenerate in 3-D (two equal vertices or zero area), every undirected
//! edge (endpoints matched exactly, bit for bit) used by exactly two
//! facets and in opposite directions (so a duplicated facet, a
//! non-manifold edge of three or more facets, or a flipped neighbour is
//! refused), the facets forming ONE edge-connected shell, and a positive
//! signed volume (outward-wound; an inside-out shell is refused). More
//! than the ray argument needs, on purpose: the gate refuses what the
//! rest of the program would read ambiguously, and a refusal is only a
//! conviction kept. Not tested: self-intersection - the ray argument
//! holds for any closed surface, self-intersecting or not, so the bound
//! does not rest on it. Cost: one pass over the facets with a hash map
//! of edges, O(F) expected, once per solid per operation - never per
//! rapid.

use std::collections::HashMap;

use crate::geometry::{Point3, Triangle};

type Key = [u64; 3];
/// An edge's uses: the facet using it low->high, the one high->low, and the count.
type Uses = (Option<usize>, Option<usize>, u32);

fn key(p: Point3) -> Key {
    // +0.0 and -0.0 are one point.
    let b = |v: f64| if v == 0.0 { 0.0f64.to_bits() } else { v.to_bits() };
    [b(p.x), b(p.y), b(p.z)]
}

/// Whether `facets` is one closed, outward, oriented shell (module doc).
pub(super) fn closed_shell(facets: &[Triangle]) -> bool {
    if facets.is_empty() {
        return false;
    }
    // Edge (low key, high key) -> (facet of the low->high use, of the high->low use, uses).
    let mut edges: HashMap<(Key, Key), Uses> = HashMap::with_capacity(facets.len() * 2);
    let mut volume = 0.0f64;
    for (i, t) in facets.iter().enumerate() {
        let v = [t.a, t.b, t.c];
        if !v.iter().all(|p| p.x.is_finite() && p.y.is_finite() && p.z.is_finite()) {
            return false;
        }
        let k = [key(t.a), key(t.b), key(t.c)];
        if k[0] == k[1] || k[1] == k[2] || k[2] == k[0] {
            return false;
        }
        let (u, w) = (t.b - t.a, t.c - t.a);
        let n = u.cross(w);
        let n2 = n.dot(n);
        if n2.is_nan() || n2 <= 0.0 {
            return false;
        }
        volume += t.a.dot(t.b.cross(t.c));
        for j in 0..3 {
            let (a, b) = (k[j], k[(j + 1) % 3]);
            let (lo, hi, forward) = if a < b { (a, b, true) } else { (b, a, false) };
            let e = edges.entry((lo, hi)).or_insert((None, None, 0));
            e.2 += 1;
            let slot = if forward { &mut e.0 } else { &mut e.1 };
            if slot.is_some() {
                return false; // the same direction twice: duplicate or flipped neighbour
            }
            *slot = Some(i);
        }
    }
    if !volume.is_finite() || volume <= 0.0 {
        return false;
    }
    // Every edge exactly twice, once each way; union the two facets.
    let mut parent: Vec<usize> = (0..facets.len()).collect();
    fn root(p: &mut [usize], mut x: usize) -> usize {
        while let Some(&up) = p.get(x) {
            if up == x {
                break;
            }
            let grand = p.get(up).copied().unwrap_or(up);
            if let Some(s) = p.get_mut(x) {
                *s = grand;
            }
            x = up;
        }
        x
    }
    for (f, b, uses) in edges.values() {
        let (Some(f), Some(b)) = (f, b) else { return false };
        if *uses != 2 {
            return false;
        }
        let (rf, rb) = (root(&mut parent, *f), root(&mut parent, *b));
        if rf != rb
            && let Some(s) = parent.get_mut(rf)
        {
            *s = rb;
        }
    }
    let first = root(&mut parent, 0);
    (1..facets.len()).all(|i| root(&mut parent, i) == first)
}
