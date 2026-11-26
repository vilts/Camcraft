//! #2780: a closed solid with one strongly twisted B-spline flank, read
//! through the solidcore kernel. The exact file reads at its true volume;
//! the file with a vertex off its curves is refused by name. Never a mesh
//! with a wall missing.
//! Fixtures (our own, testdata/models/): `twisted_flank.step` is a 40 x 40
//! mm block whose top is the degree-3 B-spline z = 60uv (volume 8000 mm3);
//! `twisted_flank_off.step` is the same solid with one vertex lifted 0.5 mm
//! off its curves and its surface.
//! Provenance: our own fixtures, written from ISO 10303-21/-42.
//!
//! Gate map (M4.4): covers camcraft_geometry::step.

#![cfg(not(target_arch = "wasm32"))]

use std::path::Path;

use camcraft_geometry::geometry::Mesh;
use camcraft_geometry::step::{self, StepError};

/// The importer's facet budget and its chord cap (mm), as the app passes them.
const BUDGET: usize = 30_000_000;
const CHORD: f64 = 0.005;

fn read(name: &str) -> Result<step::StepMesh, StepError> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../testdata/models").join(name);
    let data = std::fs::read(&path).expect("fixture");
    step::parse(&data, BUDGET, CHORD)
}

fn volume(m: &Mesh) -> f64 {
    m.facets
        .iter()
        .map(|t| {
            let (a, b, c) = (t.a, t.b, t.c);
            (a.x * (b.y * c.z - b.z * c.y) - a.y * (b.x * c.z - b.z * c.x) + a.z * (b.x * c.y - b.y * c.x)) / 6.0
        })
        .sum()
}

/// The volume tolerance of the kernel twin
/// (`crates/kernel/tests/step_twisted_flank.rs`): 0.8 mm3 in 8000.
const VOLUME_TOL: f64 = 0.8;

#[test]
fn a_twisted_bspline_flank_reads_at_its_volume() {
    let m = match read("twisted_flank.step") {
        Ok(m) => m,
        Err(e) => panic!("twisted_flank.step must read: {e}"),
    };
    let v = volume(&m.mesh);
    assert!((v - 8000.0).abs() <= VOLUME_TOL, "volume {v} mm3, want 8000");
    let b = m.mesh.bounds().expect("non-empty");
    assert!((b.max.z - 60.0).abs() < 0.05 && b.min.z.abs() < 1e-6, "{b:?}");
}

#[test]
fn a_twisted_flank_with_a_vertex_off_its_curves_is_refused_by_name() {
    // The kernel twin pins the same refusal: the flank's pcurve would have to
    // move the surface by up to 0.89 mm to reach the vertex. Since solidcore
    // fa67b3f5 (#3767) the vertex check refuses it (0.343 mm off its edges).
    match read("twisted_flank_off.step") {
        Ok(m) => panic!(
            "twisted_flank_off.step must be refused, read {} facets",
            m.mesh.facets.len()
        ),
        Err(e) => {
            let s = e.to_string();
            assert!(s.contains("#117 at check: a vertex sits"), "refusal names the entity: {s}");
            assert!(s.contains("mm off its edges"), "refusal: {s}");
        }
    }
}
