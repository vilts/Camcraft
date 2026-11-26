//! #2780 at the app: the user's import of a closed solid with one strongly
//! twisted B-spline flank. `ImportModel` of `twisted_flank.step` lands the
//! model at full fidelity and its true volume (8000 mm3); the same solid
//! with one vertex lifted 0.5 mm off its curves (`twisted_flank_off.step`)
//! is refused by name. Never a model with a wall missing.
//! The geometry twin is `crates/geometry/tests/step_twisted_flank.rs`, the
//! kernel twin `crates/kernel/tests/step_twisted_flank.rs`.
//! Provenance: our own fixtures, written from ISO 10303-21/-42.
//!
//! Gate map (#2780): covers app::commands::import_model (ImportModel),
//! app::geometry::import (STEP through solidcore), app::model_fidelity.

use std::path::PathBuf;

use camcraft_app::commands::{History, ImportModel};
use camcraft_app::geometry::Mesh;
use camcraft_app::world::new_world;
use camcraft_app::{model, model_fidelity};

fn fixture(name: &str) -> PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../testdata/models").join(name)
}

/// Signed volume of a closed mesh (divergence theorem).
fn volume(m: &Mesh) -> f64 {
    m.facets
        .iter()
        .map(|t| {
            let (a, b, c) = (t.a, t.b, t.c);
            (a.x * (b.y * c.z - b.z * c.y) - a.y * (b.x * c.z - b.z * c.x) + a.z * (b.x * c.y - b.y * c.x)) / 6.0
        })
        .sum()
}

#[test]
fn the_twisted_flank_imports_at_full_fidelity_and_its_volume() {
    let cmd = match ImportModel::new(&fixture("twisted_flank.step")) {
        Ok(c) => c,
        Err(e) => panic!("twisted_flank.step must import: {e}"),
    };
    assert_eq!(cmd.coarse_mm, None, "the model meets the importer's promise");
    let v = volume(&cmd.mesh);
    // The kernel twin's tolerance: 0.8 mm3 in 8000.
    assert!((v - 8000.0).abs() <= 0.8, "volume {v} mm3, want 8000");
    let world = new_world();
    let mut h = History::default();
    h.execute(&world, cmd.renamed("flank"));
    let e = model::find_by_name(&world, "flank").expect("the model lands");
    assert_eq!(model_fidelity::chord_of(&world, &e), None, "landed at full fidelity");
}

#[test]
fn the_twisted_flank_with_a_vertex_off_its_curves_is_refused_by_name() {
    match ImportModel::new(&fixture("twisted_flank_off.step")) {
        Ok(c) => panic!("twisted_flank_off.step must be refused, read {} facets", c.mesh.facets.len()),
        Err(e) => {
            let s = format!("{e}");
            // #3767: solidcore fa67b3f5 refuses at its vertex check, which
            // names the solid and the vertex fault, not face #93.
            assert!(s.contains("#117 at check: a vertex sits"), "the refusal names the solid: {s}");
            assert!(s.contains("mm off its edges"), "the refusal says why: {s}");
        }
    }
}
