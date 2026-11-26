//! #2976 at the importer: a model whose edges lie off their surfaces by
//! more than the chord, in a file that states a tolerance, is meshed at a
//! coarser tolerance (twice the stated one, never over 0.02 mm), and the
//! importer hands out that chord (reduced fidelity, #864) with a note in
//! plain words. A file that states no tolerance is refused as before, and
//! the tracked fixtures keep the chord cap.
//! Fixtures (our own, testdata/models/edge_gap/*.step, generated): a box
//! whose top is a B-spline tilted by the gap while its top edges stay flat.
//! Provenance: our own fixtures, written from ISO 10303-21/-42.
//!
//! Gate map (#2976): covers camcraft_geometry::step (the looser mesh).

#![cfg(not(target_arch = "wasm32"))]

use std::path::Path;

use camcraft_geometry::step::{self, StepError};

/// The importer's facet budget and its chord cap (mm), as the app passes them.
const BUDGET: usize = 30_000_000;
const CHORD: f64 = 0.005;

fn read(name: &str) -> Result<step::StepMesh, StepError> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../testdata/models").join(name);
    let data = std::fs::read(&path).expect("fixture");
    step::parse_open(&data, BUDGET, CHORD)
}

#[test]
fn a_gap_under_the_cap_meshes_coarser_with_a_plain_note() {
    let m = read("edge_gap/edge_gap_018_stated_010.step").expect("imports at reduced fidelity");
    assert!((m.chord_mm - 0.02).abs() <= 1e-12, "chord {}", m.chord_mm);
    assert!(m.deviation_mm <= m.chord_mm, "deviation {}", m.deviation_mm);
    assert!(m.open.is_none());
    assert_eq!(
        m.notes.first().map(String::as_str),
        Some(
            "This file's edges sit up to 0.018 mm off their surfaces. The file states a tolerance of 0.010 mm. \
             The model is used at reduced accuracy (0.020 mm)."
        ),
        "{:?}",
        m.notes
    );
    assert!(m.loosened);
}

#[test]
fn an_inch_file_meshes_at_twice_its_converted_tolerance() {
    let m = read("edge_gap/edge_gap_inch_014.step").expect("imports at reduced fidelity");
    assert!((m.chord_mm - 0.01524).abs() <= 1e-9, "chord {}", m.chord_mm);
    assert!(m.loosened);
    assert!(m.notes.first().is_some_and(|n| n.ends_with("reduced accuracy (0.015 mm).")), "{:?}", m.notes);
}

#[test]
fn without_a_stated_tolerance_or_a_measured_gap_there_is_no_looser_mesh() {
    // Since solidcore fa67b3f5 (#3767) edge_gap_021_stated_5mm.step is refused
    // at the read by the kernel's gap check (the second list), not at the
    // chord by the mesh: the cap stays at 0.02 mm, not at twice 5 mm.
    let file = "edge_gap/edge_gap_009_unstated.step";
    match read(file) {
        Ok(m) => panic!("{file} must be refused, meshed at {} mm", m.chord_mm),
        Err(e) => {
            let s = e.to_string();
            assert!(s.contains("capped at") && s.contains("tolerance 5e-3 mm"), "{file}: {s}");
        }
    }
    for file in [
        "edge_gap/edge_gap_021_stated_010.step",
        "edge_gap/edge_gap_021_stated_5mm.step",
        "edge_gap/edge_gap_011_unstated.step",
        "bspline_discontinuous.step",
    ] {
        let s = read(file).map(|m| m.chord_mm).expect_err(file).to_string();
        assert!(s.contains("off the face's surface, over the cap of"), "{file}: {s}");
    }
    let s = read("edge_gap/edge_gap_021_stated_5mm.step").map(|m| m.chord_mm).expect_err("5 mm").to_string();
    assert!(s.contains("over the cap of 0.02 mm; the file states a tolerance of 5 mm"), "{s}");
}

#[test]
fn the_tracked_fixtures_keep_the_chord_cap() {
    for file in ["fig8.step", "fillet_corner.step", "bar_cross_holes.step", "twisted_flank.step"] {
        let m = read(file).unwrap_or_else(|e| panic!("{file}: {e}"));
        assert!((m.chord_mm - CHORD).abs() <= 1e-12, "{file}: chord {}", m.chord_mm);
        assert!(!m.notes.iter().any(|n| n.contains("reduced accuracy")), "{file}: {:?}", m.notes);
        assert!(!m.loosened, "{file}");
    }
}
