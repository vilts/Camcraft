//! #864: a STEP import reports the chord error of the mesh it KEPT. A model
//! that fits the facet budget reaches the 0.005 mm cap and its mesh is the
//! one `parse` always made; a model forced over budget keeps a coarser mesh
//! and says by how much.
//!
//! Gate map: covers app::geometry::import::step and step_refine.
//!
//! Provenance: our own pins over the importer's tessellation schedule.

use camcraft_app::geometry::import::step;

fn milling_test() -> Vec<u8> {
    let p = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../testdata/models/Milling-test.step");
    std::fs::read(p).expect("the Milling-test fixture is in testdata")
}

#[test]
fn a_model_that_fits_the_budget_reaches_the_cap_and_keeps_its_mesh() {
    let data = milling_test();
    let (mesh, chord) = step::parse_with_chord(&data).expect("imports");
    assert!(chord > 0.0 && chord <= step::CHORD_CAP, "full fidelity: {chord}");
    let plain = step::parse(&data).expect("imports");
    assert_eq!(mesh.facets.len(), plain.facets.len());
    assert!(mesh.facets.iter().zip(plain.facets.iter()).all(|(a, b)| a == b), "same mesh as before #864");
}

#[test]
fn a_model_forced_over_budget_reports_the_coarser_chord_it_kept() {
    let data = milling_test();
    let (full, _) = step::parse_with_chord(&data).expect("imports");
    let budget = 2_000;
    let (mesh, chord) = step::with_facet_budget(budget, || step::parse_with_chord(&data)).expect("imports at reduced fidelity");
    assert!(chord > step::CHORD_CAP, "reduced fidelity is reported: {chord}");
    assert!(chord.is_finite());
    assert!(mesh.facets.len() <= budget, "{} facets within the {budget} budget", mesh.facets.len());
    assert!(mesh.facets.len() < full.facets.len());
    // The override is gone once the closure returns.
    let (_, again) = step::parse_with_chord(&data).expect("imports");
    assert!(again <= step::CHORD_CAP);
}
