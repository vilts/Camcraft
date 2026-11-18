//! M13.3: the `camcraft-cl` headless binary and the `cl::text` dump it
//! reads - every app golden reads back and re-writes byte for byte; a
//! missing or malformed CLFile exits non-zero with a message, never a
//! panic (Prime Directive).
//!
//! Gate map (M4.4): covers camcraft_cl::cl::text, src/bin/camcraft-cl.rs.
//!
//! Provenance: our own fixtures; the goldens are the app's committed
//! CL dumps (crates/app/tests/golden).

use camcraft_cl::cl::text;
use std::path::PathBuf;
use std::process::{Command, Output};

fn goldens() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../app/tests/golden")
}

fn run(args: &[&std::ffi::OsStr]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_camcraft-cl")).args(args).output().expect("spawn")
}

#[test]
fn cl_text_round_trips_every_golden() {
    let mut n = 0;
    for e in std::fs::read_dir(goldens()).expect("golden dir") {
        let p = e.expect("entry").path();
        if p.extension().is_none_or(|x| x != "cl") {
            continue;
        }
        let src = std::fs::read_to_string(&p).expect("read golden").replace("\r\n", "\n");
        let t = match text::from_text(&src) {
            Ok(t) => t,
            Err(e) => panic!("{}: {e}", p.display()),
        };
        let back = text::to_text(&t).expect("finite");
        assert_eq!(back, src, "{}", p.display());
        n += 1;
    }
    assert!(n > 10, "only {n} goldens found");
}

#[test]
fn cl_bin_prints_move_stats() {
    let p = goldens().join("contour_disc_hole.cl");
    let o = run(&[p.as_os_str()]);
    let out = String::from_utf8_lossy(&o.stdout);
    assert!(o.status.success(), "{out}{}", String::from_utf8_lossy(&o.stderr));
    assert!(out.contains("moves: 46 (rapid 8, feed 34, plunge 4, arcs 22)"), "{out}");
    assert!(out.contains("chained: true"), "{out}");
}

#[test]
fn cl_bin_refuses_bad_input() {
    let bad = std::env::temp_dir().join(format!("cc-cl-{}-bad.cl", std::process::id()));
    for (body, why) in [
        ("START 0 0 0\nFeed 1 2\n", "needs X Y Z"),
        ("Feed 1 2 3\n", "before any START"),
        ("START 0 0 0\nArc 1 0 0 F1 I0\n", "center words"),
        ("START 0 0 0\nWarp 1 2 3\n", "unknown record"),
        ("START 0 0 0\nFeed 1.5 2 3\n", "not an integer"),
        ("START 0 0 0\nFeed 1 2 3 A5\n", "all of A, B and C"),
        ("START 0 0 0\n; CYCLE Drill X0 AT9\n", "past the"),
    ] {
        std::fs::write(&bad, body).expect("write fixture");
        let o = run(&[bad.as_os_str()]);
        let err = String::from_utf8_lossy(&o.stderr);
        assert_eq!(o.status.code(), Some(1), "{body:?}: {err}");
        assert!(err.contains(why) && !err.contains("panicked"), "{body:?}: {err}");
    }
    let _ = std::fs::remove_file(bad);
    let o = run(&["no-such-file.cl".as_ref()]);
    assert_eq!(o.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&o.stderr).contains("cannot read"));
}
