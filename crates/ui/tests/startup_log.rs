//! #2421: a console-less desktop launch leaves durable startup diagnostics.
//! Provenance: our own subprocess fixtures over the public desktop entry point.
//! Gate map: covers camcraft_ui::run, app::logging.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

struct ConfigDir(PathBuf);

impl ConfigDir {
    fn new(tag: &str) -> Self {
        let path = std::env::temp_dir().join(format!("camcraft-startup-log-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).expect("isolated config directory");
        Self(path)
    }

    fn log(&self) -> PathBuf {
        self.0.join("camcraft.log")
    }

    fn text(&self) -> String {
        std::fs::read_to_string(self.log()).expect("desktop launch must leave camcraft.log without a terminal")
    }
}

impl Drop for ConfigDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn desktop(config: &Path, args: &[&str], filter: &str) -> Output {
    Command::new(env!("CARGO_BIN_EXE_camcraft-ui"))
        .args(args)
        .env("CAMCRAFT_CONFIG_DIR", config)
        .env("CAMCRAFT_LOG", filter)
        .env_remove("CAMCRAFT_GPU_BACKEND")
        .env_remove("CAMCRAFT_GPU")
        .output()
        .expect("desktop process starts")
}

#[test]
fn desktop_startup_error_survives_without_a_terminal() {
    let config = ConfigDir::new("startup-error");
    let result = desktop(&config.0, &["--gpu-backend", "invalid-test-backend"], "info");
    assert_eq!(result.status.code(), Some(2), "{result:?}");
    let text = config.text();
    assert!(text.contains("invalid-test-backend"), "the refusal names the bad backend: {text}");
    assert!(!text.contains('\u{1b}'), "the file must contain plain text: {text:?}");
    assert!(text.contains(&config.log().display().to_string()), "the failure names its log location: {text}");
    for retry in ["--gpu-backend dx12", "--gpu-backend vulkan", "--gpu-backend gl"] {
        assert!(text.contains(retry), "missing retry hint {retry}: {text}");
    }
    assert!(text.contains("backtrace"), "the failure keeps a backtrace: {text}");
}

#[test]
fn desktop_startup_error_is_persisted_when_terminal_logging_is_off() {
    let config = ConfigDir::new("filter-off");
    let result = desktop(&config.0, &["--gpu-backend", "off-test-backend"], "off");
    assert_eq!(result.status.code(), Some(2), "{result:?}");
    let text = config.text();
    assert!(text.contains("off-test-backend"), "terminal verbosity must not hide crash diagnostics: {text}");
}

#[test]
fn desktop_startup_log_preserves_the_previous_run() {
    let config = ConfigDir::new("append");
    std::fs::write(config.log(), "previous run diagnostic\n").expect("previous log");
    let result = desktop(&config.0, &["--gpu-backend", "append-test-backend"], "info");
    assert_eq!(result.status.code(), Some(2), "{result:?}");
    let text = config.text();
    assert!(text.starts_with("previous run diagnostic\n"), "do not truncate evidence from a hard lock: {text}");
    assert!(text.contains("append-test-backend"), "the current run is appended before exit: {text}");
}

#[test]
fn desktop_log_open_failure_keeps_a_named_terminal_diagnostic() {
    let config = ConfigDir::new("unavailable");
    let occupied = config.0.join("occupied");
    std::fs::write(&occupied, "a file cannot contain a log").expect("occupied config path");
    let result = desktop(&occupied, &["--gpu-backend", "unavailable-test-backend"], "info");
    assert_eq!(result.status.code(), Some(2), "a log setup failure must not panic: {result:?}");
    let text = String::from_utf8_lossy(&result.stderr);
    assert!(text.contains("cannot open desktop log"), "the fallback names the failure: {text}");
    assert!(text.contains(&occupied.display().to_string()), "the fallback names the path: {text}");
    assert!(text.contains("unavailable-test-backend"), "the original startup refusal remains visible: {text}");
}

#[test]
fn desktop_panic_keeps_its_message_and_backtrace() {
    let config = ConfigDir::new("panic");
    let result = Command::new(std::env::current_exe().expect("test executable"))
        .args(["--exact", "desktop_panic_child", "--nocapture"])
        .env("CAMCRAFT_TEST_PANIC_CHILD", "1")
        .env("CAMCRAFT_CONFIG_DIR", &config.0)
        .env("CAMCRAFT_LOG", "off")
        .output()
        .expect("panic child starts");
    assert!(result.status.success(), "the child catches its thread panic: {result:?}");
    let text = config.text();
    assert!(text.contains("2421 forced startup panic"), "panic message persisted: {text}");
    assert!(text.contains("backtrace"), "panic backtrace persisted: {text}");
    assert!(text.contains("gpu: seen synthetic adapter via vulkan"), "GPU startup info survives the terminal filter: {text}");
    assert_eq!(std::fs::read_to_string(config.0.join("previous-hook.txt")).expect("previous panic hook runs"), "called");
}

#[test]
fn desktop_panic_child() {
    if std::env::var_os("CAMCRAFT_TEST_PANIC_CHILD").is_none() {
        return;
    }
    let marker = PathBuf::from(std::env::var_os("CAMCRAFT_CONFIG_DIR").expect("child config"))
        .join("previous-hook.txt");
    std::panic::set_hook(Box::new(move |_| {
        std::fs::write(&marker, "called").expect("previous hook marker");
    }));
    camcraft_app::logging::init_desktop();
    camcraft_app::logging::init_desktop();
    tracing::info!(target: "camcraft_ui", "gpu: seen synthetic adapter via vulkan");
    assert!(std::thread::spawn(|| panic!("2421 forced startup panic")).join().is_err());
}
