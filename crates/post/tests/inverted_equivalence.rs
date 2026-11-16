//! The key safety law of #374: a program posted for a machine with an
//! inverted axis, read by THAT machine's controller (its + on the
//! inverted axis moves the other way), drives the tool along the CAM path:
//! lines, G2/G3 in G17/G18/G19 with their I/J/K, the work-offset
//! origin, the drilled hole, and the physical side cutter compensation
//! offsets to. And the converse: the standard-sign program on that
//! machine does NOT (the reader is not a tautology).
//!
//! Gate map (M4.4): covers post::invert, post::engine, assets/posts fanuc.cps + haas.cps.

mod inverted_support;

use camcraft_post::post::engine;
use inverted_support::{
    assert_same_path, cam_events, controller_events, cps, machine, probe_records, probe_stream, Event,
};

/// The posted program's physical path within this of the CAM path: the
/// dialects write three decimals.
const TOL: f64 = 0.002;

const CASES: &[(&[&str], [bool; 3])] = &[
    (&["X"], [true, false, false]),
    (&["Y"], [false, true, false]),
    (&["X", "Y"], [true, true, false]),
];

#[test]
fn an_inverted_machine_cuts_the_cam_path() {
    let want = cam_events(&probe_records());
    assert!(want.iter().any(|e| matches!(e, Event::Engage(_))) && want.iter().any(|e| matches!(e, Event::Hole(_))));
    for stem in ["fanuc", "haas"] {
        // The reader itself, on a standard machine: the CAM path.
        let out = engine::run(&cps(stem), &machine(&[]), &probe_stream()).expect("standard posts");
        assert!(out.warnings.is_empty(), "{stem}: {:?}", out.warnings);
        assert_same_path(&want, &controller_events(&out.program, [false; 3]), TOL, &format!("{stem} standard"));
        for (letters, flip) in CASES {
            let what = format!("{stem} inverted {letters:?}");
            let out =
                engine::run(&cps(stem), &machine(letters), &probe_stream()).unwrap_or_else(|e| panic!("{what}: {e}"));
            assert!(out.warnings.is_empty(), "{what}: {:?}", out.warnings);
            assert_same_path(&want, &controller_events(&out.program, *flip), TOL, &what);
        }
    }
}

#[test]
fn the_standard_program_on_an_inverted_machine_strays() {
    let want = cam_events(&probe_records());
    let out = engine::run(&cps("fanuc"), &machine(&[]), &probe_stream()).expect("standard posts");
    for (letters, flip) in CASES {
        let got = controller_events(&out.program, *flip);
        let same = std::panic::catch_unwind(|| assert_same_path(&want, &got, TOL, "probe"));
        assert!(same.is_err(), "inverted {letters:?}: the untranslated program must not pass the reader");
    }
}
