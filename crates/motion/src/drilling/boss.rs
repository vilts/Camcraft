//! Boss recognition from the facet mesh (#985): the vertical convex cylinders an external thread is milled on.
//!
//! Provenance: the hole recogniser's own weld, circle fit and rim-span
//! law ([`super::recognize`], which names its public sources), read on
//! the other side of the wall - a boss is a closed round wall whose
//! facets face AWAY from its axis. The reading is ours, specced from
//! general machining knowledge (an external thread stands on a
//! cylinder that rises from a shoulder); no vendor feature-recognition
//! strategy consulted.
//!
//! A boss is reported in the hole list's own shape so the thread ride
//! and the file keep one feature record: `radius` is the wall's own
//! (the thread's MAJOR radius - CAD draws an external thread at its
//! nominal diameter), `entry_z` the wall's top and `floor_z` its foot
//! (the shoulder it rises from, or the part's bottom on a plain stud).
//! Unlike holes, walls stepped along one axis are NOT folded: a
//! stepped shaft is two bosses of two diameters, and threading "the
//! narrow one down to the wide one's foot" would be a crash.

use super::cylinder::cmp64;
use super::hole::Hole;
use super::recognize::analyze_axis;

use crate::cl::lattice;
use crate::geo2d::Point2;
use crate::geometry::Triangle;

/// Find the bosses in a triangle mesh: closed, round, vertical walls
/// whose facets face away from their own axis (so drilled holes fall
/// out), snapped onto the lattice so re-detection of an unmoved model
/// is bit-stable. Sorted by Y, X, radius, then top down. Degenerate or
/// empty input yields an empty list - never a panic, never a hang.
#[must_use]
pub fn recognize_bosses(tris: &[Triangle]) -> Vec<Hole> {
    let mesh_min = tris
        .iter()
        .filter(|t| t.a.is_finite() && t.b.is_finite() && t.c.is_finite())
        .flat_map(|t| [t.a.z, t.b.z, t.c.z])
        .fold(f64::INFINITY, f64::min);
    if !mesh_min.is_finite() {
        return Vec::new();
    }
    let mut bosses: Vec<Hole> = analyze_axis(tris, 2, 0, 1, mesh_min, true)
        .iter()
        .map(|f| Hole {
            xy: Point2::new(lattice(f.cu), lattice(f.cv)),
            radius: lattice(f.rad),
            entry_z: lattice(f.smax),
            floor_z: lattice(f.smin),
            // A boss has no exit and no fold: both words are the hole's.
            through: false,
            stepped: false,
        })
        .filter(Hole::is_usable)
        .collect();
    bosses.sort_by(|a, b| {
        cmp64(a.xy.y, b.xy.y)
            .then(cmp64(a.xy.x, b.xy.x))
            .then(cmp64(a.radius, b.radius))
            .then(cmp64(b.entry_z, a.entry_z))
    });
    bosses
}

#[cfg(test)]
mod tests {
    use super::super::tests::{circle, circle_cw, ring_walls};
    use super::*;
    use crate::geometry::Point3;

    fn rect(h: f64) -> Vec<(f64, f64)> {
        vec![(-h, -h), (h, -h), (h, h), (-h, h)]
    }

    #[test]
    fn a_boss_on_a_plate_is_found_with_its_shoulder_and_top() {
        let mut m = ring_walls(&rect(20.0), 0.0, 10.0);
        m.extend(ring_walls(&circle(3.0, -2.0, 4.0, 48), 10.0, 22.0));
        let b = recognize_bosses(&m);
        assert_eq!(b.len(), 1, "{b:?}");
        assert!((b[0].xy.x - 3.0).abs() < 1e-6 && (b[0].xy.y + 2.0).abs() < 1e-6);
        assert!((b[0].radius - 4.0).abs() < 1e-6);
        assert_eq!((b[0].entry_z, b[0].floor_z), (22.0, 10.0));
        assert!(!b[0].through && !b[0].stepped);
        assert!(super::super::recognize(&m).is_empty(), "a boss is still no hole");
    }

    #[test]
    fn holes_straight_walls_and_half_rounds_are_not_bosses() {
        // A plate with a drilled hole: the hole's wall faces its axis.
        let mut m = ring_walls(&rect(20.0), 0.0, 10.0);
        m.extend(ring_walls(&circle_cw(0.0, 0.0, 3.0, 32), 0.0, 10.0));
        assert!(recognize_bosses(&m).is_empty(), "{:?}", recognize_bosses(&m));
        // A half-round rib: a convex arc that closes no circle.
        let half: Vec<(f64, f64)> = (0..=16)
            .map(|i| {
                let a = std::f64::consts::PI * f64::from(i) / 16.0;
                (5.0 * a.cos(), 5.0 * a.sin())
            })
            .collect();
        assert!(recognize_bosses(&ring_walls(&half, 0.0, 10.0)).is_empty());
    }

    #[test]
    fn a_stepped_shaft_is_two_bosses_never_one_fold() {
        let mut m = ring_walls(&circle(0.0, 0.0, 8.0, 64), 0.0, 10.0);
        m.extend(ring_walls(&circle(0.0, 0.0, 5.0, 64), 10.0, 25.0));
        let b = recognize_bosses(&m);
        assert_eq!(b.len(), 2, "{b:?}");
        assert!((b[0].radius - 5.0).abs() < 1e-6 && (b[0].floor_z - 10.0).abs() < 1e-9, "{b:?}");
        assert!((b[1].radius - 8.0).abs() < 1e-6 && (b[1].entry_z - 10.0).abs() < 1e-9, "{b:?}");
    }

    #[test]
    fn degenerate_input_yields_nothing_and_never_panics() {
        assert!(recognize_bosses(&[]).is_empty());
        let nan = Point3::new(f64::NAN, 0.0, 0.0);
        let t = Triangle::new(nan, Point3::new(1.0, 0.0, 0.0), Point3::new(0.0, 1.0, 0.0));
        assert!(recognize_bosses(&[t]).is_empty());
        let flat = Triangle::new(Point3::new(0.0, 0.0, 0.0), Point3::new(0.0, 0.0, 0.0), Point3::new(0.0, 0.0, 0.0));
        assert!(recognize_bosses(&[flat; 12]).is_empty());
    }
}
