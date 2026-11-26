//! The UI crate's one diagnostics seam (#31, #163): a line for whoever
//! launched the app. Natively it is a `tracing` event (target
//! `camcraft_ui`), so it reaches the terminal through the one subscriber
//! `camcraft_app::logging::init` installs in `main` - with a level, a
//! timestamp and the `CAMCRAFT_LOG` / `RUST_LOG` filter, like every other
//! crate's lines. In a browser no subscriber prints, so on wasm32 the same
//! line goes to the page's console instead. UI code says `diag_warn!` /
//! `diag_info!` and never prints directly; the law test below keeps it
//! that way ("Log diagnostics, never silence").
//! Provenance: our own code, specced from the `tracing` macro docs and the
//! web-sys `console` API.

/// A warning or refusal: a `warn` event natively, `console.warn` in a browser.
pub fn warn(line: &str) {
    #[cfg(not(target_arch = "wasm32"))]
    tracing::warn!(target: "camcraft_ui", "{line}");
    #[cfg(target_arch = "wasm32")]
    eframe::web_sys::console::warn_1(&line.into());
}

/// A plain note: an `info` event natively, `console.log` in a browser.
pub fn info(line: &str) {
    #[cfg(not(target_arch = "wasm32"))]
    tracing::info!(target: "camcraft_ui", "{line}");
    #[cfg(target_arch = "wasm32")]
    eframe::web_sys::console::log_1(&line.into());
}

/// `eprintln!`'s shape over [`warn`].
macro_rules! diag_warn {
    ($($arg:tt)*) => { $crate::diag::warn(&format!($($arg)*)) };
}

/// `println!`'s shape over [`info`].
macro_rules! diag_info {
    ($($arg:tt)*) => { $crate::diag::info(&format!($($arg)*)) };
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    /// Files that may print directly: this seam and test-only code, whose
    /// lines must reach a test run's output with no subscriber installed.
    /// The native window, its GPU bring-up and the headless proof log
    /// through the seam like everything else (#163).
    const NATIVE_OR_TEST: &[&str] = &[
        "diag.rs",
        // gpu.rs prints only inside its #[cfg(test)] module; headless_gpu.rs
        // only in `device_for_test`, whose "SKIPPED - NOT a pass" line a
        // test run must show.
        "egui_app/gpu.rs",
        "headless_gpu.rs",
    ];

    const DIRECT_PRINTS: &[&str] = &["eprintln!(", "println!(", "eprint!(", "print!(", "dbg!("];

    fn rust_files(dir: &Path, out: &mut Vec<PathBuf>) {
        let Ok(entries) = std::fs::read_dir(dir) else { return };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                rust_files(&path, out);
            } else if path.extension().is_some_and(|e| e == "rs") {
                out.push(path);
            }
        }
    }

    #[test]
    fn shared_ui_code_prints_only_through_the_diag_seam() {
        let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let mut files = Vec::new();
        rust_files(&src, &mut files);
        assert!(files.len() > 50, "the source walk found too few files to mean anything");
        let mut offenders = Vec::new();
        for path in files {
            let rel = path.strip_prefix(&src).unwrap_or(&path).to_string_lossy().replace('\\', "/");
            let is_test = rel.ends_with("tests.rs") || rel.contains("/tests/") || rel.ends_with("test_harness.rs");
            if is_test || NATIVE_OR_TEST.contains(&rel.as_str()) {
                continue;
            }
            let text = std::fs::read_to_string(&path).unwrap_or_default();
            for (n, line) in text.lines().enumerate() {
                let code = line.split("//").next().unwrap_or("");
                if DIRECT_PRINTS.iter().any(|m| code.contains(m)) {
                    offenders.push(format!("{rel}:{}", n + 1));
                }
            }
        }
        assert!(
            offenders.is_empty(),
            "these lines print directly, past the log filter and dropped by a browser - use diag_warn!/diag_info!: {offenders:?}"
        );
    }

    /// The event levels of `warn` and `info` are pinned in
    /// `tests/diag_events.rs`, a test binary of its own (#2595).
    #[test]
    fn the_macros_take_println_shaped_arguments() {
        let n = 3;
        diag_warn!("diag test: a warning with {n} and {}", "an argument");
        diag_info!("diag test: a note");
    }
}
