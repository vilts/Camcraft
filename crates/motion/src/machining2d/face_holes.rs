//! Facing rows honouring the region's holes (M13.11, the M1.5 leftover):
//! the facer still clears the bounding band of its region - rows across
//! the full Y span, an overrun past each X end, `face_band_sane`'s law -
//! but a hole in the region now bounds the rows the way it bounds a
//! pocket's: the tool centre keeps out of every hole grown by the tool
//! radius (the pocket's cutter-centre area), the pieces of a row that
//! are left get cut, and the row crosses the hole in the air at the
//! clearance plane - or, when the user asks and the hole is air, straight
//! across at feed. Islands (material standing above the face inside a
//! hole of the region) are always lifted over.
//!
//! Provenance: our own design notes - the cutter-centre area of a
//! pocket (region less islands grown by the tool radius) is the textbook
//! construction of any CAM manual's 2D pocketing chapter; the row clip
//! is the plane-sweep parity walk `finish_clip` already uses. No vendor
//! material consulted.

use crate::geo2d::{GeoError, Point2, Region};

/// Below this (mm^2) a hole-caused leftover is nothing.
const AREA_EPS: f64 = 1e-3;

/// How far (mm) a hole may stand off the material at the cut plane and
/// still be bounded by it (#2437): past the silhouettes' chord error,
/// far inside any strip a row could cut.
const BESIDE: f64 = 0.05;

/// How a facing row meets a hole in its region.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum FaceHoles {
    /// Every hole is bridged in the air: the row stops a tool radius
    /// short of it, lifts to the clearance plane, crosses, and plunges
    /// back in. Safe whether the hole is a bore (air) or a boss
    /// (solid) - the default. With the outline of the material at the
    /// cut plane in hand (#2437), a hole that no such material stands
    /// in or beside is wholly below the plane and plays no part.
    #[default]
    Lift,
    /// Holes that are air (nothing of the material stands in them) are
    /// cut straight across at feed; islands - holes the material does
    /// stand in - are lifted over. With no material outline in hand
    /// every hole reads as an island, and the path's notes say so.
    CutAir,
    /// Rows run the bounding band end to end, holes ignored: the M1.3
    /// serpentine every pre-M13.11 file holds, to the byte.
    Through,
}

/// The holes of `region`: the area its outer loops enclose that is not
/// material - every hole, less any island standing inside one.
pub(super) fn voids(region: &Region) -> Result<Region, GeoError> {
    let loops = region.loops();
    let holes: Vec<Region> = loops
        .iter()
        .filter(|l| !l.is_ccw())
        .map(|l| Region::new(vec![l.clone()]))
        .collect::<Result<_, _>>()?;
    // A material loop is an outer when no hole encloses it (the loops
    // are disjoint, so one vertex decides); one inside a hole is an
    // island, and part of the region's material already.
    let outers: Vec<_> = loops
        .iter()
        .filter(|l| l.is_ccw())
        .filter(|l| {
            l.points()
                .first()
                .is_none_or(|p| !holes.iter().any(|h| h.contains(*p)))
        })
        .cloned()
        .collect();
    if outers.is_empty() {
        return Ok(Region::empty());
    }
    Region::new(outers)?.difference(region)
}

/// The holes a row bridges under `law`, and where the tool centre may
/// NOT stand for them: those holes grown by the tool radius `r`.
/// `solid` is the outline of material standing above the face (what
/// makes a hole an island) under `CutAir`, and of the material standing
/// at or above it under `Lift` (what a hole must touch to count,
/// [`beside`]); the note is for the path when the law had to fall back.
/// Both regions are empty under `Through`, and for a region without
/// holes.
pub(super) struct Bridged {
    /// The holes bridged, as the region holds them.
    pub holes: Region,
    /// Those holes grown by the tool radius: the row keeps out of it.
    pub grown: Region,
    pub note: Option<String>,
}

pub(super) fn forbidden(
    region: &Region,
    law: FaceHoles,
    solid: Option<&Region>,
    r: f64,
) -> Result<Bridged, GeoError> {
    let none = |note| Bridged { holes: Region::empty(), grown: Region::empty(), note };
    if law == FaceHoles::Through {
        return Ok(none(None));
    }
    let v = voids(region)?;
    if v.is_empty() {
        return Ok(none(None));
    }
    let (holes, note) = match (law, solid) {
        (FaceHoles::CutAir, Some(s)) => (v.intersect(s)?, None),
        (FaceHoles::CutAir, None) => (
            v,
            Some("no material outline to tell air from an island: every hole is lifted over".to_string()),
        ),
        (FaceHoles::Lift, Some(s)) => (beside(&v, s)?, None),
        _ => (v, None),
    };
    if holes.is_empty() {
        return Ok(none(note));
    }
    let grown = holes.offset(r)?;
    Ok(Bridged { holes, grown, note })
}

/// The holes of `voids` that `present` - the material standing at or
/// above the cut plane - stands in or within [`BESIDE`] of (#2437). A
/// hole it does not touch opens wholly below the plane: the row above
/// it cuts stock or air, never the hole's rim, so it is no hole of the
/// face. A boss (material in the hole) and a bore whose rim the plane
/// reaches are both kept, whole.
fn beside(voids: &Region, present: &Region) -> Result<Region, GeoError> {
    if present.is_empty() {
        return Ok(Region::empty());
    }
    let near = present.offset(BESIDE)?;
    let mut kept = Region::empty();
    for l in voids.loops().iter().filter(|l| l.is_ccw()) {
        let whole = Region::new(vec![l.clone()])?;
        if !whole.intersect(&near)?.is_empty() {
            kept = kept.union(&whole)?;
        }
    }
    voids.intersect(&kept)
}

/// Where the horizontal line `y` crosses the edges of `region`, sorted:
/// the parity walk, half-open at the vertices so a vertex on the line
/// counts once; consecutive pairs bound the inside. Keep the boundary
/// identity so a concave return in one hole is not called two holes.
fn crossings(region: &Region, y: f64) -> Vec<(f64, usize)> {
    let mut xs: Vec<(f64, usize)> = Vec::new();
    for (boundary, l) in region.loops().iter().enumerate() {
        let pts = l.points();
        let n = pts.len();
        for i in 0..n {
            let (p, q) = (pts[i], pts[(i + 1) % n]);
            if (p.y <= y) != (q.y <= y) {
                xs.push((p.x + (y - p.y) * (q.x - p.x) / (q.y - p.y), boundary));
            }
        }
    }
    xs.sort_by(|a, b| a.0.total_cmp(&b.0));
    xs
}

/// The first missing row piece between distinct hole boundaries whose
/// horizontal gap is no wider than the tool. A concave return in one
/// hole can exclude a row at any width; it belongs to `uncut`'s swept-area
/// account instead of the refusal about material between two holes.
/// Returns `(row y, gap width, gap midpoint x)` - the midpoint is where
/// the refusal points (#261 part 2).
pub(super) fn narrow_gap(
    holes: &Region,
    rows: &[(f64, f64, f64)],
    pieces: &[Vec<(f64, f64)>],
    diameter: f64,
) -> Option<(f64, f64, f64)> {
    rows.iter().zip(pieces).find_map(|(&(y, _, _), ps)| {
        let xs = crossings(holes, y);
        // The material gaps between consecutive hole spans on the row.
        xs.windows(2)
            .skip(1)
            .step_by(2)
            .map(|w| (w[0], w[1]))
            .find(|&((a, left), (b, right))| {
                left != right
                    && b > a
                    && b - a <= diameter
                    && !ps.iter().any(|&(p0, p1)| p0.min(p1) < b && p0.max(p1) > a)
            })
            .map(|((a, _), (b, _))| (y, b - a, 0.5 * (a + b)))
    })
}

/// The pieces of the row `y` from `xa` to `xb` (in that order, either
/// way) that keep out of `forbidden`: the row's own endpoints where
/// nothing is forbidden, so a hole-free row is exactly the row.
pub(super) fn pieces(forbidden: &Region, y: f64, xa: f64, xb: f64) -> Vec<(f64, f64)> {
    if forbidden.is_empty() {
        return vec![(xa, xb)];
    }
    let xs = crossings(forbidden, y);
    let (lo, hi) = (xa.min(xb), xa.max(xb));
    // Inside intervals are consecutive pairs; the row keeps what lies
    // between them.
    let mut keep: Vec<(f64, f64)> = Vec::new();
    let mut cur = lo;
    for [(x0, _), (x1, _)] in xs.as_chunks::<2>().0 {
        let (a, b) = (x0.max(lo), x1.min(hi));
        if b <= a {
            continue;
        }
        if a > cur {
            keep.push((cur, a));
        }
        cur = cur.max(b);
    }
    if hi > cur {
        keep.push((cur, hi));
    }
    if keep.len() == 1 && keep[0] == (lo, hi) {
        return vec![(xa, xb)];
    }
    if xa > xb {
        keep.reverse();
        keep.iter_mut().for_each(|p| *p = (p.1, p.0));
    }
    keep
}

/// The material of `region` the full rows would have faced that the
/// clipped pieces do not reach - what a hole leaves standing when the
/// strip beside it, or between it and its neighbour, is narrower than
/// the tool. `Some(area)` names the leftover; `None` is a clean face.
/// Measured as the tool's swept footprint, so a stepover wider than
/// the tool (the facer's own, visible band) is not counted.
pub(super) fn uncut(
    region: &Region,
    rows: &[(f64, f64, f64)],
    pieces: &[Vec<(f64, f64)>],
    r: f64,
) -> Result<Option<f64>, GeoError> {
    let seg = |y: f64, a: f64, b: f64| vec![Point2::new(a, y), Point2::new(b, y)];
    let full: Vec<Vec<Point2>> = rows.iter().map(|&(y, a, b)| seg(y, a, b)).collect();
    let kept: Vec<Vec<Point2>> = rows
        .iter()
        .zip(pieces)
        .flat_map(|(&(y, _, _), ps)| ps.iter().map(move |&(a, b)| seg(y, a, b)))
        .collect();
    let reached = region.intersect(&Region::sweep(&full, r)?)?;
    let left = if kept.is_empty() { reached } else { reached.difference(&Region::sweep(&kept, r)?)? };
    let area = left.area().abs();
    Ok((area > AREA_EPS).then_some(area))
}

#[cfg(test)]
mod law {
    use super::*;
    use crate::geo2d::Loop;

    fn sq(c: (f64, f64), h: f64) -> Loop {
        Loop::new(vec![
            Point2::new(c.0 - h, c.1 - h),
            Point2::new(c.0 + h, c.1 - h),
            Point2::new(c.0 + h, c.1 + h),
            Point2::new(c.0 - h, c.1 + h),
        ])
        .expect("loop")
    }

    #[test]
    fn voids_are_the_holes_less_their_islands() {
        let r = Region::new(vec![sq((0.0, 0.0), 20.0), sq((0.0, 0.0), 8.0), sq((0.0, 0.0), 2.0)]).expect("region");
        let v = voids(&r).expect("voids");
        assert!(v.contains(Point2::new(5.0, 0.0)), "the hole");
        assert!(!v.contains(Point2::new(0.0, 0.0)), "the island in it is material");
        assert!(!v.contains(Point2::new(15.0, 0.0)), "the outer's material");
        assert!((v.area().abs() - (256.0 - 16.0)).abs() < 1e-6);
    }

    #[test]
    fn lift_bridges_only_the_holes_that_material_at_the_plane_touches() {
        // #2437: two holes; the material at the cut plane rings the left
        // one only - the right one opens wholly below the plane.
        let (a, b) = (Point2::new(-10.0, 0.0), Point2::new(10.0, 0.0));
        let r = Region::new(vec![sq((0.0, 0.0), 20.0), sq((a.x, a.y), 2.0), sq((b.x, b.y), 2.0)]).expect("region");
        let ring = Region::new(vec![sq((a.x, a.y), 6.0), sq((a.x, a.y), 2.0)]).expect("ring");
        let got = forbidden(&r, FaceHoles::Lift, Some(&ring), 1.0).expect("bridged");
        assert!(got.holes.contains(a) && !got.holes.contains(b), "only the hole the material bounds");
        assert!(got.grown.contains(Point2::new(a.x + 2.5, 0.0)) && !got.grown.contains(b));
        // With no outline in hand every hole is bridged, as before.
        let all = forbidden(&r, FaceHoles::Lift, None, 1.0).expect("bridged");
        assert!(all.holes.contains(a) && all.holes.contains(b));
        // A boss standing in the right hole keeps that hole, whole.
        let boss = Region::new(vec![sq((b.x, b.y), 1.0)]).expect("boss");
        let got = forbidden(&r, FaceHoles::Lift, Some(&boss), 1.0).expect("bridged");
        assert!(got.holes.contains(Point2::new(b.x + 1.5, 0.0)) && !got.holes.contains(a));
        // Nothing at the plane: no hole of the face.
        let none = forbidden(&r, FaceHoles::Lift, Some(&Region::empty()), 1.0).expect("bridged");
        assert!(none.holes.is_empty() && none.grown.is_empty());
    }

    #[test]
    fn a_row_keeps_its_own_endpoints_when_nothing_is_forbidden_and_splits_at_a_grown_hole() {
        let forb = Region::new(vec![sq((0.0, 0.0), 5.0)]).expect("region");
        assert_eq!(pieces(&Region::empty(), 0.0, -30.0, 30.0), vec![(-30.0, 30.0)]);
        assert_eq!(pieces(&forb, 10.0, -30.0, 30.0), vec![(-30.0, 30.0)], "a row past the hole is whole");
        assert_eq!(pieces(&forb, 0.0, -30.0, 30.0), vec![(-30.0, -5.0), (5.0, 30.0)]);
        assert_eq!(pieces(&forb, 0.0, 30.0, -30.0), vec![(30.0, 5.0), (-5.0, -30.0)], "the return row runs the other way");
    }
}
