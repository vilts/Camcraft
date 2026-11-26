//! #163, #2595: a native diag line is a `tracing` event at its level under
//! the `camcraft_ui` target, so the launch terminal's filter governs it.
//! This test has a binary of its own (#2595). `tracing` caches the interest
//! of each callsite for the whole process, and in the lib test binary other
//! threads hit the same `diag::info` callsite while this test installs its
//! thread-local subscriber. A first hit on another thread could then cache
//! the INFO callsite as disabled, and the test lost its INFO line.
//! Keep this file to ONE test: a second test would share the callsites again.
//! Provenance: our own test, specced from the `tracing` and
//! `tracing-subscriber` docs.
//! Gate map: covers camcraft_ui::diag.

#![cfg(not(target_arch = "wasm32"))]

use std::sync::{Arc, Mutex};

use camcraft_ui::diag;

/// Everything the test subscriber writes.
#[derive(Clone, Default)]
struct Captured(Arc<Mutex<Vec<u8>>>);

impl Captured {
    fn text(&self) -> String {
        self.0.lock().map(|v| String::from_utf8_lossy(&v).into_owned()).unwrap_or_default()
    }
}

impl std::io::Write for Captured {
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

fn subscriber(filter: &str, out: &Captured) -> impl tracing::Subscriber + Send + Sync + use<> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::new(filter))
        .with_writer(out.clone())
        .with_ansi(false)
        .finish()
}

#[test]
fn diag_lines_are_tracing_events_at_their_level() {
    let out = Captured::default();
    tracing::subscriber::with_default(subscriber("info", &out), || {
        diag::warn(&format!("gpu: test adapter failed: {} - trying the next adapter", "no surface"));
        diag::info("headless: test adapter");
    });
    let text = out.text();
    let warn = text.lines().find(|l| l.contains("gpu: test adapter failed: no surface")).unwrap_or("");
    assert!(warn.contains("WARN") && warn.contains("camcraft_ui"), "the warning is a WARN event: {text}");
    let info = text.lines().find(|l| l.contains("headless: test adapter")).unwrap_or("");
    assert!(info.contains("INFO") && info.contains("camcraft_ui"), "the note is an INFO event: {text}");

    let quiet = Captured::default();
    tracing::subscriber::with_default(subscriber("camcraft_ui=warn", &quiet), || diag::info("headless: filtered out"));
    let text = quiet.text();
    assert!(!text.contains("filtered out"), "a note below the filter stays quiet: {text}");
}
