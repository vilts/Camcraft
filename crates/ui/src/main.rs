// GUI entry point. Windows: built as a GUI-subsystem exe so a double-click opens
// no console window (#1278); the console and error-box plumbing is in win_console.
// A panic the app handles itself shows nothing; only one escaping `run` shows the
// box (#1325). Spec: Microsoft's public documentation of the PE subsystem flag and
// the Win32 console and message-box APIs; std::panic::catch_unwind.
#![cfg_attr(windows, windows_subsystem = "windows")]

mod panic_text;
#[cfg(windows)]
mod win_console;

fn main() {
    #[cfg(windows)]
    win_console::attach_parent_console();
    camcraft_app::logging::init();
    let args: Vec<String> = std::env::args().skip(1).collect();
    #[cfg(windows)]
    let code = match std::panic::catch_unwind(|| camcraft_ui::run(&args)) {
        Ok(code) => code,
        Err(payload) => {
            win_console::report_panic(&panic_text::payload_text(&*payload));
            101
        }
    };
    #[cfg(not(windows))]
    let code = camcraft_ui::run(&args);
    #[cfg(windows)]
    if code != 0 {
        win_console::report_exit_failure(code);
    }
    std::process::exit(code);
}
