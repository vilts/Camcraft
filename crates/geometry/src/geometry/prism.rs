//! Verify a vertical extrusion by its oriented boundary and cap coverage.
//! Provenance: elementary polyhedral geometry and the project's planar region algebra.

use super::{Point3, Triangle, slice_bodies};
use crate::geo2d::{Loop, Point2, Region, SCALE, quantize};

struct Side {
    a: Point2,
    tangent: Point2,
    length: f64,
}

impl Side {
    fn new(a: Point2, b: Point2) -> Option<Self> {
        let (dx, dy) = (b.x - a.x, b.y - a.y);
        let length = dx.hypot(dy);
        (length.is_finite() && length > 0.0).then(|| Self {
            a,
            tangent: Point2::new(dx / length, dy / length),
            length,
        })
    }

    fn contains_wall(&self, points: &[Point3; 3], normal: Point3) -> bool {
        // A normalized region has material on the left, also around a hole.
        // XY projection may round each corner by half a lattice unit per axis.
        let tolerance = 1.0 / SCALE;
        let (tx, ty) = (self.tangent.x, self.tangent.y);
        normal.x * ty - normal.y * tx > 0.0
            && points.iter().all(|p| {
                let (dx, dy) = (p.x - self.a.x, p.y - self.a.y);
                let s = dx * tx + dy * ty;
                (dx * ty - dy * tx).abs() <= tolerance
                    && s >= -tolerance
                    && s <= self.length + tolerance
            })
    }
}

// Twice the oriented area in integer lattice units. Comparing the sum of
// triangle areas with the area of their union also rules out overlapping caps.
fn lattice_area2(ring: &Loop) -> Option<i128> {
    let points = ring.points();
    let mut sum = 0_i128;
    for (a, b) in points.iter().zip(points.iter().cycle().skip(1)) {
        let q = |x| quantize(x).ok().map(i128::from);
        sum = sum.checked_add(q(a.x)? * q(b.y)? - q(b.x)? * q(a.y)?)?;
    }
    Some(sum)
}

fn cap_region(triangles: Vec<Loop>) -> Result<Region, &'static str> {
    let covered = triangles
        .iter()
        .try_fold(0_i128, |s, t| {
            let a = lattice_area2(t)?;
            if a <= 0 {
                return None;
            }
            s.checked_add(a)
        })
        .ok_or("degenerate cap triangle")?;
    let region = Region::union_all(triangles).map_err(|_| "invalid cap union")?;
    let area = region
        .loops()
        .iter()
        .try_fold(0_i128, |s, ring| s.checked_add(lattice_area2(ring)?));
    if covered == 0 || area != Some(covered) {
        return Err("empty or overlapping cap triangles");
    }
    Ok(region)
}

fn same_footprint(a: &Region, b: &Region) -> bool {
    a.difference(b).is_ok_and(|r| r.is_empty()) && b.difference(a).is_ok_and(|r| r.is_empty())
}

fn boundary_sides(region: &Region) -> Result<Vec<Side>, &'static str> {
    let mut sides = Vec::new();
    for ring in region.loops() {
        // A section or cap union can split a straight wall at a triangle
        // diagonal. Compare wall facets with maximal collinear boundary faces.
        let pts = ring.points();
        let coords = pts
            .iter()
            .map(|p| {
                Some([
                    i128::from(quantize(p.x).ok()?),
                    i128::from(quantize(p.y).ok()?),
                ])
            })
            .collect::<Option<Vec<_>>>()
            .ok_or("invalid footprint coordinate")?;
        let boundary: Vec<_> = pts
            .iter()
            .enumerate()
            .filter_map(|(i, &p)| {
                let n = coords.len();
                let (a, b, c) = (coords[(i + n - 1) % n], coords[i], coords[(i + 1) % n]);
                ((b[0] - a[0]) * (c[1] - b[1]) != (b[1] - a[1]) * (c[0] - b[0])).then_some(p)
            })
            .collect();
        for (&a, &b) in boundary.iter().zip(boundary.iter().cycle().skip(1)) {
            let Some(side) = Side::new(a, b) else {
                return Err("degenerate side");
            };
            sides.push(side);
        }
    }
    Ok(sides)
}

/// Return the actual footprint of a full-height vertical extrusion. Check
/// every facet, including internal surfaces that projection and slicing miss.
pub fn closed_vertical_prism(
    facets: &[Triangle],
    bottom: f64,
    top: f64,
) -> Result<Region, &'static str> {
    if facets.is_empty()
        || facets.len() > 100_000
        || !bottom.is_finite()
        || !top.is_finite()
        || bottom >= top
    {
        return Err("invalid prism bounds or too many facets");
    }
    let mut caps = [Vec::new(), Vec::new()];
    let mut walls = Vec::new();
    for facet in facets {
        let points = [facet.a, facet.b, facet.c];
        if points
            .iter()
            .any(|p| !p.is_finite() || p.z < bottom || p.z > top)
        {
            return Err("facet outside the stock planes");
        }
        let normal = facet.normal();
        let at_top = points.iter().all(|p| p.z == top);
        if at_top || points.iter().all(|p| p.z == bottom) {
            if (at_top && normal.z <= 0.0) || (!at_top && normal.z >= 0.0) {
                return Err("cap faces inward");
            }
            let mut xy = points.map(|p| Point2::new(p.x, p.y));
            if !at_top {
                xy.reverse();
            }
            caps[usize::from(at_top)]
                .push(Loop::new(xy.to_vec()).map_err(|_| "degenerate cap triangle")?);
        } else if normal.z.abs() <= 1e-12 {
            walls.push((points, normal));
        } else {
            return Err("wall is not vertical");
        }
    }
    let [bottom_cap, top_cap] = caps;
    let footprint = cap_region(top_cap)?;
    if !same_footprint(&footprint, &cap_region(bottom_cap)?) {
        return Err("the top and bottom cap footprints differ");
    }
    let sides = boundary_sides(&footprint)?;
    if walls.len().saturating_mul(sides.len()) > 8_000_000 {
        return Err("too many boundary faces");
    }
    for (points, normal) in walls {
        if !sides.iter().any(|s| s.contains_wall(&points, normal)) {
            return Err("wall does not lie on an oriented boundary face");
        }
    }
    if !slice_bodies::closed_solids(facets).is_some_and(|solids| solids.len() == 1) {
        return Err("the facets do not form one closed solid");
    }
    Ok(footprint)
}

/// Test a single convex ring, preserving the original convex-prism contract.
pub fn is_closed_convex_prism(facets: &[Triangle], ring: &Loop, bottom: f64, top: f64) -> bool {
    let pts = ring.points();
    let sense = ring.area().signum();
    if pts.len() < 3
        || (0..pts.len()).any(|i| {
            let (a, b, c) = (pts[i], pts[(i + 1) % pts.len()], pts[(i + 2) % pts.len()]);
            ((b.x - a.x) * (c.y - b.y) - (b.y - a.y) * (c.x - b.x)) * sense < 0.0
        })
    {
        return false;
    }
    Region::new(vec![ring.clone()]).is_ok_and(|expected| {
        closed_vertical_prism(facets, bottom, top)
            .is_ok_and(|actual| same_footprint(&actual, &expected))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::{Point3, cube};

    #[test]
    fn an_outer_projection_does_not_prove_the_solid_is_a_prism() {
        let min = Point3::new(-20.0, -10.0, 0.0);
        let max = Point3::new(20.0, 10.0, 20.0);
        let block = cube(min, max);
        let ring = Loop::new(vec![
            Point2::new(min.x, min.y),
            Point2::new(max.x, min.y),
            Point2::new(max.x, max.y),
            Point2::new(min.x, max.y),
        ])
        .unwrap();
        let region = Region::new(vec![ring]).unwrap();
        assert!(same_footprint(
            &closed_vertical_prism(&block.facets, min.z, max.z).unwrap(),
            &region
        ));
        let mut shells = block.facets.to_vec();
        let cavity = cube(Point3::new(-1.0, -1.0, 4.0), Point3::new(1.0, 1.0, 6.0));
        shells.extend(cavity.facets.iter().map(|t| Triangle::new(t.a, t.c, t.b)));
        assert!(closed_vertical_prism(&shells, min.z, max.z).is_err());
        assert!(closed_vertical_prism(&block.facets[1..], min.z, max.z).is_err());
    }

    #[test]
    fn a_through_opening_remains_a_hole_in_the_verified_footprint() {
        // A frame with two rectangular rings. Outer walls face outward and
        // inner walls face the opening. Both caps exclude the central void.
        let outer = [[-10., -10.], [10., -10.], [10., 10.], [-10., 10.]];
        let inner = [[-3., -3.], [3., -3.], [3., 3.], [-3., 3.]];
        let v = |[x, y]: [f64; 2], z| Point3::new(x, y, z);
        let mut facets = Vec::new();
        let mut quad = |a, b, c, d| {
            facets.push(Triangle::new(a, b, c));
            facets.push(Triangle::new(a, c, d));
        };
        for i in 0..4 {
            let j = (i + 1) % 4;
            let (a, b, c, d) = (outer[i], outer[j], inner[j], inner[i]);
            quad(v(a, 20.), v(b, 20.), v(c, 20.), v(d, 20.));
            quad(v(d, 0.), v(c, 0.), v(b, 0.), v(a, 0.));
            quad(v(a, 0.), v(b, 0.), v(b, 20.), v(a, 20.));
            quad(v(c, 0.), v(d, 0.), v(d, 20.), v(c, 20.));
        }
        let region = closed_vertical_prism(&facets, 0., 20.).unwrap();
        assert_eq!(region.loops().len(), 2);
        assert_eq!(region.area(), 364.);
        assert!(!region.contains(Point2::new(0., 0.)));
        assert!(region.contains(Point2::new(5., 0.)));
    }
}
