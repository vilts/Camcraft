//! Conservative rest-stock carry across a WCS yaw (#760, PR #2136).
//! Provenance: our own sparse height-grid fixtures and rigid-rotation law.
//! Gate map: covers camcraft_sim::stock_sim::dexel, camcraft_sim::stock_sim.

use camcraft_sim::geo2d::Point2;
use camcraft_sim::stock_sim::Dexel;

fn rotate(p: Point2, origin: Point2, degrees: f64) -> Point2 {
    let (s, c) = degrees.to_radians().sin_cos();
    let (x, y) = (p.x - origin.x, p.y - origin.y);
    Point2::new(origin.x + x * c - y * s, origin.y + x * s + y * c)
}

#[test]
fn oblique_rotation_preserves_narrow_standing_cell() {
    let mut source = Dexel::block(0.0, 0.0, 10.0, 10.0, 0.0, 1.0).unwrap();
    source.set(2 + source.nx, 10.0);
    let origin = Point2::new(0.0, 0.0);
    let rotated = source.rotated_z_conservative(origin, 30_f64.to_radians()).unwrap();
    let p = Point2::new(2.01, 1.01);
    let q = rotate(p, origin, 30.0);
    assert_eq!(source.top_at(source.col(p).unwrap()), 10.0);
    assert_eq!(rotated.top_at(rotated.col(q).unwrap()), 10.0,
        "an overlapping standing source cell must never be reported cleared");
}

#[test]
fn yaw_carry_never_lowers_standing_stock_at_rotated_source_points() {
    let mut source = Dexel::block(-3.0, -2.0, 7.0, 8.0, 0.0, 1.0).unwrap();
    // Sparse, disjoint standing regions, including each boundary of the grid.
    for (i, j, top) in [(0, 0, 4.0), (2, 1, 10.0), (7, 3, 7.0), (9, 9, 6.0)] {
        source.set(i + j * source.nx, top);
    }
    for origin in [Point2::new(0.0, 0.0), Point2::new(12.25, -7.75)] {
        for degrees in [-135.0, -89.0, -30.0, 0.0, 17.0, 30.0, 45.0, 90.0, 179.0] {
            let rotated = source.rotated_z_conservative(origin, f64::to_radians(degrees)).unwrap();
            for j in 0..source.ny {
                for i in 0..source.nx {
                    let top = source.top_at(i + j * source.nx);
                    for dx in [0.001, 0.5, 0.999] {
                        for dy in [0.001, 0.5, 0.999] {
                            let p = Point2::new(source.x0 + i as f64 + dx, source.y0 + j as f64 + dy);
                            let q = rotate(p, origin, degrees);
                            let carried = rotated.top_at(rotated.col(q).expect("rotated grid covers source"));
                            assert!(carried >= top,
                                "yaw {degrees}, origin {origin:?}, source {p:?}: {top} became {carried}");
                        }
                    }
                }
            }
        }
    }
}
