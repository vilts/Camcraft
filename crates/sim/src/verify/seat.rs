//! The part's undersides and the floor a mark must leave on them
//! (#1595, review of PR #1615): an Engrave or a Trace may lower the
//! part's surface under its cutter, but never to within [`MARK_FLOOR`]
//! of the surface the material there rests on - the part's underside,
//! or the roof of a void inside it. Past that the cut is a
//! breakthrough, whatever depth the operation states.
//!
//! Provenance: our own law (`docs/decisions/1595-engrave-mark-law.md`)
//! over this module's closed-shell reading - a facet is an underside
//! when the solid stands just above it and air just below, by the same
//! vertical-ray parity the solid law uses ([`super::solid_index`]). The
//! winding is never trusted. No vendor material consulted.

use super::solid_index::SolidIndex;
use crate::geometry::{Point3, Triangle};

/// The least thickness of the part a mark leaves standing on its
/// underside, mm: under it the cutter has broken through in every way
/// that matters on a machine (a web the tool's runout and the sheet's
/// flatness tear open). One number for the operation's law and the
/// program sim's credit alike.
pub const MARK_FLOOR: f64 = 0.2;

/// How far off a facet the solid is asked "material or air?", mm.
const PROBE: f64 = 1e-6;

/// The facets of `facets` that are undersides: the solid stands just
/// above the facet and air just below it. A facet between two bodies of
/// one part (material on both sides) is not one, and neither is a
/// vertical wall. `None` when the mesh cannot be read (a non-finite
/// vertex): the caller fails closed. A facet whose column cannot be
/// read is kept as an underside - a floor too many, never one too few.
#[must_use]
pub fn undersides(facets: &[Triangle]) -> Option<Vec<Triangle>> {
    let mut index = SolidIndex::build(facets)?;
    let mut out = Vec::new();
    for t in facets {
        let det = (t.b.x - t.a.x) * (t.c.y - t.a.y) - (t.b.y - t.a.y) * (t.c.x - t.a.x);
        if det == 0.0 || !det.is_finite() {
            continue; // a vertical wall has no floor to leave
        }
        // The facet's own plane over a point a hair aside (the retries).
        let (ux, uy, uz) = (t.b.x - t.a.x, t.b.y - t.a.y, t.b.z - t.a.z);
        let (vx, vy, vz) = (t.c.x - t.a.x, t.c.y - t.a.y, t.c.z - t.a.z);
        let (nx, ny) = (uy * vz - uz * vy, uz * vx - ux * vz);
        let c = Point3::new((t.a.x + t.b.x + t.c.x) / 3.0, (t.a.y + t.b.y + t.c.y) / 3.0, (t.a.z + t.b.z + t.c.z) / 3.0);
        let mid = |q: Point3| Point3::new(0.5 * (c.x + q.x), 0.5 * (c.y + q.y), 0.5 * (c.z + q.z));
        // Read at the centre and half way to each corner: a facet that
        // is an underside at any of them is one (a facet of a closed
        // shell reads the same at all four).
        let mut under_at = |s: Point3| {
            [(0.0, 0.0), (1.37e-7, 0.71e-7), (-0.93e-7, 1.73e-7)]
                .into_iter()
                .find_map(|(dx, dy)| {
                    let z = s.z - (nx * dx + ny * dy) / det;
                    let col = index.column(s.x + dx, s.y + dy)?;
                    // Parity of the crossings above a height: odd is inside.
                    let inside = |h: f64| col.iter().filter(|(cz, _)| *cz > h).count() % 2 == 1;
                    z.is_finite().then(|| inside(z + PROBE) && !inside(z - PROBE))
                })
                .unwrap_or(true)
        };
        if [c, mid(t.a), mid(t.b), mid(t.c)].into_iter().any(&mut under_at) {
            out.push(*t);
        }
    }
    Some(out)
}

/// The floor a mark must leave, as a solid the cutter may not enter:
/// every underside of `facets` raised into a closed prism `floor` high.
/// Read with the solid law, a cutting edge that enters it stands less
/// than `floor` above the part's underside (or a void's roof) - through
/// the part, or all but. Empty for a `floor` that is not positive and
/// finite; `None` when the mesh cannot be read.
#[must_use]
pub fn seat_skin(facets: &[Triangle], floor: f64) -> Option<Vec<Triangle>> {
    if !(floor.is_finite() && floor > 0.0) {
        return Some(Vec::new());
    }
    let under = undersides(facets)?;
    let up = |p: Point3| Point3::new(p.x, p.y, p.z + floor);
    let mut out = Vec::with_capacity(under.len() * 8);
    for t in &under {
        let (a, b, c) = (t.a, t.b, t.c);
        let (a1, b1, c1) = (up(a), up(b), up(c));
        out.push(Triangle::new(a, b, c));
        out.push(Triangle::new(a1, b1, c1));
        for (p, q, p1, q1) in [(a, b, a1, b1), (b, c, b1, c1), (c, a, c1, a1)] {
            out.push(Triangle::new(p, q, q1));
            out.push(Triangle::new(p, q1, p1));
        }
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::cube;

    fn p(x: f64, y: f64, z: f64) -> Point3 {
        Point3::new(x, y, z)
    }

    fn zs(f: &[Triangle]) -> Vec<f64> {
        let mut z: Vec<f64> = f.iter().map(|t| (t.a.z + t.b.z + t.c.z) / 3.0).collect();
        z.sort_by(f64::total_cmp);
        z
    }

    #[test]
    fn a_blocks_only_underside_is_its_bottom_face_whatever_the_winding() {
        let block = cube(p(0.0, 0.0, 0.0), p(20.0, 20.0, 10.0)).facets.to_vec();
        assert_eq!(zs(&undersides(&block).expect("read")), vec![0.0, 0.0]);
        // Every facet wound the other way: the same two facets.
        let flipped: Vec<Triangle> = block.iter().map(|t| Triangle::new(t.a, t.c, t.b)).collect();
        assert_eq!(zs(&undersides(&flipped).expect("read")), vec![0.0, 0.0]);
    }

    #[test]
    fn a_face_between_two_bodies_of_the_part_is_not_an_underside() {
        // A boss standing on a block: its bottom face lies on the
        // block's top, material on both sides.
        let mut part = cube(p(0.0, 0.0, 0.0), p(40.0, 40.0, 10.0)).facets.to_vec();
        part.extend(cube(p(30.0, 0.0, 10.0), p(40.0, 40.0, 15.0)).facets.iter().copied());
        assert_eq!(zs(&undersides(&part).expect("read")), vec![0.0, 0.0]);
    }

    #[test]
    fn the_roof_of_a_void_is_an_underside() {
        // A bridge: two feet and a deck over a tunnel along Y.
        let mut part = cube(p(0.0, 0.0, 0.0), p(10.0, 20.0, 6.0)).facets.to_vec();
        part.extend(cube(p(30.0, 0.0, 0.0), p(40.0, 20.0, 6.0)).facets.iter().copied());
        part.extend(cube(p(0.0, 0.0, 6.0), p(40.0, 20.0, 8.0)).facets.iter().copied());
        let u = undersides(&part).expect("read");
        assert!(u.iter().any(|t| t.a.z == 6.0 && t.b.z == 6.0 && t.c.z == 6.0), "the deck's underside over the tunnel: {:?}", zs(&u));
        assert!(u.iter().all(|t| t.a.z <= 6.0), "no top face: {:?}", zs(&u));
    }

    #[test]
    fn the_skin_is_a_closed_prism_per_underside_and_nothing_for_no_floor() {
        let block = cube(p(0.0, 0.0, 0.0), p(20.0, 20.0, 10.0)).facets.to_vec();
        let skin = seat_skin(&block, 0.2).expect("read");
        assert_eq!(skin.len(), 16);
        let top = skin.iter().flat_map(|t| [t.a.z, t.b.z, t.c.z]).fold(f64::NEG_INFINITY, f64::max);
        assert_eq!(top, 0.2);
        for none in [0.0, -1.0, f64::NAN, f64::INFINITY] {
            assert!(seat_skin(&block, none).expect("read").is_empty(), "{none}");
        }
    }

    #[test]
    fn a_mesh_that_cannot_be_read_is_refused() {
        let mut block = cube(p(0.0, 0.0, 0.0), p(20.0, 20.0, 10.0)).facets.to_vec();
        block[0] = Triangle::new(p(f64::NAN, 0.0, 0.0), block[0].b, block[0].c);
        assert!(undersides(&block).is_none() && seat_skin(&block, 0.2).is_none());
    }
}
