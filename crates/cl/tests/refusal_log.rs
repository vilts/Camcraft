//! #163: a refusal in the CL library reaches the launch terminal as a
//! `tracing` warning (target `camcraft_cl`), not an unfiltered eprintln -
//! so the app's one subscriber prints it with its level and the
//! `CAMCRAFT_LOG` / `RUST_LOG` filter governs it.
//!
//! Gate map (#163): covers camcraft_cl::cl::moves (with_axis,
//! with_rotary), camcraft_cl::cl::path (PathBuilder::set_axis).
//!
//! Provenance: our own fixtures over our own log lines.

use camcraft_cl::cl::{Move, MoveKind, PathBuilder};
use camcraft_cl::geometry::Point3;
use std::io::Write;
use std::sync::{Arc, Mutex};

/// Everything the subscriber writes.
#[derive(Clone, Default)]
struct Captured(Arc<Mutex<Vec<u8>>>);

impl Write for Captured {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        if let Ok(mut v) = self.0.lock() {
            v.extend_from_slice(buf);
        }
        Ok(buf.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl<'a> tracing_subscriber::fmt::MakeWriter<'a> for Captured {
    type Writer = Captured;
    fn make_writer(&'a self) -> Self::Writer {
        self.clone()
    }
}

/// Run `f` under a subscriber with `filter`, on this thread only, and
/// return what it printed.
fn logged(filter: &str, f: impl FnOnce()) -> String {
    let out = Captured::default();
    let sub = tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::new(filter))
        .with_writer(out.clone())
        .with_ansi(false)
        .finish();
    tracing::subscriber::with_default(sub, f);
    out.0.lock().map(|v| String::from_utf8_lossy(&v).into_owned()).unwrap_or_default()
}

#[test]
fn a_refused_axis_or_angle_is_a_warning_under_the_cl_target() {
    let a = Point3::new(0.0, 0.0, 0.0);
    let b = Point3::new(1.0, 0.0, 0.0);
    let mut kept = None;
    let text = logged("info", || {
        let m = Move::new(MoveKind::Feed, a, b, Some(100.0)).with_axis([0.0, 0.0, 0.0]).with_rotary(f64::NAN);
        let mut pb = PathBuilder::new(a);
        pb.set_axis(Some([f64::NAN, 0.0, 1.0]));
        kept = Some(m);
    });
    let m = kept.unwrap_or_else(|| Move::new(MoveKind::Feed, a, b, None));
    assert!(m.axis.is_none() && m.rotary.is_none(), "the refused values are dropped, as before");
    for said in ["refusing a move axis", "refusing a C angle", "refusing a builder axis"] {
        let line = text.lines().find(|l| l.contains(said)).unwrap_or("");
        assert!(line.contains("WARN") && line.contains("camcraft_cl"), "{said} is a WARN camcraft_cl event: {text}");
    }
    let quiet = logged("camcraft_cl=error", || {
        let _ = Move::new(MoveKind::Feed, a, b, None).with_axis([0.0, 0.0, 0.0]);
    });
    assert!(!quiet.contains("refusing"), "the filter governs the line: {quiet}");
}
