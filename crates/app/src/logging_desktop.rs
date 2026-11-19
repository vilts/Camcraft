//! Durable desktop diagnostics for console-less launches (#2421).
//! Provenance: our own wiring over std file/panic APIs and tracing-subscriber layers.

use std::fs::{File, OpenOptions};
use std::io::{self, Write};
use std::path::PathBuf;
use std::sync::{Arc, Mutex, OnceLock};

use tracing_subscriber::prelude::*;

type Sink = Arc<Mutex<File>>;
static DESKTOP: OnceLock<Result<Sink, String>> = OnceLock::new();

/// The existing platform config folder, or a temporary folder when no home resolves.
pub fn log_path() -> PathBuf {
    crate::recent::config_dir_for(std::env::consts::OS, |key| std::env::var_os(key))
        .unwrap_or_else(|| std::env::temp_dir().join("camcraft"))
        .join("camcraft.log")
}

fn append(sink: &Sink, bytes: &[u8]) -> io::Result<()> {
    let mut file = sink.lock().map_err(|_| io::Error::other("desktop log lock poisoned"))?;
    file.write_all(bytes)?;
    // No shutdown guard: main uses process::exit and a driver hang may prevent shutdown.
    file.sync_data()
}

fn report_write(sink: &Sink, bytes: &[u8]) {
    if let Err(err) = append(sink, bytes) {
        let _ = writeln!(io::stderr(), "cannot write desktop log {}: {err}", log_path().display());
    }
}

/// Buffer one formatted event, then append and synchronize it before returning.
struct Record {
    sink: Sink,
    bytes: Vec<u8>,
}

impl Write for Record {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.bytes.extend_from_slice(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl Drop for Record {
    fn drop(&mut self) {
        report_write(&self.sink, &self.bytes);
    }
}

fn open_log() -> Result<Sink, String> {
    let path = log_path();
    let opened = (|| -> io::Result<File> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        OpenOptions::new().create(true).append(true).open(&path)
    })();
    let sink = Arc::new(Mutex::new(opened.map_err(|err| format!("cannot open desktop log {}: {err}", path.display()))?));
    let file_sink = Arc::clone(&sink);
    let file = tracing_subscriber::fmt::layer()
        .with_ansi(false)
        .with_writer(move || Record { sink: Arc::clone(&file_sink), bytes: Vec::new() })
        // Terminal verbosity must never hide the startup evidence in the file.
        .with_filter(tracing_subscriber::EnvFilter::new(super::DEFAULT_FILTER));
    let terminal = tracing_subscriber::fmt::layer()
        .with_writer(io::stderr)
        .with_filter(super::filter_from_env());
    if let Err(err) = tracing_subscriber::registry().with(terminal).with(file).try_init() {
        report_write(&sink, format!("desktop tracing subscriber not installed: {err}\n").as_bytes());
    }
    let panic_sink = Arc::clone(&sink);
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let text = format!("{}\nbacktrace:\n{}\n", failure_message(&format!("panic: {info}")), std::backtrace::Backtrace::force_capture());
        report_write(&panic_sink, text.as_bytes());
        previous(info);
    }));
    append(&sink, format!("CamCraft desktop startup (pid {}); log file: {}\n", std::process::id(), path.display()).as_bytes())
        .map_err(|err| format!("cannot synchronize desktop log {}: {err}", path.display()))?;
    Ok(sink)
}

/// Install file and terminal layers once. The CLI keeps the stderr-only initializer.
pub fn init_desktop() {
    if let Err(err) = DESKTOP.get_or_init(open_log) {
        super::init();
        let _ = writeln!(io::stderr(), "{err}");
    }
}

/// Crash dialogs and terminal failures share the same location and recovery hints.
pub fn failure_message(message: &str) -> String {
    let location = match DESKTOP.get() {
        Some(Err(err)) => format!("Log unavailable: {err}"),
        _ => format!("Log file: {}", log_path().display()),
    };
    format!("{message}\n\n{location}\nTry --gpu-backend dx12, --gpu-backend vulkan or --gpu-backend gl.\nUse --gpu <adapter name> to choose an adapter.")
}

/// Persist a returned startup error with a forced backtrace, independent of filters.
pub fn startup_failure(message: &str) {
    let text = format!("{}\nbacktrace:\n{}\n", failure_message(message), std::backtrace::Backtrace::force_capture());
    if let Some(Ok(sink)) = DESKTOP.get() {
        report_write(sink, text.as_bytes());
    }
    let _ = writeln!(io::stderr(), "{}", failure_message(message));
}
