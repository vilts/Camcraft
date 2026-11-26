//! #1536: a STEP face that cannot be meshed is refused by name inside a
//! bounded time, never a hang; smooth B-spline faces still import;
//! degenerate B-spline input still ends cleanly. #1568: a model that keeps
//! a coarser mesh says its own chord, and a face that may jump is never
//! smeared into a kept mesh.
//!
//! Since #917 the solidcore kernel reads the file. The old reader's
//! per-face evaluation cap is gone with it: the kernel refuses the jumping
//! face when it reads it (its edge lies off the face's surface), and a
//! model over its facet budget is meshed again at a coarser chord.
//!
//! Fixtures (testdata/models/): `bspline_discontinuous.step` is box.step
//! with its top face a degree-1 B-spline whose interior knot is repeated
//! past its degree, so the surface jumps at u = 0.5; `bspline_dome.step`
//! is box.step with a smooth biquadratic dome on top.
//!
//! The default-import pin lives in its own binary,
//! `import_step_guard_default_cap` (#1605).
//!
//! Gate map (M4.4): covers app::geometry::import::step and
//! camcraft_geometry::step (through it).

use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use camcraft_app::geometry::import::{self, step};
use camcraft_app::geometry::{Mesh, Point3};

fn testdata(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../testdata/models").join(name)
}

/// Run `f` on its own thread and fail the test - instead of hanging the
/// suite - when it has not answered within `limit`.
fn within<R: Send + 'static>(limit: Duration, f: impl FnOnce() -> R + Send + 'static) -> R {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(f());
    });
    match rx.recv_timeout(limit) {
        Ok(r) => r,
        Err(_) => panic!("import did not finish within {limit:?} - the hang is back"),
    }
}

/// The outer hang barrier of the refusal pins. Measured (#917, debug):
/// the kernel refuses the jumping face in under 0.1 s. The barrier is
/// 120 s, so a loaded runner never turns a finished refusal into a false
/// hang.
const HANG_BARRIER: Duration = Duration::from_secs(120);

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
fn the_jumping_face_is_refused_by_name_at_once() {
    let r = within(HANG_BARRIER, || {
        import::load_model(&testdata("bspline_discontinuous.step"))
    });
    let err = r.expect_err("refused").to_string();
    assert!(err.contains("face #53") && err.contains("off the face's surface"), "{err}");
}

#[test]
fn a_smooth_bspline_face_still_imports_closed_at_its_volume() {
    let m = within(Duration::from_secs(60), || import::load_model(&testdata("bspline_dome.step")))
        .expect("a smooth B-spline face imports");
    let b = m.bounds().expect("non-empty");
    // The dome's crown: 1 + 0.2 * (2 * 0.5 * 0.5)^2 = 1.05 at u = v = 0.5.
    assert!((b.max.z - 1.05).abs() < 5e-3, "dome crown at {}", b.max.z);
    let want = 1.0 + 0.2 / 9.0;
    let got = volume(&m);
    assert!((got - want).abs() < 2e-3, "volume {got}, want {want}");
    assert!(b.min.x.abs() < 1e-9 && (b.max.x - 1.0).abs() < 1e-9, "{b:?}");
}

#[test]
fn degenerate_bspline_variants_end_cleanly_and_bounded() {
    // Knots, multiplicities and points bent into nonsense: each must come
    // back - an import or a refusal - never a hang or a crash.
    let text = std::fs::read_to_string(testdata("bspline_discontinuous.step")).expect("fixture");
    let knots = "(2, 2, 2), (2, 2), (0.0, 0.5, 1.0), (0.0, 1.0)";
    assert!(text.contains(knots), "fixture layout changed");
    let variants = [
        "(2, 2, 2), (2, 2), (0.0, 0.0, 1.0), (0.0, 1.0)",
        "(2, 2, 2), (2, 2), (0.0, 0.5, 0.5), (0.0, 1.0)",
        "(2, 2, 2), (2, 2), (1.0, 0.5, 0.0), (0.0, 1.0)",
        "(2, 2, 2), (2, 2), (0.0, 0.5, 1.0), (1.0, 1.0)",
        "(3, 3, 3), (2, 2), (0.0, 0.5, 1.0), (0.0, 1.0)",
        "(1, 1, 1), (2, 2), (0.0, 0.5, 1.0), (0.0, 1.0)",
        "(2, 2, 2), (2, 2), (0.0, 0.5, 1.0E300), (0.0, 1.0)",
    ];
    for v in variants {
        let data = text.replace(knots, v).into_bytes();
        let started = Instant::now();
        let r = within(Duration::from_secs(60), move || {
            import::parse_model(&data, import::Format::Step)
        });
        if let Ok(m) = &r {
            assert!(m.facets.iter().all(|t| [t.a, t.b, t.c].iter().all(|p: &Point3| p.is_finite())), "{v}");
        }
        eprintln!("{v}: {} in {:?}", r.map(|m| format!("{} facets", m.len())).unwrap_or_else(|e| e.to_string()), started.elapsed());
    }
}


/// #1568: a model kept coarser than the cap says its own chord. Since
/// #917 a facet budget the dome does not fit at the cap makes the importer
/// mesh it again at a coarser chord (the old reader's evaluation cap is
/// gone); the kept mesh is smaller and keeps its volume.
#[test]
fn a_dome_over_its_facet_budget_keeps_a_coarser_mesh_at_its_own_chord() {
    let data = std::fs::read(testdata("bspline_dome.step")).expect("fixture");
    let (full, full_chord) = step::parse_with_chord(&data).expect("the dome imports");
    let budget = full.len() * 9 / 10;
    let (m, chord) = within(Duration::from_secs(60), move || {
        step::with_facet_budget(budget, || step::parse_with_chord(&data))
    })
    .expect("a smooth face over the budget keeps a coarser mesh");
    assert!(chord > full_chord, "the kept mesh reports its own, coarser chord: {chord} vs {full_chord}");
    assert!(m.len() <= budget && m.len() < full.len(), "{} facets kept, {} at full detail", m.len(), full.len());
    // Every facet stands at most `chord` off its surface, and only the dome
    // (area under 1.2 mm^2) is curved: the volume is off by at most
    // 1.2 * chord. The earlier 2e-3 bound was for a finer mesh; at a coarser
    // chord the inscribed dome loses more (#917: 1.0194 at a 0.01 chord).
    let want = 1.0 + 0.2 / 9.0;
    assert!((volume(&m) - want).abs() <= 1.2 * chord, "volume {}, want {want} within {}", volume(&m), 1.2 * chord);
    assert!((volume(&full) - want).abs() < 2e-3, "at the cap: volume {}, want {want}", volume(&full));
}

/// #1568 / #1536: a face that may jump is never smeared into a kept
/// mesh. Under a small facet budget the jumping face is still refused by
/// name, never meshed wide over the jump. Since #917 the kernel refuses it
/// when it reads the file, before the importer tries a coarser chord.
#[test]
fn a_face_that_may_jump_is_refused_by_name_under_a_small_facet_budget() {
    let r = within(HANG_BARRIER, || {
        step::with_facet_budget(12, || import::load_model(&testdata("bspline_discontinuous.step")))
    });
    let err = r.expect_err("a smeared jump is never kept").to_string();
    assert!(err.contains("face #53") && err.contains("off the face's surface"), "{err}");
}

/// #1599: a hostile shell of many runaway faces (the fixture's jumping face
/// #53 listed 40 more times) is refused by name at once, not after every
/// face: the kernel refuses the repeated face when it reads the shell.
#[test]
fn a_shell_of_many_runaway_faces_is_refused_after_a_few_trips() {
    let text = std::fs::read_to_string(testdata("bspline_discontinuous.step")).expect("fixture");
    let shell = "CLOSED_SHELL('', (#18, #25, #32, #39, #46, #53));";
    assert!(text.contains(shell), "the fixture's shell line");
    let hostile = text.replace(shell, &format!("CLOSED_SHELL('', (#18, #25, #32, #39, #46, #53{}));", ", #53".repeat(40)));
    let started = Instant::now();
    let r = within(HANG_BARRIER, move || {
        import::parse_model(hostile.as_bytes(), import::Format::Step)
    });
    let err = r.expect_err("refused").to_string();
    // The old "stopped after 3 faces or edges": now no face is meshed at
    // all; the solid is refused when its shell is read.
    assert!(err.contains("first #16 at read: face #53 is used twice"), "{err}");
    assert!(started.elapsed() < Duration::from_secs(30), "refused in {:?}", started.elapsed());
}
