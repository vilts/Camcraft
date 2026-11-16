//! #1536 / #1605: the default evaluation cap, as a user's import runs it, in
//! a test binary of its own. truck tessellates on rayon's one global pool;
//! this import holds pool workers for ~50 s, so any other test sharing the
//! process queued behind it and missed its hang barrier under load. Cargo
//! runs test binaries one after another, so alone here it starves nothing.
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

/// The default cap, as a user's import runs it: bounded, and named. Since
/// #1568 raised the cap to 150 M this spends ~10 s in release and ~27 s
/// in debug (it was 4.5 s debug at 25 M); the lowered-cap pins below cover
/// the same refusal in milliseconds.
#[test]
fn a_face_that_never_converges_is_refused_by_name_within_the_budget() {
    let started = Instant::now();
    let r = within(Duration::from_secs(120), || import::load_model(&testdata("bspline_discontinuous.step")));
    let err = r.expect_err("a surface that never tessellates must be refused").to_string();
    assert!(
        err.contains("face #53 (B-spline surface, face 6 of 6 in shell #17)"),
        "the refusal names the face, its kind and its shell: {err}"
    );
    assert!(err.contains("did not converge"), "{err}");
    assert!(err.contains("import an STL"), "the refusal says what to do: {err}");
    eprintln!("refused in {:?}", started.elapsed());
}
