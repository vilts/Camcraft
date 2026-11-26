//! Open polylines clipped by a region (#3316): the stretches of a path
//! that lie inside (or outside) an even-odd [`Region`]. Provenance: the
//! line-against-polygon clip of the published computer-graphics
//! literature (split each segment at every boundary crossing, then keep
//! or drop each piece by the parity of its midpoint - the
//! Weiler-Atherton family); our own code, no vendor material consulted.
//!
//! Every point of a kept piece is a vertex of the input or a crossing on
//! one of its segments, so a piece runs exactly where the path ran, in
//! the same direction. A stretch that lies ON the region's boundary is
//! decided by [`Region::contains`]' half-open ray rule; a caller that
//! needs a definite answer there clips against the region grown or
//! shrunk by its tolerance.

use super::{COORD_LIMIT, GeoError, Point2, Region};

/// A closed ring clipped by a region.
#[derive(Clone, Debug, PartialEq)]
pub enum RingClip {
    /// Every stretch of the ring is kept: walk it closed.
    Whole,
    /// The kept stretches as open paths, in the ring's walking order
    /// (cyclic: the first one may wrap over the ring's first vertex).
    Pieces(Vec<Vec<Point2>>),
}

/// A kept stretch and whether it starts at the path's first point and
/// ends at its last.
struct Stretch {
    pts: Vec<Point2>,
    from_start: bool,
    to_end: bool,
}

/// One boundary edge with its bounding box, for the crossing scan.
struct Edge {
    a: Point2,
    b: Point2,
    lo: Point2,
    hi: Point2,
}

impl Region {
    /// The stretches of the open polyline `path` inside this region
    /// (`keep_inside`) or outside it, in the path's order and direction.
    /// A non-finite or out-of-range vertex is refused; an empty path or
    /// a single point gives no stretch.
    pub fn clip_polyline(&self, path: &[Point2], keep_inside: bool) -> Result<Vec<Vec<Point2>>, GeoError> {
        Ok(self.stretches(path, keep_inside)?.into_iter().map(|s| s.pts).collect())
    }

    /// The closed ring `ring` (its last vertex joins its first; a
    /// repeated closing vertex is allowed) clipped like
    /// [`Region::clip_polyline`]: [`RingClip::Whole`] when nothing is
    /// cut away, else the kept stretches with the one that wraps over
    /// the first vertex joined.
    pub fn clip_ring(&self, ring: &[Point2], keep_inside: bool) -> Result<RingClip, GeoError> {
        let Some(&first) = ring.first() else {
            return Ok(RingClip::Pieces(Vec::new()));
        };
        let mut walk = ring.to_vec();
        if walk.last() != Some(&first) {
            walk.push(first);
        }
        let mut parts = self.stretches(&walk, keep_inside)?;
        if parts.len() == 1 && parts[0].from_start && parts[0].to_end {
            return Ok(RingClip::Whole);
        }
        if parts.len() >= 2 && parts[0].from_start && parts[parts.len() - 1].to_end {
            let head = parts.remove(0);
            if let Some(tail) = parts.last_mut() {
                tail.pts.extend(head.pts.iter().skip(1).copied());
            }
            parts.rotate_right(1);
        }
        Ok(RingClip::Pieces(parts.into_iter().map(|s| s.pts).collect()))
    }

    fn stretches(&self, path: &[Point2], keep_inside: bool) -> Result<Vec<Stretch>, GeoError> {
        let pts = checked(path)?;
        let edges = self.edges();
        let mut out: Vec<Stretch> = Vec::new();
        let mut cur: Option<Stretch> = None;
        let last_seg = pts.len().saturating_sub(2);
        for (i, w) in pts.windows(2).enumerate() {
            let (a, b) = (w[0], w[1]);
            let mut ts = vec![0.0, 1.0];
            crossings(a, b, &edges, &mut ts);
            ts.sort_by(f64::total_cmp);
            let len = (b.x - a.x).hypot(b.y - a.y);
            ts.dedup_by(|x, y| (*x - *y).abs() * len <= 1e-12);
            let at = |t: f64| {
                if t <= 0.0 {
                    a
                } else if t >= 1.0 {
                    b
                } else {
                    Point2::new(a.x + t * (b.x - a.x), a.y + t * (b.y - a.y))
                }
            };
            for (k, tw) in ts.windows(2).enumerate() {
                let (p, q) = (at(tw[0]), at(tw[1]));
                let kept = self.contains(at(0.5 * (tw[0] + tw[1]))) == keep_inside;
                if kept {
                    let s = cur.get_or_insert_with(|| Stretch {
                        pts: vec![p],
                        from_start: i == 0 && k == 0,
                        to_end: false,
                    });
                    if s.pts.last() != Some(&q) {
                        s.pts.push(q);
                    }
                    s.to_end = i == last_seg && k + 2 == ts.len();
                } else if let Some(s) = cur.take() {
                    out.push(s);
                }
            }
        }
        if let Some(s) = cur.take() {
            out.push(s);
        }
        out.retain(|s| s.pts.len() >= 2 && length(&s.pts) > 1e-9);
        Ok(out)
    }

    fn edges(&self) -> Vec<Edge> {
        let mut out = Vec::new();
        for l in self.loops() {
            let p = l.points();
            for i in 0..p.len() {
                let (a, b) = (p[i], p[(i + 1) % p.len()]);
                let lo = Point2::new(a.x.min(b.x), a.y.min(b.y));
                let hi = Point2::new(a.x.max(b.x), a.y.max(b.y));
                out.push(Edge { a, b, lo, hi });
            }
        }
        out
    }
}

/// The path's vertices, finite and in range, repeats dropped.
fn checked(path: &[Point2]) -> Result<Vec<Point2>, GeoError> {
    let mut out: Vec<Point2> = Vec::with_capacity(path.len());
    for &p in path {
        if !(p.x.is_finite() && p.y.is_finite()) {
            return Err(GeoError::Invalid("non-finite polyline vertex".into()));
        }
        if p.x.abs() >= COORD_LIMIT || p.y.abs() >= COORD_LIMIT {
            return Err(GeoError::OutOfRange);
        }
        if out.last() != Some(&p) {
            out.push(p);
        }
    }
    Ok(out)
}

fn length(pts: &[Point2]) -> f64 {
    pts.windows(2).map(|w| (w[1].x - w[0].x).hypot(w[1].y - w[0].y)).sum()
}

/// Push the parameters on `a -> b` (strictly inside 0..1) where it meets
/// a boundary edge: proper and touching crossings, and both ends of a
/// collinear overlap.
fn crossings(a: Point2, b: Point2, edges: &[Edge], ts: &mut Vec<f64>) {
    let (dx, dy) = (b.x - a.x, b.y - a.y);
    let dd = dx * dx + dy * dy;
    if dd <= 0.0 {
        return;
    }
    let (lo, hi) = (Point2::new(a.x.min(b.x), a.y.min(b.y)), Point2::new(a.x.max(b.x), a.y.max(b.y)));
    let mut push = |t: f64| {
        if t > 0.0 && t < 1.0 {
            ts.push(t);
        }
    };
    for e in edges {
        if e.hi.x < lo.x || e.lo.x > hi.x || e.hi.y < lo.y || e.lo.y > hi.y {
            continue;
        }
        let (ex, ey) = (e.b.x - e.a.x, e.b.y - e.a.y);
        let (wx, wy) = (e.a.x - a.x, e.a.y - a.y);
        let den = dx * ey - dy * ex;
        let scale = dd.sqrt() * ex.hypot(ey);
        if den.abs() <= 1e-12 * scale {
            // Parallel: a collinear overlap splits at the edge's ends.
            if (wx * dy - wy * dx).abs() <= 1e-9 * dd.sqrt() {
                for q in [e.a, e.b] {
                    push(((q.x - a.x) * dx + (q.y - a.y) * dy) / dd);
                }
            }
            continue;
        }
        let t = (wx * ey - wy * ex) / den;
        let u = (wx * dy - wy * dx) / den;
        if (-1e-12..=1.0 + 1e-12).contains(&u) {
            push(t);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geo2d::Loop;

    fn sq(x0: f64, y0: f64, x1: f64, y1: f64) -> Loop {
        let p = |x, y| Point2::new(x, y);
        let Ok(l) = Loop::new(vec![p(x0, y0), p(x1, y0), p(x1, y1), p(x0, y1)]) else { panic!("loop") };
        l
    }

    fn annulus() -> Region {
        let Ok(r) = Region::new(vec![sq(0.0, 0.0, 10.0, 10.0), sq(4.0, 4.0, 6.0, 6.0)]) else { panic!("region") };
        r
    }

    fn near(p: Option<&Point2>, x: f64, y: f64) -> bool {
        p.is_some_and(|p| (p.x - x).abs() < 1e-9 && (p.y - y).abs() < 1e-9)
    }

    #[test]
    fn a_segment_across_a_square_keeps_its_inside_stretch_in_its_direction() {
        let r = annulus();
        let path = [Point2::new(-5.0, 2.0), Point2::new(15.0, 2.0)];
        let Ok(inside) = r.clip_polyline(&path, true) else { panic!("clip") };
        assert_eq!(inside.len(), 1, "{inside:?}");
        assert!(near(inside[0].first(), 0.0, 2.0) && near(inside[0].last(), 10.0, 2.0), "{inside:?}");
        let Ok(outside) = r.clip_polyline(&path, false) else { panic!("clip") };
        assert_eq!(outside.len(), 2, "{outside:?}");
        assert!(near(outside[0].first(), -5.0, 2.0) && near(outside[1].last(), 15.0, 2.0), "{outside:?}");
        // Across the hole: two inside stretches, walked left to right.
        let Ok(holed) = r.clip_polyline(&[Point2::new(-1.0, 5.0), Point2::new(11.0, 5.0)], true) else { panic!("clip") };
        assert_eq!(holed.len(), 2, "{holed:?}");
        assert!(near(holed[0].first(), 0.0, 5.0) && near(holed[0].last(), 4.0, 5.0), "{holed:?}");
        assert!(near(holed[1].first(), 6.0, 5.0) && near(holed[1].last(), 10.0, 5.0), "{holed:?}");
    }

    #[test]
    fn a_ring_is_whole_or_cut_into_ordered_pieces_with_the_wrap_joined() {
        let r = annulus();
        let ring = |x0: f64, y0: f64, x1: f64, y1: f64| sq(x0, y0, x1, y1).points().to_vec();
        assert_eq!(r.clip_ring(&ring(1.0, 1.0, 9.0, 9.0), true), Ok(RingClip::Whole));
        assert_eq!(r.clip_ring(&ring(1.0, 1.0, 9.0, 9.0), false), Ok(RingClip::Pieces(Vec::new())));
        // The ring (3,3)-(5,7) runs through the hole on x = 5 between
        // y 4 and 6: one stretch, joined over the first vertex (3,3).
        let Ok(RingClip::Pieces(p)) = r.clip_ring(&ring(3.0, 3.0, 5.0, 7.0), true) else { panic!("pieces") };
        assert_eq!(p.len(), 1, "{p:?}");
        assert!(near(p[0].first(), 5.0, 6.0) && near(p[0].last(), 5.0, 4.0), "{p:?}");
        assert!(p[0].iter().any(|q| near(Some(q), 3.0, 3.0)), "the wrap keeps the first vertex: {p:?}");
        // Kept outside instead: the stretch in the hole, in the ring's direction.
        let Ok(RingClip::Pieces(h)) = r.clip_ring(&ring(3.0, 3.0, 5.0, 7.0), false) else { panic!("pieces") };
        assert_eq!(h.len(), 1, "{h:?}");
        assert!(near(h[0].first(), 5.0, 4.0) && near(h[0].last(), 5.0, 6.0), "{h:?}");
        let Ok(RingClip::Pieces(q)) = r.clip_ring(&ring(7.0, 1.0, 9.0, 9.0), false) else { panic!("pieces") };
        assert!(q.is_empty(), "a ring inside the region keeps nothing outside: {q:?}");
    }

    #[test]
    fn degenerate_paths_give_clean_answers() {
        let r = annulus();
        assert_eq!(r.clip_polyline(&[], true), Ok(Vec::new()));
        assert_eq!(r.clip_polyline(&[Point2::new(1.0, 1.0)], true), Ok(Vec::new()));
        let same = [Point2::new(1.0, 1.0), Point2::new(1.0, 1.0)];
        assert_eq!(r.clip_polyline(&same, true), Ok(Vec::new()), "a zero-length path keeps nothing");
        assert!(r.clip_polyline(&[Point2::new(f64::NAN, 0.0), Point2::new(1.0, 1.0)], true).is_err());
        assert_eq!(r.clip_polyline(&[Point2::new(2e12, 0.0), Point2::new(1.0, 1.0)], true), Err(GeoError::OutOfRange));
        // A path along the boundary: an answer, never a panic or a hang.
        let edge = [Point2::new(0.0, 0.0), Point2::new(10.0, 0.0), Point2::new(10.0, 10.0)];
        let Ok(on) = r.clip_polyline(&edge, true) else { panic!("boundary clip") };
        assert!(on.iter().flatten().all(|p| p.x >= 0.0 && p.x <= 10.0 && p.y >= 0.0 && p.y <= 10.0));
        // An empty region keeps all outside and nothing inside.
        let path = [Point2::new(0.0, 0.0), Point2::new(3.0, 4.0)];
        assert_eq!(Region::empty().clip_polyline(&path, true), Ok(Vec::new()));
        assert_eq!(Region::empty().clip_polyline(&path, false), Ok(vec![path.to_vec()]));
        assert_eq!(Region::empty().clip_ring(sq(0.0, 0.0, 1.0, 1.0).points(), false), Ok(RingClip::Whole));
    }

    fn total(pieces: &[Vec<Point2>]) -> f64 {
        pieces.iter().map(|p| length(p)).sum()
    }

    #[test]
    fn a_vertex_on_the_boundary_a_tangent_touch_and_an_edge_overlap() {
        let r = annulus();
        // A vertex exactly on the outer edge: the inside stretch starts there.
        let Ok(v) = r.clip_polyline(&[Point2::new(-5.0, 2.0), Point2::new(0.0, 2.0), Point2::new(3.0, 2.0)], true) else {
            panic!("clip")
        };
        assert!(v.len() == 1 && near(v[0].first(), 0.0, 2.0) && near(v[0].last(), 3.0, 2.0), "{v:?}");
        // A segment that only touches the corner (10, 10) from outside:
        // nothing inside, and the outside stretch stays one piece.
        let touch = [Point2::new(5.0, 15.0), Point2::new(15.0, 5.0)];
        let Ok(t_in) = r.clip_polyline(&touch, true) else { panic!("clip") };
        assert!(total(&t_in) < 1e-9, "a touch keeps no length inside: {t_in:?}");
        let Ok(t_out) = r.clip_polyline(&touch, false) else { panic!("clip") };
        assert!((total(&t_out) - 200f64.sqrt()).abs() < 1e-9, "{t_out:?}");
        // A segment along the bottom edge in mid-segment: the inside and the
        // outside stretches split it exactly, every point on the line.
        let along = [Point2::new(-5.0, 0.0), Point2::new(15.0, 0.0)];
        let (Ok(a_in), Ok(a_out)) = (r.clip_polyline(&along, true), r.clip_polyline(&along, false)) else { panic!("clip") };
        assert!((total(&a_in) + total(&a_out) - 20.0).abs() < 1e-9, "{a_in:?} {a_out:?}");
        assert!(a_in.iter().chain(&a_out).flatten().all(|p| p.y == 0.0));
        assert!(a_out.iter().flatten().any(|p| near(Some(p), 0.0, 0.0)), "the overlap splits at its ends: {a_out:?}");
    }
}
