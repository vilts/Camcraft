//! Provenance: original closed-solid fixture from elementary drilled-block geometry, for #2601.

use super::tests::{circle, circle_cw, ring_walls};
use super::{recognize, recognize_cross};
use crate::geometry::{Point3, Triangle};

/// Reduced form of the reported D50 bar: a closed square prism with
/// opposite D5 holes, each 8 mm deep, across a 50 mm material span.
/// The two floor discs leave 34 mm of solid material between the holes.
fn opposite_blind_block() -> Vec<Triangle> {
    let n = 32;
    let inner = circle(0.0, 0.0, 2.5, n);
    let outer: Vec<_> = inner
        .iter()
        .map(|&(x, y)| {
            let scale = 5.0 / x.abs().max(y.abs());
            (x * scale, y * scale)
        })
        .collect();
    let mut mesh = ring_walls(&outer, -25.0, 25.0);
    let bore = circle_cw(0.0, 0.0, 2.5, n);
    mesh.extend(ring_walls(&bore, 17.0, 25.0));
    mesh.extend(ring_walls(&bore, -25.0, -17.0));
    let at = |p: (f64, f64), z| Point3::new(p.0, p.1, z);
    for i in 0..n {
        let j = (i + 1) % n;
        for (mouth, floor, reverse) in [(25.0, 17.0, false), (-25.0, -17.0, true)] {
            let triangles = [
                [
                    at(inner[i], mouth),
                    at(outer[i], mouth),
                    at(outer[j], mouth),
                ],
                [
                    at(inner[i], mouth),
                    at(outer[j], mouth),
                    at(inner[j], mouth),
                ],
                [
                    Point3::new(0.0, 0.0, floor),
                    at(inner[i], floor),
                    at(inner[j], floor),
                ],
            ];
            for [a, b, c] in triangles {
                mesh.push(if reverse {
                    Triangle::new(a, c, b)
                } else {
                    Triangle::new(a, b, c)
                });
            }
        }
    }
    mesh
}

fn radial_block() -> Vec<Triangle> {
    // Proper rotation: old Z becomes X; the outward winding stays intact.
    let rotate = |p: Point3| Point3::new(p.z, p.x, p.y);
    opposite_blind_block()
        .into_iter()
        .map(|t| Triangle::new(rotate(t.a), rotate(t.b), rotate(t.c)))
        .collect()
}

#[test]
fn opposite_blind_z_walls_keep_their_separate_depths() {
    let found = recognize(&opposite_blind_block());
    assert_eq!(
        found.len(),
        1,
        "only the positive mouth is a downward Z hole: {found:?}"
    );
    assert!(!found[0].through);
    let reverse = recognize_cross(&opposite_blind_block());
    assert_eq!(reverse.len(), 1, "{reverse:?}");
    assert_eq!(reverse[0].origin.z, -25.0);
    assert_eq!(reverse[0].dir.z, 1.0);
    assert_eq!(reverse[0].floor_d, -8.0);
    assert!(!reverse[0].through);
    for hole in found {
        assert_eq!(hole.entry_z - hole.floor_z, 8.0, "{hole:?}");
        assert_eq!(hole.radius, 2.5);
    }
}

#[test]
fn opposite_blind_radial_walls_keep_their_separate_depths() {
    let found = recognize_cross(&radial_block());
    assert_eq!(
        found.len(),
        2,
        "separate floors must not create a through hole: {found:?}"
    );
    for hole in found {
        assert_eq!(hole.floor_d, -8.0, "{hole:?}");
        assert_eq!(hole.radius, 2.5);
    }
}

#[test]
fn opposite_blind_radial_holes_enter_from_their_own_mouths() {
    let found = recognize_cross(&radial_block());
    for side in [-1.0, 1.0] {
        let origin = Point3::new(side * 25.0, 0.0, 0.0);
        let hole = found.iter().find(|h| h.origin == origin);
        assert!(hole.is_some(), "missing the mouth at {origin:?}: {found:?}");
        let hole = hole.unwrap();
        assert_eq!(hole.dir, Point3::new(-side, 0.0, 0.0), "{hole:?}");
        assert!(!hole.through, "the floor makes this hole blind: {hole:?}");
        assert_eq!(hole.entry_d, 0.0);
        assert_eq!(hole.floor_d, -8.0);
    }
}

#[test]
fn six_blind_holes_on_a_d50_bar_keep_their_depths_and_mouths() {
    use super::hole::CrossHole;
    let holes: Vec<_> = [
        (25.0, 0.0, -20.0),
        (-25.0, 0.0, -20.0),
        (0.0, 25.0, 0.0),
        (0.0, -25.0, 0.0),
        (25.0, 0.0, 20.0),
        (0.0, 25.0, 35.0),
    ]
    .into_iter()
    .map(|(x, y, z)| CrossHole {
        origin: Point3::new(x, y, z),
        dir: Point3::new(-x / 25.0, -y / 25.0, 0.0),
        radius: 2.5,
        entry_d: 0.0,
        floor_d: -8.0,
        through: false,
    })
    .collect();
    let mesh = super::tests_punched_bar::punched_bar(25.0, -40.0, 50.0, &holes);
    for horizontal in [false, true] {
        // A proper rotation also puts opposite mouths on world Z,
        // as in the reported false entry_z=25, floor_z=-25 finding.
        let rotate = |p: Point3| {
            if horizontal {
                Point3::new(p.x, p.z, -p.y)
            } else {
                p
            }
        };
        let tris: Vec<_> = mesh
            .facets
            .iter()
            .map(|t| Triangle::new(rotate(t.a), rotate(t.b), rotate(t.c)))
            .collect();
        let axial = recognize(&tris);
        assert_eq!(axial.len(), usize::from(horizontal), "{axial:?}");
        let mut found = recognize_cross(&tris);
        found.extend(axial.into_iter().map(|h| CrossHole {
            origin: Point3::new(h.xy.x, h.xy.y, h.entry_z),
            dir: Point3::new(0.0, 0.0, -1.0),
            radius: h.radius,
            entry_d: 0.0,
            floor_d: h.floor_z - h.entry_z,
            through: h.through,
        }));
        assert_eq!(found.len(), 6, "{found:?}");
        for mut expected in holes.iter().copied() {
            expected.origin = rotate(expected.origin);
            expected.dir = rotate(expected.dir);
            let actual = found
                .iter()
                .find(|h| h.origin == expected.origin)
                .unwrap_or_else(|| panic!("missing {expected:?}: {found:?}"));
            assert_eq!(actual, &expected);
        }
    }
}

/// Split only the blind floors. Each replacement preserves the original
/// triangle's boundary and winding, but introduces interior vertices.
fn subdivide_floors(mesh: Vec<Triangle>) -> Vec<Triangle> {
    mesh.into_iter()
        .flat_map(|t| {
            if t.a.z.abs() == 17.0 && t.b.z == t.a.z && t.c.z == t.a.z {
                let center = Point3::new(
                    (t.a.x + t.b.x + t.c.x) / 3.0,
                    (t.a.y + t.b.y + t.c.y) / 3.0,
                    t.a.z,
                );
                vec![
                    Triangle::new(t.a, t.b, center),
                    Triangle::new(t.b, t.c, center),
                    Triangle::new(t.c, t.a, center),
                ]
            } else {
                vec![t]
            }
        })
        .collect()
}

#[test]
fn subdivided_blind_z_floors_preserve_mouths_and_depths() {
    let mut mesh = opposite_blind_block();
    for _ in 0..2 {
        mesh = subdivide_floors(mesh);
        let found = recognize(&mesh);
        assert_eq!(found.len(), 1, "{found:?}");
        assert_eq!(found[0].entry_z, 25.0);
        assert_eq!(found[0].floor_z, 17.0);
        assert_eq!(found[0].radius, 2.5);
        assert!(!found[0].through);
        let reverse = recognize_cross(&mesh);
        assert_eq!(reverse.len(), 1, "{reverse:?}");
        assert_eq!(reverse[0].origin, Point3::new(0.0, 0.0, -25.0));
        assert_eq!(reverse[0].dir, Point3::new(0.0, 0.0, 1.0));
        assert_eq!(reverse[0].radius, 2.5);
        assert_eq!(reverse[0].entry_d, 0.0);
        assert_eq!(reverse[0].floor_d, -8.0);
        assert!(!reverse[0].through);
    }
}

#[test]
fn subdivided_blind_radial_floors_preserve_mouths_and_depths() {
    let mut mesh = opposite_blind_block();
    for _ in 0..2 {
        mesh = subdivide_floors(mesh);
        for along_x in [true, false] {
            let rotate = |p: Point3| {
                if along_x {
                    Point3::new(p.z, p.x, p.y)
                } else {
                    Point3::new(p.y, p.z, p.x)
                }
            };
            let radial: Vec<_> = mesh
                .iter()
                .map(|t| Triangle::new(rotate(t.a), rotate(t.b), rotate(t.c)))
                .collect();
            assert!(recognize(&radial).is_empty());
            let found = recognize_cross(&radial);
            assert_eq!(found.len(), 2, "{found:?}");
            for side in [-1.0, 1.0] {
                let origin = rotate(Point3::new(0.0, 0.0, side * 25.0));
                let hole = found
                    .iter()
                    .find(|h| h.origin == origin)
                    .unwrap_or_else(|| panic!("missing {origin:?}: {found:?}"));
                assert_eq!(hole.dir, rotate(Point3::new(0.0, 0.0, -side)));
                assert_eq!(hole.radius, 2.5);
                assert_eq!(hole.entry_d, 0.0);
                assert_eq!(hole.floor_d, -8.0);
                assert!(!hole.through);
            }
        }
    }
}

#[test]
fn disconnected_crossing_faces_do_not_close_bore_mouths() {
    let original = opposite_blind_block();
    let mut mesh = original.clone();
    // These patches intersect the probe rays, but have no edge path to a
    // bore rim. The interior patch also lies between a mouth and its floor.
    for z in [-25.0, -21.0, 21.0, 25.0] {
        mesh.push(Triangle::new(
            Point3::new(0.0, 0.0, z),
            Point3::new(2.0, 0.0, z),
            Point3::new(1.0, 2.0, z),
        ));
    }
    assert_eq!(recognize(&mesh), recognize(&original));
    assert_eq!(recognize_cross(&mesh), recognize_cross(&original));
    let rotate = |p: Point3| Point3::new(p.z, p.x, p.y);
    let radial: Vec<_> = mesh
        .iter()
        .map(|t| Triangle::new(rotate(t.a), rotate(t.b), rotate(t.c)))
        .collect();
    assert_eq!(recognize_cross(&radial), recognize_cross(&radial_block()));
}
