//! The edge a no-pick 2D chamfer rides (#2435): the wall at the corner
//! depth, matched to the edge at the top plane.
//!
//! Provenance: the chamfer mill's cut law (machine-tool handbook
//! geometry, [`super::chamfer`]): the cone's tip stands ON the wall at
//! the corner depth `top - width * tan(angle)`, its generatrix on the
//! face. Where the wall is vertical the section at the top plane and
//! the section at the corner depth are one outline. Where the model
//! already carries a bevel (a modelled chamfer or round), the top
//! plane's outline is the bevel's TOP edge, standing inside the wall
//! by the bevel's width - a tip on it sits inside the part. The wall
//! the cone must touch is the section at the corner depth. Our own law
//! over this repo's sections; no vendor material consulted.

use crate::geo2d::{Loop, Point2, Region};
use crate::geometry::Point3;

use super::face::GenError;

/// Plan slack of the match between a top edge and its wall, mm: the
/// chord sag two tessellated sections of one rounded corner can
/// disagree by. A loose match only picks which outline to ride; the
/// ride itself is always the wall section, and the program sim holds
/// a 2D chamfer's tip to its wall (#2435) whatever was matched.
const MATCH_SLACK: f64 = 0.05;

/// Below this plan gap the wall IS the top edge (a vertical wall): the
/// top plane's own loop is ridden, unchanged, so every sharp-edged
/// part posts exactly what it did before #2435.
const SAME_EDGE: f64 = 1e-6;

/// The loops a no-pick 2D chamfer of `width` rides: for each loop of
/// `top` (the section at the top plane - the edges to break), the loop
/// of `corner` (the section at the corner depth - the wall the tip
/// touches) that every vertex of the top loop stands within `width` of,
/// in plan (one way only: a wall loop's mitred corner stands farther
/// than `width` from a bevel's top corner).
/// A vertical wall rides the top loop itself. A `corner` loop matched
/// to no top loop is a lower edge (a ledge, the plate under an island)
/// and is left alone: it is not an edge at the top plane.
///
/// # Errors
/// A top loop with no wall within `width` of it under it - a blind
/// pocket shallower than the chamfer's depth, an island whose wall
/// stands lower than that depth - refuses by name at that loop: the
/// tip would ride below the edge's floor, into the part.
pub fn ride_loops(top: &Region, corner: &Region, width: f64, corner_z: f64) -> Result<Region, GenError> {
    if !(width.is_finite() && width > 0.0 && corner_z.is_finite()) {
        return Err(GenError::Invalid("chamfer width must be positive".into()));
    }
    let reach = width + MATCH_SLACK;
    let mut out: Vec<Loop> = Vec::with_capacity(top.loops().len());
    for t in top.loops() {
        let wall = corner
            .loops()
            .iter()
            .filter(|c| c.is_ccw() == t.is_ccw())
            .map(|c| (c, gap(t.points(), c.points())))
            .filter(|(_, g)| *g <= reach)
            .min_by(|a, b| a.1.total_cmp(&b.1));
        match wall {
            Some((_, g)) if g <= SAME_EDGE => out.push(t.clone()),
            Some((c, _)) => out.push(c.clone()),
            None => {
                let at = t.points().first().copied().unwrap_or(Point2::new(0.0, 0.0));
                return Err(GenError::At {
                    msg: format!(
                        "the edge near X{:.3} Y{:.3} has no wall under it down to the chamfer's depth Z{corner_z:.3} - a {width} mm chamfer there would ride into the part; lower the width or pick the edges to break",
                        at.x, at.y
                    ),
                    at: Point3::new(at.x, at.y, corner_z),
                });
            }
        }
    }
    Region::new(out).map_err(GenError::Geo)
}

/// The farthest any vertex of `from` stands from the closed polyline
/// `to`, in plan (the one-sided Hausdorff gap over the vertices).
/// `+inf` when either side has no points.
fn gap(from: &[Point2], to: &[Point2]) -> f64 {
    if from.is_empty() || to.is_empty() {
        return f64::INFINITY;
    }
    from.iter()
        .map(|&p| near(p, to))
        .fold(0.0, f64::max)
}

/// Plan distance from `p` to the closed polyline `ring`.
fn near(p: Point2, ring: &[Point2]) -> f64 {
    let n = ring.len();
    (0..n)
        .map(|i| seg(p, ring[i], ring[(i + 1) % n]))
        .fold(f64::INFINITY, f64::min)
}

fn seg(p: Point2, a: Point2, b: Point2) -> f64 {
    let (dx, dy) = (b.x - a.x, b.y - a.y);
    let l2 = dx * dx + dy * dy;
    let t = if l2 > 1e-24 { (((p.x - a.x) * dx + (p.y - a.y) * dy) / l2).clamp(0.0, 1.0) } else { 0.0 };
    let (qx, qy) = (a.x + t * dx - p.x, a.y + t * dy - p.y);
    (qx * qx + qy * qy).sqrt()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rect(x0: f64, y0: f64, x1: f64, y1: f64) -> Loop {
        Loop::new(vec![Point2::new(x0, y0), Point2::new(x1, y0), Point2::new(x1, y1), Point2::new(x0, y1)]).expect("rect")
    }

    #[test]
    fn a_vertical_wall_rides_its_own_top_loop() {
        let top = Region::new(vec![rect(-20.0, -10.0, 20.0, 10.0)]).expect("top");
        let got = ride_loops(&top, &top.clone(), 1.0, 9.0).expect("ride");
        assert_eq!(got, top);
    }

    #[test]
    fn a_modelled_bevel_rides_the_wall_under_it() {
        // The bevel's top edge 1 mm inside the wall.
        let top = Region::new(vec![rect(-19.0, -9.0, 19.0, 9.0)]).expect("top");
        let wall = Region::new(vec![rect(-20.0, -10.0, 20.0, 10.0)]).expect("wall");
        let got = ride_loops(&top, &wall, 1.0, 9.0).expect("ride");
        assert_eq!(got, wall);
    }

    #[test]
    fn a_wall_farther_than_the_width_refuses_by_name() {
        let top = Region::new(vec![rect(-19.0, -9.0, 19.0, 9.0)]).expect("top");
        let plate = Region::new(vec![rect(-50.0, -30.0, 50.0, 30.0)]).expect("plate");
        let err = ride_loops(&top, &plate, 1.0, 9.0).expect_err("no wall within the width");
        assert!(err.to_string().contains("no wall under it"), "{err}");
    }

    #[test]
    fn a_lower_edge_matched_to_no_top_loop_is_left_alone() {
        let top = Region::new(vec![rect(-20.0, -10.0, 20.0, 10.0)]).expect("top");
        // A second, lower boss beside the island: its wall is in the
        // corner section, but no edge of the top plane stands over it.
        let corner = Region::new(vec![rect(-20.0, -10.0, 20.0, 10.0), rect(30.0, -10.0, 40.0, 10.0)]).expect("corner");
        let got = ride_loops(&top, &corner, 1.0, 9.0).expect("ride");
        assert_eq!(got, top);
    }
}
