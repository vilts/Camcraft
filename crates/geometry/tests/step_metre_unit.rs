//! #917: a STEP file in metres meshes in millimetres at the importer's chord
//! cap, and its measured deviation proves it (the old reader stopped at
//! 1e-5 m, 0.01 mm, and flagged the model). Moved from the app's
//! `model_fidelity` test so that it calls the geometry reader in its own
//! crate (#2904 review, test placement); the app test keeps the flag.
//! Provenance: our own test design; `sample_ap214.step` is the repo's own.
//!
//! Gate map (#917): covers camcraft_geometry::step.

#![cfg(not(target_arch = "wasm32"))]

use std::path::Path;

use camcraft_geometry::step;

/// The importer's facet budget and its chord cap (mm), as the app passes them.
const BUDGET: usize = 30_000_000;
const CHORD: f64 = 0.005;

#[test]
fn a_metre_step_meshes_at_the_cap_with_a_measured_deviation() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../testdata/models/sample_ap214.step");
    let bytes = std::fs::read(path).expect("read");
    let read = step::parse(&bytes, BUDGET, CHORD).expect("the metre sample meshes");
    assert_eq!(read.chord_mm, CHORD, "meshed at the cap, not coarser");
    assert!(
        read.deviation_mm > 0.0 && read.deviation_mm <= CHORD,
        "measured deviation {} mm, promise {CHORD} mm",
        read.deviation_mm
    );
}
