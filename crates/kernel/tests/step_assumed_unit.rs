//! #1874: the caller's length unit for a STEP file that states none. The
//! unit is written into the file's unit contexts before the read, so the
//! solids read in it. A file that states its own unit keeps it.
//! Provenance: our own test design; mutates `box.step` in memory (ISO
//! 10303-41 unit records). No HSMWorks material consulted.
//!
//! Gate map (#1874): covers camcraft_kernel::step_assumed_unit,
//! camcraft_kernel::solidcore_step (read_step_assuming),
//! camcraft_kernel::solidcore_step_import (import_step_model_assuming).

use std::path::PathBuf;

use camcraft_kernel::solidcore_impl::SolidcoreKernel;
use camcraft_kernel::solidcore_step::{StepModel, import_step_model, import_step_model_assuming};
use solidcore::kernel::CancelToken;

fn box_text() -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../testdata/models/box.step");
    std::fs::read_to_string(path).expect("box.step").replace("\r\n", "\n")
}

/// `text` with `from` replaced by `to`; the edit must apply.
fn cut(text: String, from: &str, to: &str) -> String {
    assert!(text.contains(from), "fixture edit no longer applies: {from}");
    text.replacen(from, to, 1)
}

/// `box.step` with no length unit: the unit, the uncertainty that names
/// it, and their places in the context are taken out.
fn unitless() -> String {
    let t = cut(box_text(), "    GLOBAL_UNCERTAINTY_ASSIGNED_CONTEXT((#15))\n", "");
    let t = cut(t, "GLOBAL_UNIT_ASSIGNED_CONTEXT((#12, #13, #14))", "GLOBAL_UNIT_ASSIGNED_CONTEXT((#13, #14))");
    let t = cut(t, "#12 = ( LENGTH_UNIT() NAMED_UNIT(*) SI_UNIT(.MILLI.,.METRE.) );\n", "");
    cut(t, "#15 = UNCERTAINTY_MEASURE_WITH_UNIT(1.0E-6, #12, 'distance_accuracy_value','confusion accuracy');\n", "")
}

fn read(text: &str, assume: Option<f64>) -> StepModel {
    let stop = CancelToken::new();
    let got = match assume {
        None => import_step_model(&mut SolidcoreKernel::default(), text.as_bytes(), &stop),
        Some(mm) => import_step_model_assuming(&mut SolidcoreKernel::default(), text.as_bytes(), &stop, Some(mm)),
    };
    got.unwrap_or_else(|e| panic!("the box reads (assume {assume:?}): {e}"))
}

fn area(m: &StepModel) -> f64 {
    m.solids.iter().map(|s| s.area).sum()
}

#[test]
fn a_unitless_file_reads_in_the_unit_that_the_caller_states() {
    let none = read(&unitless(), None);
    assert!(none.length_units_mm.is_empty(), "the fixture states no length unit: {:?}", none.length_units_mm);
    assert_eq!(none.assumed_mm, None);
    let inch = read(&unitless(), Some(25.4));
    assert_eq!(inch.length_units_mm, vec![25.4], "the stated unit is the file's unit now");
    assert_eq!(inch.assumed_mm, Some(25.4), "the read says that it used the caller's unit");
    let ratio = area(&inch) / area(&none);
    assert!((ratio - 25.4 * 25.4).abs() < 1e-6 * 25.4 * 25.4, "an inch box has 25.4^2 times the area: {ratio}");
    let mm = read(&unitless(), Some(1.0));
    assert_eq!(mm.length_units_mm, vec![1.0]);
    assert!((area(&mm) - area(&none)).abs() < 1e-9 * area(&none), "a stated millimetre keeps the size");
}

#[test]
fn a_file_that_states_its_unit_keeps_it_whatever_the_caller_states() {
    let own = read(&box_text(), None);
    let asked = read(&box_text(), Some(25.4));
    assert_eq!(asked.length_units_mm, vec![1.0], "the file's millimetre stays");
    assert_eq!(asked.assumed_mm, None, "the caller's unit is not used");
    assert!((area(&asked) - area(&own)).abs() < 1e-9 * area(&own));
}

#[test]
fn a_stated_unit_out_of_range_is_refused_by_name() {
    let stop = CancelToken::new();
    for bad in [0.0, -1.0, f64::NAN, f64::INFINITY, 1.0e6] {
        let got = import_step_model_assuming(&mut SolidcoreKernel::default(), unitless().as_bytes(), &stop, Some(bad));
        let msg = got.err().map(|e| e.to_string()).unwrap_or_default();
        assert!(msg.contains("stated length unit"), "{bad} mm is refused by name: {msg:?}");
    }
}
