//! Diagnostics to the launching terminal (#163): one `tracing`
//! subscriber for every binary, printing to stderr, filtered by
//! `CAMCRAFT_LOG` (else `RUST_LOG`, else [`DEFAULT_FILTER`]). The seams
//! that emit - project save/load, post writes, generation, command
//! dispatch - carry their own `tracing` events; this module only
//! decides where they go.
//!
//! Provenance: the `tracing` / `tracing-subscriber` crates' documented
//! fmt + EnvFilter setup. Our own wiring, no vendor material.

use std::path::Path;
use crate::clock::Instant;

use tracing_subscriber::util::SubscriberInitExt as _;
use tracing_subscriber::EnvFilter;

#[cfg(not(target_arch = "wasm32"))]
#[path = "logging_desktop.rs"]
mod desktop;
#[cfg(not(target_arch = "wasm32"))]
pub use desktop::{failure_message, init_desktop, log_path, startup_failure};

/// Milliseconds since `started`, saturating at `u64::MAX` rather than
/// truncating the `u128` `as_millis` returns (safe numeric conversion).
#[must_use]
pub fn elapsed_ms(started: Instant) -> u64 {
    u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX)
}

/// The file name a log line shows for `path`: the name only, never the
/// folder, so a log pasted into a bug report does not expose the user's
/// directory layout. A path with no final name shows as given.
#[must_use]
pub fn file_name(path: &Path) -> String {
    path.file_name()
        .map_or_else(|| path.display().to_string(), |n| n.to_string_lossy().into_owned())
}

/// Quiet by default: the app's own info lines, the GPU stack's warnings.
/// `CAMCRAFT_LOG=debug` (or `RUST_LOG=camcraft_app=debug`) opens it up.
pub const DEFAULT_FILTER: &str = "info,wgpu_core=warn,wgpu_hal=warn,naga=warn,winit=warn";

/// The filter the environment asks for, falling back to the default.
/// Invalid directives are ignored by name in the filter itself, never a
/// panic (Prime Directive).
#[must_use]
pub fn filter_from_env() -> EnvFilter {
    EnvFilter::try_from_env("CAMCRAFT_LOG")
        .or_else(|_| EnvFilter::try_from_default_env())
        .unwrap_or_else(|_| EnvFilter::new(DEFAULT_FILTER))
}

/// Build the `tracing` subscriber for `writer`. When `browser` is true
/// (`wasm32`), timestamps and ANSI colour codes are omitted: `SystemTime::now()`
/// panics on `wasm32-unknown-unknown` and browser consoles print plain text.
pub fn dispatch<W>(writer: W, browser: bool) -> tracing::Dispatch
where
    W: for<'a> tracing_subscriber::fmt::MakeWriter<'a> + Send + Sync + 'static,
{
    let base = tracing_subscriber::fmt()
        .with_env_filter(filter_from_env())
        .with_writer(writer)
        .with_target(true);
    if browser {
        tracing::Dispatch::new(base.without_time().with_ansi(false).finish())
    } else {
        tracing::Dispatch::new(base.finish())
    }
}

/// Install the stderr subscriber. Safe to call more than once (a test
/// harness, a second entry point): only the first install wins, later
/// calls are no-ops.
pub fn init() {
    let _ = dispatch(std::io::stderr, cfg!(target_arch = "wasm32")).try_init();
}
