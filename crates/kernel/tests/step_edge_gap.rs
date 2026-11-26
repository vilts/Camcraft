//! #2976 at the kernel: the cap on how far an edge may lie off its face's
//! surface follows the tolerance that the file states (twice it, from
//! 0.01 mm to 0.02 mm), and a file that states none keeps 0.01 mm. A gap
//! just under the cap reads, with a note; a gap just over it is refused by
//! name with the gap, the cap and where the cap came from. A gap over the
//! cap that a large stated tolerance would give is refused by the machining
//! mesh at the chord. The tracked fixtures keep their verdicts.
//! Fixtures (our own, testdata/models/edge_gap/*.step, generated): a box
//! whose top face is a bilinear B-spline tilted by the gap along x while
//! its top edges stay flat, so its edges at the far side lie the gap off
//! the top surface.
//! Provenance: our own fixtures, written from ISO 10303-21/-42.
//!
//! Gate map (#2976): covers camcraft_kernel::step_edge_gap,
//! camcraft_kernel::solidcore_trims (KernelTrims, the cap) and
//! camcraft_kernel::solidcore_step (read_step, import_step: the edge gap).

mod step_util;

use std::path::PathBuf;

use camcraft_kernel::KernelError;
use camcraft_kernel::machining_mesh::{MeshRefusal, machining_mesh};
use camcraft_kernel::solidcore_impl::SolidcoreKernel;
use camcraft_kernel::solidcore_step::{StepImport, import_step};
use camcraft_kernel::step_edge_gap::{EDGE_GAP_LIMIT_MM, MAX_EDGE_GAP_MM};
use step_util::{CASE_TIMEOUT, bounded_cancel};

/// The importer's chord cap (mm), as the app passes it.
const CHORD: f64 = 0.005;

fn import(file: &'static str) -> Result<StepImport, KernelError> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../testdata/models")
        .join(file);
    let bytes = std::fs::read(&path).unwrap_or_default();
    assert!(!bytes.is_empty(), "fixture {file} is missing");
    bounded_cancel(file, CASE_TIMEOUT, move |stop| {
        let mut k = SolidcoreKernel::default();
        import_step(&mut k, &bytes, stop)
    })
}

fn refusal(file: &'static str) -> String {
    match import(file) {
        Ok(i) => panic!("{file} must be refused, read {} solid(s)", i.solids.len()),
        Err(e) => e.to_string(),
    }
}

fn near(a: f64, b: f64) -> bool {
    (a - b).abs() <= 1e-9
}

#[test]
fn a_gap_just_under_twice_the_stated_tolerance_reads_with_a_note() {
    let i = import("edge_gap/edge_gap_018_stated_010.step").expect("0.018 mm is under the 0.02 mm cap");
    assert_eq!(i.solids.len(), 1);
    let g = i.edge_gap;
    assert_eq!(g.cap.stated_mm, Some(0.01));
    assert!(near(g.cap.cap_mm, EDGE_GAP_LIMIT_MM), "{g:?}");
    assert!((g.widest_mm - 0.018).abs() <= 2e-4, "{g:?}");
    assert!(
        i.notes.iter().any(|n| n
            == "an edge lies 0.0180 mm off its face's surface; the file states a tolerance of 0.0100 mm, so the reader accepts up to 0.0200 mm"),
        "never a silent widening: {:?}",
        i.notes
    );
    // The mesh cannot meet the chord on the tilted face: the importer must
    // mesh it coarser, at twice the stated tolerance.
    assert_eq!(g.loose_chord_mm(CHORD), Some(EDGE_GAP_LIMIT_MM));
    let stop = solidcore::kernel::CancelToken::new();
    let at_chord = machining_mesh(&i.solids[0], CHORD, &stop);
    assert!(matches!(at_chord, Err(MeshRefusal::FaceCapped { .. })), "{at_chord:?}");
    let loose = machining_mesh(&i.solids[0], EDGE_GAP_LIMIT_MM, &stop).expect("meshes at 0.02 mm");
    assert!(loose.report.deviation.largest() <= EDGE_GAP_LIMIT_MM, "{:?}", loose.report.deviation);
}

#[test]
fn a_gap_just_over_twice_the_stated_tolerance_is_refused_by_name() {
    let s = refusal("edge_gap/edge_gap_021_stated_010.step");
    assert!(s.contains("face #46"), "names the face: {s}");
    assert!(
        s.contains(
            "edge 0 lies 0.0210 mm off the face's surface, over the cap of 0.02 mm; \
             the file states a tolerance of 0.01 mm, and the reader accepts twice that, from 0.01 mm to 0.02 mm"
        ),
        "names the gap, the cap and its source: {s}"
    );
}

#[test]
fn a_huge_stated_tolerance_does_not_raise_the_cap_past_its_limit() {
    // The file states 5 mm: the cap stops at 0.02 mm. Up to solidcore
    // 2016386 the reader fitted this face's pcurves itself and the machining
    // mesh refused the 0.021 mm step at the chord. Since fa67b3f5 (#3767)
    // the face reaches the kernel's pcurves, and the gap check refuses it at
    // the read, with the gap, the cap and the stated 5 mm.
    let s = refusal("edge_gap/edge_gap_021_stated_5mm.step");
    assert!(s.contains("face #46"), "names the face: {s}");
    assert!(
        s.contains(
            "edge 0 lies 0.0210 mm off the face's surface, over the cap of 0.02 mm; \
             the file states a tolerance of 5 mm, and the reader accepts twice that, from 0.01 mm to 0.02 mm"
        ),
        "the cap stops at its limit, not at 10 mm: {s}"
    );
}

#[test]
fn a_file_without_a_stated_tolerance_keeps_the_fixed_cap() {
    let i = import("edge_gap/edge_gap_009_unstated.step").expect("0.009 mm is under 0.01 mm");
    assert_eq!(i.edge_gap.cap.stated_mm, None);
    assert!(near(i.edge_gap.cap.cap_mm, MAX_EDGE_GAP_MM));
    // No stated tolerance, no looser mesh: the importer refuses it at the
    // chord, as before #2976.
    assert_eq!(i.edge_gap.loose_chord_mm(CHORD), None);
    let s = refusal("edge_gap/edge_gap_011_unstated.step");
    assert!(
        s.contains("edge 0 lies 0.0110 mm off the face's surface, over the cap of 0.01 mm; the file states no tolerance"),
        "refusal: {s}"
    );
}

#[test]
fn an_inch_files_tolerance_is_converted_to_millimetres() {
    // 0.0003 inch = 0.00762 mm: the cap is 0.01524 mm.
    let i = import("edge_gap/edge_gap_inch_014.step").expect("0.014 mm is under 0.01524 mm");
    let stated = i.edge_gap.cap.stated_mm.expect("stated");
    assert!((stated - 0.00762).abs() <= 1e-9, "{stated}");
    assert!((i.edge_gap.cap.cap_mm - 0.01524).abs() <= 1e-9, "{:?}", i.edge_gap);
    let loose = i.edge_gap.loose_chord_mm(CHORD).expect("a looser mesh");
    assert!((loose - 0.01524).abs() <= 1e-9, "{loose}");
    let s = refusal("edge_gap/edge_gap_inch_016.step");
    assert!(
        s.contains("edge 0 lies 0.0160 mm off the face's surface, over the cap of 0.01524 mm; the file states a tolerance of 0.00762 mm"),
        "refusal: {s}"
    );
}

#[test]
fn the_tracked_fixtures_keep_a_tight_gap_and_no_looser_mesh() {
    for file in ["fig8.step", "fillet_corner.step", "bar_cross_holes.step", "twisted_flank.step"] {
        let i = import(file).unwrap_or_else(|e| panic!("{file} reads: {e}"));
        assert!(i.edge_gap.widest_mm < 1e-3, "{file}: {:?}", i.edge_gap);
        assert_eq!(i.edge_gap.loose_chord_mm(CHORD), None, "{file}");
        assert!(!i.notes.iter().any(|n| n.contains("an edge lies")), "{file}: {:?}", i.notes);
    }
}
