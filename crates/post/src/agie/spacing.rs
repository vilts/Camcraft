//! Keep the rough passes of unrelated faces of a reentrant punch apart, by
//! the distance between disjoint segment capsules (planar segment geometry).
use super::contour::{Point, cross, dot, vector};

fn point_clears_segment(p: Point, a: Point, b: Point, clearance2: i128) -> bool {
    let edge = vector(a, b);
    let from_a = vector(a, p);
    let length2 = dot(edge, edge);
    let projection = dot(edge, from_a);
    if projection <= 0 {
        dot(from_a, from_a) > clearance2
    } else if projection >= length2 {
        let from_b = vector(b, p);
        dot(from_b, from_b) > clearance2
    } else {
        let side = cross(a, b, p);
        side * side > clearance2 * length2
    }
}

pub(super) fn check_feature_spacing(ring: &[Point], clearance: i64) -> Result<(), String> {
    let n = ring.len();
    let clearance2 = i128::from(clearance).pow(2);
    for (i, &a) in ring.iter().enumerate() {
        let b = ring[(i + 1) % n];
        let edge = vector(a, b);
        // The two joins on a short face could meet before full rough offset.
        if dot(edge, edge) <= clearance2 {
            return Err(
                "The rough corner joins cannot be separated on a short reentrant profile segment"
                    .into(),
            );
        }
        for j in i + 2..n {
            if i == 0 && j + 1 == n {
                continue;
            }
            let (c, d) = (ring[j], ring[(j + 1) % n]);
            // Nominal segments are disjoint. The minimum distance between two
            // disjoint segments occurs at an endpoint of at least one segment.
            if !point_clears_segment(a, c, d, clearance2)
                || !point_clears_segment(b, c, d, clearance2)
                || !point_clears_segment(c, a, b, clearance2)
                || !point_clears_segment(d, a, b, clearance2)
            {
                return Err(
                    "The rough pass cannot be separated from a nearby nonadjacent profile segment"
                        .into(),
                );
            }
        }
    }
    Ok(())
}
