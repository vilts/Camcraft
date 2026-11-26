//! #1536 / #1605: the jumping face, as a user's import reads it, in a test
//! binary of its own. Since #917 the solidcore kernel refuses the face when
//! it reads the file, in well under a second; the binary stays apart so
//! the pin keeps its own process, as before.
//! Fixture: testdata/models/bspline_discontinuous.step (see
//! import_step_guard.rs).
//!
//! Gate map (M4.4): covers app::geometry::import::step,
//! app::geometry::import::step_guard.

use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use camcraft_app::geometry::import;

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

/// The default import, as a user's runs it: bounded, and named, with what
/// to do next.
#[test]
fn a_face_that_never_converges_is_refused_by_name_within_the_budget() {
    let started = Instant::now();
    let r = within(Duration::from_secs(120), || import::load_model(&testdata("bspline_discontinuous.step")));
    let err = r.expect_err("a surface that never tessellates must be refused").to_string();
    assert!(err.contains("face #53") && err.contains("#16"), "the refusal names the face and its solid: {err}");
    // The solid's size and the face's place in it, as the old "face 6 of 6
    // in shell #17" said them (#917: the kernel names the solid, #16).
    assert!(err.contains("first #16 at read: pcurves failed on 1 of 6 faces; first: face #53"), "{err}");
    assert!(err.contains("edge 0 lies 0.2500 mm off the face's surface"), "{err}");
    assert!(err.contains("import an STL"), "the refusal says what to do: {err}");
    eprintln!("refused in {:?}", started.elapsed());
}
