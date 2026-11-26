//! The triangles of one simple polygon over its own vertices (#2844): ear
//! clipping, then side flips until every inside side is locally Delaunay,
//! so a long strip gets short sides across it and no fan along it. No
//! point is added, and a boundary side is never flipped.
//! Provenance: textbook computational geometry - ear clipping by the two
//! ears theorem (Meisters, "Polygons Have Ears", Amer. Math. Monthly 82,
//! 1975), the flip algorithm of Lawson ("Software for C1 surface
//! interpolation", 1977) and the in-circle determinant (de Berg et al.,
//! "Computational Geometry", ch. 9). No HSMWorks material consulted.

use std::collections::HashMap;

/// The most vertices a polygon may have here: ear clipping is quadratic.
pub const MAX_FILL_VERTICES: usize = 8192;

/// Twice the signed area of the triangle `a b c`.
pub(crate) fn area2(a: [f64; 2], b: [f64; 2], c: [f64; 2]) -> f64 {
    (b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0])
}

/// The triangles of the counter-clockwise simple polygon `p`, each
/// counter-clockwise, as indices into `p`: `p.len() - 2` of them. `None`
/// for a polygon past [`MAX_FILL_VERTICES`], one with a point that is not
/// finite, and one where no ear is found (it crosses or touches itself).
pub(crate) fn fill(p: &[[f64; 2]]) -> Option<Vec<[u32; 3]>> {
    let n = p.len();
    if !(3..=MAX_FILL_VERTICES).contains(&n) || p.iter().flatten().any(|x| !x.is_finite()) {
        return None;
    }
    let mut triangles = clip_ears(p)?;
    flip_to_delaunay(p, &mut triangles);
    Some(triangles)
}

/// Ear clipping: a corner that turns left and holds no other vertex of
/// what is left of the polygon is cut off, until a triangle is left.
fn clip_ears(p: &[[f64; 2]]) -> Option<Vec<[u32; 3]>> {
    let n = p.len();
    let at = |i: usize| p.get(i).copied();
    let mut prev: Vec<usize> = (0..n).map(|i| (i + n - 1) % n).collect();
    let mut next: Vec<usize> = (0..n).map(|i| (i + 1) % n).collect();
    let id = |i: usize| u32::try_from(i).ok();
    let mut out = Vec::with_capacity(n.saturating_sub(2));
    let (mut left, mut cur, mut misses) = (n, 0_usize, 0_usize);
    while left > 3 {
        let (a, c) = (*prev.get(cur)?, *next.get(cur)?);
        let (pa, pb, pc) = (at(a)?, at(cur)?, at(c)?);
        let mut ear = area2(pa, pb, pc) > 0.0;
        let mut v = *next.get(c)?;
        while ear && v != a {
            let q = at(v)?;
            ear = !(area2(pa, pb, q) >= 0.0 && area2(pb, pc, q) >= 0.0 && area2(pc, pa, q) >= 0.0);
            v = *next.get(v)?;
        }
        if ear {
            out.push([id(a)?, id(cur)?, id(c)?]);
            *next.get_mut(a)? = c;
            *prev.get_mut(c)? = a;
            left -= 1;
            misses = 0;
        } else {
            misses += 1;
            if misses > left {
                return None;
            }
        }
        cur = c;
    }
    let (a, c) = (*prev.get(cur)?, *next.get(cur)?);
    out.push([id(a)?, id(cur)?, id(c)?]);
    // What is left turns left too, or the polygon was not a simple one.
    (area2(at(a)?, at(cur)?, at(c)?) >= 0.0).then_some(out)
}

/// `d` lies inside the circle through the counter-clockwise `a b c`, by
/// more than rounding: the determinant against the size of its terms.
fn in_circle(a: [f64; 2], b: [f64; 2], c: [f64; 2], d: [f64; 2]) -> bool {
    let row = |q: [f64; 2]| {
        let (x, y) = (q[0] - d[0], q[1] - d[1]);
        [x, y, x * x + y * y]
    };
    let (r, s, t) = (row(a), row(b), row(c));
    let terms = [
        r[0] * (s[1] * t[2] - s[2] * t[1]),
        -r[1] * (s[0] * t[2] - s[2] * t[0]),
        r[2] * (s[0] * t[1] - s[1] * t[0]),
    ];
    let size: f64 = terms.iter().map(|x| x.abs()).sum();
    terms.iter().sum::<f64>() > 1e-12 * size
}

/// Lawson's flips: an inside side whose far corner lies in its triangle's
/// circle is turned to the other diagonal, where the two triangles make a
/// convex quad. Bounded: at most `n * n` flips (the textbook bound).
fn flip_to_delaunay(p: &[[f64; 2]], triangles: &mut [[u32; 3]]) {
    let at = |i: u32| p.get(i as usize).copied();
    let mut owner: HashMap<(u32, u32), usize> = HashMap::with_capacity(triangles.len().saturating_mul(3));
    let mut stack: Vec<(u32, u32)> = Vec::new();
    for (k, t) in triangles.iter().enumerate() {
        for side in [(t[0], t[1]), (t[1], t[2]), (t[2], t[0])] {
            owner.insert(side, k);
            stack.push(side);
        }
    }
    // The corner of triangle `k` that is not on its side `a b`.
    let apex = |triangles: &[[u32; 3]], k: usize, a: u32, b: u32| {
        triangles.get(k).and_then(|t| t.iter().copied().find(|i| *i != a && *i != b))
    };
    let mut budget = p.len().saturating_mul(p.len());
    while let Some((a, b)) = stack.pop() {
        let (Some(&k1), Some(&k2)) = (owner.get(&(a, b)), owner.get(&(b, a))) else { continue };
        let (Some(c), Some(d)) = (apex(triangles, k1, a, b), apex(triangles, k2, a, b)) else { continue };
        let (Some(pa), Some(pb), Some(pc), Some(pd)) = (at(a), at(b), at(c), at(d)) else { continue };
        if !in_circle(pa, pb, pc, pd) || area2(pa, pd, pc) <= 0.0 || area2(pd, pb, pc) <= 0.0 {
            continue;
        }
        if budget == 0 {
            return;
        }
        budget -= 1;
        if let Some(t) = triangles.get_mut(k1) {
            *t = [a, d, c];
        }
        if let Some(t) = triangles.get_mut(k2) {
            *t = [d, b, c];
        }
        owner.remove(&(a, b));
        owner.remove(&(b, a));
        owner.insert((d, c), k1);
        owner.insert((c, d), k2);
        owner.insert((a, d), k1);
        owner.insert((b, c), k2);
        stack.extend([(a, d), (d, b), (b, c), (c, a)]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A strip 100 long and 1 wide, `n` nodes on each long side, the far
    /// side shifted half a step.
    fn strip(n: usize) -> Vec<[f64; 2]> {
        let step = 100.0 / (n - 1) as f64;
        let mut p: Vec<[f64; 2]> = (0..n).map(|i| [i as f64 * step, 0.0]).collect();
        p.extend((0..n).rev().map(|i| [(i as f64 * step + 0.5 * step).min(100.0), 1.0]));
        p
    }

    fn checked(p: &[[f64; 2]]) -> Vec<[u32; 3]> {
        let t = fill(p).expect("a simple polygon fills");
        assert_eq!(t.len(), p.len() - 2);
        let area = |t: &[u32; 3]| area2(p[t[0] as usize], p[t[1] as usize], p[t[2] as usize]);
        assert!(t.iter().all(|t| area(t) >= 0.0), "every triangle counter-clockwise");
        let whole: f64 = (1..p.len() - 1).map(|i| area2(p[0], p[i], p[i + 1])).sum();
        let sum: f64 = t.iter().map(area).sum();
        assert!((sum - whole).abs() < 1e-9 * whole.abs(), "the triangles cover the polygon: {sum} of {whole}");
        t
    }

    #[test]
    fn a_long_strip_is_filled_with_short_sides_across_it() {
        let p = strip(200);
        let t = checked(&p);
        let longest = t
            .iter()
            .flat_map(|t| [(t[0], t[1]), (t[1], t[2]), (t[2], t[0])])
            .map(|(a, b)| (p[a as usize][0] - p[b as usize][0]).hypot(p[a as usize][1] - p[b as usize][1]))
            .fold(0.0, f64::max);
        assert!(longest < 1.5, "no side runs along the strip: {longest}");
    }

    #[test]
    fn collinear_runs_and_a_notch_keep_every_boundary_side() {
        // An L with collinear nodes on each side: 5 reflex-free runs and
        // one reflex corner.
        let mut p = Vec::new();
        p.extend((0..8).map(|i| [i as f64, 0.0]));
        p.extend((0..4).map(|j| [8.0, j as f64]));
        p.extend((0..4).map(|i| [8.0 - i as f64, 4.0]));
        p.extend((0..4).map(|j| [4.0, 4.0 + j as f64]));
        p.extend((0..4).map(|i| [4.0 - i as f64, 8.0]));
        p.extend((0..8).map(|j| [0.0, 8.0 - j as f64]));
        let t = checked(&p);
        let n = p.len() as u32;
        for i in 0..n {
            let side = (i, (i + 1) % n);
            assert!(
                t.iter().any(|t| [(t[0], t[1]), (t[1], t[2]), (t[2], t[0])].contains(&side)),
                "boundary side {side:?} is a triangle's side"
            );
        }
    }

    #[test]
    fn a_polygon_that_cannot_be_filled_is_refused() {
        // A bow tie: its sides cross.
        assert_eq!(fill(&[[0.0, 0.0], [2.0, 2.0], [2.0, 0.0], [0.0, 2.0]]), None);
        assert_eq!(fill(&[[0.0, 0.0], [1.0, 0.0]]), None);
        assert_eq!(fill(&[[0.0, 0.0], [1.0, 0.0], [f64::NAN, 1.0]]), None);
        assert_eq!(fill(&vec![[0.0, 0.0]; MAX_FILL_VERTICES + 1]), None);
    }
}
