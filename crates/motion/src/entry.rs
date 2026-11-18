//! The entry family: how the cutter first engages material at a level,
//! and the engagement law that spaces the `Adaptive` rings.
//!
//! Provenance: standard CNC entry motions - straight plunge, linear
//! ramp, and helical interpolation - as described in public G-code
//! manuals and general machining handbooks; the laws are ours
//! (Clean-Room Protocol).
//!
//! The family specced at M2.1 for the 3-D ladder's level changes. M5.1
//! H lifts it to a crate seam so the planar pocket's ladder can engage
//! its levels the same way - the motions read only planar geometry and
//! the move chain, which both ladders share. `machining3d` keeps
//! re-exporting the vocabulary (`machining3d::Entry3D`), so no caller
//! path changed; the level-crossing legality arguments (the islands
//! superset: a chord legal at the deepest cut plane is legal at every
//! plane above) live where each ladder proves them.

use crate::cl::{lattice, MoveKind, PathBuilder};
use crate::geo2d::{Point2, Region};
use crate::geometry::Point3;
use crate::machining2d::chord_crosses_path;

/// How the cutter first engages material at a level.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Entry3D {
    #[default]
    /// Axial dive at the pass entry, the 2-D pocket motion.
    Plunge,
    /// One straight diagonal feed from the previous level's plane down
    /// to this one (first level: falls back to a plunge - there is no
    /// open floor to start a ramp from).
    Ramp,
    /// Helical interpolation: orbit while descending one stepdown (the
    /// widened arc law, see `cl.rs`). Air orbits above stock are never
    /// posted: the first level's dive is the surface itself.
    Helix,
    /// Circular ramp (M11.1): an arc in the vertical plane of the first
    /// cut, tangent to it at the bottom, from the plane the ramp starts
    /// at - certified by the ramp's chord law and posted as one G18/G19
    /// arc, or as a chord fan when its plane is oblique (see
    /// `entry_arc`). Opt-in: no existing operation chose it.
    Arc,
}

/// One helical dive between two planes: at least one full turn (split
/// into half-circle arcs so no move is a zero-sweep full circle), Z
/// descending linearly with the orbit. The builder must stand at `xy`
/// on plane `z0` (the caller's rapid to the dive point); an even
/// number of half turns ends back on `xy` at `z1` by construction -
/// each turn is two 180° arcs whose XY net is zero. Every ladder's
/// Helix arm ends here, at the `turns` `links::helix_turns` counts: the
/// entry's law (one turn per `2 rh` of drop, at least one -
/// the bore's slope stays inside the circle's reach), or a helix
/// transition's pitch (M11.2 C). The CL arc carries the Z change, so
/// consecutive half turns are one continuous helix, each starting where
/// the last ended.
/// `ccw` is the sense (M11.3 B): `true` - every pre-M11.3 helix - runs
/// over the +Y side, `false` over -Y; the circle, and so the tube the
/// dive room certified, is the same either way. M11.3: about any
/// `center` (the helix placed so it ends ON the pass start - the near
/// side `xy` is the start, the far side its mirror through the center). Even half turns end back on `xy` at
/// `z1`; the near points are `xy` exactly, the far ones lattice-snapped.
#[allow(clippy::too_many_arguments)]
pub(crate) fn helix_about(
    b: &mut PathBuilder,
    xy: Point2,
    center: Point2,
    z0: f64,
    z1: f64,
    turns: f64,
    feed: f64,
    ccw: bool,
) {
    let span = z0 - z1;
    if span <= 0.0 || !(turns.is_finite() && turns >= 1.0) {
        return;
    }
    let half_turns = ((turns * 2.0).ceil() as u32).max(2);
    let far = Point2::new(lattice(2.0 * center.x - xy.x), lattice(2.0 * center.y - xy.y));
    let near = Point2::new(lattice(xy.x), lattice(xy.y));
    for h in 0..half_turns {
        let frac = f64::from(h + 1) / f64::from(half_turns);
        let last = h + 1 == half_turns;
        // Land the final half-turn on exact `z1` so adjacent levels do not
        // inherit a 1-ULP ramp between `lattice(z1)` and `z1`.
        let tz = if last {
            z1
        } else if z0 >= z1 {
            lattice(z0 - span * frac).max(z1)
        } else {
            lattice(z0 - span * frac).min(z1)
        };
        // Alternating far/near point of the circle; even counts close on
        // xy at the last leg.
        let q = if h % 2 == 0 { far } else if last { xy } else { near };
        b.push_arc(MoveKind::Feed, Point3::new(q.x, q.y, tz), Some(feed), center, ccw);
    }
}

/// Is `c` a center point of `region` at least `margin` clear of every
/// boundary edge - the disc of that radius about it inside the region?
/// The test [`dive_room`] applies to its candidates, asked of one point.
pub(crate) fn disc_clears(region: &Region, c: Point2, margin: f64) -> bool {
    c.x.is_finite() && c.y.is_finite() && region.contains(c) && boundary_clears(region, c, margin)
}

/// The engagement law, in one line of trigonometry: a pass that bites
/// a wall face radially by `a` with a disc of radius `r` contacts it
/// over a half-angle `theta` with `a = r (1 - cos theta)` - the sagitta
/// of the contact arc. Inverting it gives the biggest ring step whose
/// contact stays under a chosen angle: `a = r (1 - cos theta_max)`.
/// Published trochoidal/HEM literature is all computations of this
/// same disc-against-wall geometry; this is our own reading of it,
/// pinned by `the_engagement_law_is_the_sagitta` (M2.1c, lifted to the
/// seam shared by both roughing ladders in M5.1 H).
#[must_use]
pub fn engagement_step(tool_r: f64, max_engagement_deg: f64) -> f64 {
    tool_r * (1.0 - max_engagement_deg.to_radians().cos())
}

/// Would a feed chord `a -> b` along this level's plane properly cross
/// the level's own planned paths? `chord_clears` proves the chord stays
/// inside the cutter-center area, but on a holed area (an island, a
/// boss) a chord from one family's ring to another's crosses every ring
/// in between, and `verify` reports each such crossing at the cut depth.
/// A chord that fails here is flown over instead: rapids, traverse,
/// plunge. This is the same law `waterline_passes` chains its rings by,
/// applied where the ladder joins one planned pass to the next.
pub(crate) fn chord_hits_plan(a: Point2, b: Point2, passes: &[Vec<Point2>]) -> bool {
    // The planned passes store their full vertex chains - a waterline
    // ring repeats its entry point at the end, so walking consecutive
    // pairs covers its closing leg too - so consecutive segments, never
    // a phantom close, is the exact set of this level's cut paths.
    passes.iter().any(|p| chord_crosses_path(a, b, p, false))
}

/// The dive's landing spot: the centroid of the largest triangle of
/// `center`'s own triangulation whose centre sits at least `margin`
/// clear of the boundary - the widest open floor the region offers
/// under a disc of that reach, not a search along the planned passes'
/// chords. A helix asks `radius + a micron's insurance` and takes the
/// FOUND point as the orbit's CENTER: `helix_turns_down` orbits the circle
/// whose NEAR side is the anchor, so the caller dives from `found -
/// (rh, 0)` and every point of the swept tube stands a radius inside
/// the certified disc. Asking the radius of the ANCHOR certified only
/// half the tube - the far side bit a radius into curved relief (M2.5
/// F's dome); asking TWO radii of the anchor over-refused the
/// straight channel the orbit proves (M8.2's slot). The center
/// convention is exact: the disc is the tube, and the anchor stands
/// inside the certified disc by the triangle inequality. An axial
/// plunge passes 0.0 and needs only the center itself.
///
/// M8.1 replaced `dive_point`'s chord-midpoint scan with this after
/// the scan's defect came to light: on an all-arcs plan (every
/// tessellated waterline ring) the consecutive-pair midpoints sit on
/// the ring lines, which the room (the center eroded again for the
/// widened disc) has withdrawn from - so the scan resolved `None`,
/// and the helix silently fell back to a wall plunge. A triangle of
/// the ear-clip is material by construction (never a boundary chord),
/// and by the superset law (a deeper level's center is a subset of
/// every plane above), room at the destination level is room across
/// the WHOLE depth the dive crosses.
///
/// The erosion is asked of the boundary, not of `Region::offset`:
/// eroding an annulus by more than its own width makes the boolean's
/// loops cross - the shrunk outer falls inside the grown hole - and
/// the output is a symmetric-difference ring, consistent under its
/// own `contains` yet material where the truth is empty (M8.1's boss
/// band helix landed there). A distance test cannot lie that way: it
/// measures the real clearance or says it is not there.
/// `None` when no triangle's centroid clears the margin, the region
/// is empty, or it is too dense to triangulate - a caller
/// substitution in every case, never a silent choice: the fallback
/// is said. (Public: the ladders use it, and the entry-family tests
/// pin its laws directly.)
pub fn dive_room(center: &Region, margin: f64) -> Option<Point2> {
    let mut best: Option<(f64, Point2)> = None;
    for t in center.triangles().ok()? {
        let (ax, ay) = (t[1].x - t[0].x, t[1].y - t[0].y);
        let (bx, by) = (t[2].x - t[0].x, t[2].y - t[0].y);
        let area = (ax * by - ay * bx).abs() * 0.5;
        if best.is_some_and(|(b, _)| b >= area) {
            continue;
        }
        let c = Point2::new(
            (t[0].x + t[1].x + t[2].x) / 3.0,
            (t[0].y + t[1].y + t[2].y) / 3.0,
        );
        if !center.contains(c) || !boundary_clears(center, c, margin) {
            continue;
        }
        best = Some((area, c));
    }
    best.map(|(_, d)| d)
}

/// Is `p` at least `d` clear of every edge of `region`'s boundaries -
/// walls and islands alike? Exact membership in the region eroded by
/// a disc of radius `d`, evaluated geometrically.
fn boundary_clears(region: &Region, p: Point2, d: f64) -> bool {
    if d <= 0.0 {
        return true;
    }
    let d2 = d * d;
    for l in region.loops() {
        let pts = l.points();
        let n = pts.len();
        for i in 0..n {
            let (a, b) = (pts[i], pts[(i + 1) % n]);
            let (ex, ey) = (b.x - a.x, b.y - a.y);
            let len2 = ex * ex + ey * ey;
            // The parameter of p's projection, clamped to the segment.
            let t = if len2 > 0.0 {
                (((p.x - a.x) * ex + (p.y - a.y) * ey) / len2).clamp(0.0, 1.0)
            } else {
                0.0
            };
            let (qx, qy) = (a.x + t * ex - p.x, a.y + t * ey - p.y);
            if qx * qx + qy * qy < d2 {
                return false;
            }
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::helix_about;
    use crate::cl::{MoveKind, PathBuilder};
    use crate::geo2d::Point2;
    use crate::geometry::Point3;
    use crate::verify::crossed_pairs;

    #[test]
    fn helix_about_exact_end_z_prevents_next_level_ulp_crossing() {
        // -0.30 - 0.15 == -0.44999999999999996 while latticeSnap == -0.45:
        // helix ending at lattice(z1) leaves a 1-ULP step into lvl.z that overlaps the next level's helix.
        let z0 = -0.30_f64;
        let z1 = z0 - 0.15;
        let z2 = z1 - 0.15;
        assert_ne!((z1 * 1e6).round() / 1e6, z1, "expected 1-ULP difference at -0.45");

        let start = Point2::new(10.5, 0.0);
        let center = Point2::new(12.0, 0.0);
        let mut b = PathBuilder::new(Point3::new(start.x, start.y, z0));
        helix_about(&mut b, start, center, z0, z1, 1.0, 500.0, true);
        // First level cut arc at z1
        b.push_arc(MoveKind::Feed, Point3::new(12.75, -1.30, z1), Some(500.0), center, true);
        // Next level helix descending from z1 to z2 at the same entry center
        b.push(MoveKind::Rapid, Point3::new(start.x, start.y, z1), None);
        helix_about(&mut b, start, center, z1, z2, 1.0, 500.0, true);
        let path = b.finish();
        let c = crossed_pairs(&path, 0.01);
        assert!(c.is_empty(), "ULP helix step caused false crossing: {c:?}");
    }
}
