//! #3034: a closed STEP solid whose B-spline face ends in a pole at a
//! vertex imports as a closed mesh, and every slice of it closes. The QA
//! part (an ellipsoid boss on a mould base, from SolidCraft) was refused at
//! import: the deviation measure judged the fan around the boss's apex
//! 0.5 mm off its surface, though the mesh stood within 5 um of it.
//!
//! Fixture: `testdata/models/sphere_quarter_pole.step`, our own file (see
//! its header): a quarter sphere of radius 25 whose dome is a rational
//! B-spline turning 180 degrees about z, its v = 1 edge collapsed to the
//! apex. No user file.
//!
//! Gate map (M4.4): covers app::geometry::import::step,
//! app::geometry::import (parse_model), camcraft_geometry::geometry::slice.

use std::path::Path;

use camcraft_app::geometry::import::{self, Format};
use camcraft_app::geometry::{Point3, slice};

const R: f64 = 25.0;

fn fixture() -> Vec<u8> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../testdata/models/sphere_quarter_pole.step");
    std::fs::read(path).expect("fixture")
}

#[test]
fn a_solid_with_a_spline_pole_imports_closed_and_slices() {
    let mesh = import::parse_model(&fixture(), Format::Step)
        .unwrap_or_else(|e| panic!("a quarter sphere with a spline pole must import: {e}"));
    // Signed volume (divergence theorem): pi R^3 / 3, wound outward.
    let o = Point3::new(0.0, 0.0, 0.0);
    let volume: f64 = mesh.facets.iter().map(|t| (t.a - o).dot((t.b - o).cross(t.c - o)) / 6.0).sum();
    let exact = std::f64::consts::PI * R.powi(3) / 3.0;
    assert!((volume - exact).abs() < 10.0, "volume {volume} vs {exact}");
    // Each section is the half disc of radius sqrt(R^2 - z^2): closed, no
    // dangling chord ends, up to just under the pole.
    for z in [1.0, 12.5, 20.0, 24.0, 24.9] {
        let region = slice::slice(&mesh.facets, z).unwrap_or_else(|e| panic!("z = {z}: {e}"));
        let half_disc = std::f64::consts::PI * (R * R - z * z) / 2.0;
        assert!((region.area() - half_disc).abs() < 0.01 * half_disc + 0.05, "z = {z}: {} vs {half_disc}", region.area());
    }
}
