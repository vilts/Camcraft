// Windows console plumbing for the GUI-subsystem camcraft-ui exe (#1278): attach to the
// parent's console with working std handles, and show start-up failures in a message box
// when there is no console. Spec: Microsoft's public Win32 documentation of AttachConsole,
// GetStdHandle/SetStdHandle, CreateFileW ("CONOUT$") and MessageBoxW.
//
// Declared through a few `extern "system"` items rather than a windows-sys dependency
// (that crate is only transitive here; a direct dependency is the maintainer's call).

use std::ffi::c_void;
use std::sync::atomic::{AtomicBool, Ordering};

type Handle = *mut c_void;

const ATTACH_PARENT_PROCESS: u32 = u32::MAX; // (DWORD)-1
const STD_OUTPUT_HANDLE: u32 = 0xFFFF_FFF5; // (DWORD)-11
const STD_ERROR_HANDLE: u32 = 0xFFFF_FFF4; // (DWORD)-12
const INVALID_HANDLE_VALUE: Handle = std::ptr::without_provenance_mut(usize::MAX); // (HANDLE)-1
const GENERIC_READ: u32 = 0x8000_0000;
const GENERIC_WRITE: u32 = 0x4000_0000;
const FILE_SHARE_READ: u32 = 0x0000_0001;
const FILE_SHARE_WRITE: u32 = 0x0000_0002;
const OPEN_EXISTING: u32 = 3;
const MB_OK: u32 = 0x0000_0000;
const MB_ICONERROR: u32 = 0x0000_0010;
const MB_SETFOREGROUND: u32 = 0x0001_0000;

#[link(name = "kernel32")]
unsafe extern "system" {
    fn AttachConsole(process_id: u32) -> i32;
    fn GetStdHandle(std_handle: u32) -> Handle;
    fn SetStdHandle(std_handle: u32, handle: Handle) -> i32;
    fn CreateFileW(
        file_name: *const u16,
        desired_access: u32,
        share_mode: u32,
        security_attributes: *const c_void,
        creation_disposition: u32,
        flags_and_attributes: u32,
        template_file: Handle,
    ) -> Handle;
}

#[link(name = "user32")]
unsafe extern "system" {
    fn MessageBoxW(hwnd: Handle, text: *const u16, caption: *const u16, kind: u32) -> i32;
}

/// True once `attach_parent_console` attached to a terminal's console.
static CONSOLE_ATTACHED: AtomicBool = AtomicBool::new(false);
/// True once an error box was shown, so a panic followed by a non-zero exit shows one box.
static ERROR_SHOWN: AtomicBool = AtomicBool::new(false);

/// NUL-terminated UTF-16 for the W APIs (an interior NUL only shortens the text).
fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

fn is_missing(handle: Handle) -> bool {
    handle.is_null() || handle == INVALID_HANDLE_VALUE
}

/// A GUI-subsystem exe starts with no console. Attach to the parent's console when there
/// is one (launched from a terminal); with none (double-click) the call fails and nothing
/// else happens. Attaching does not give the process standard handles, so each of
/// stdout/stderr that is NULL or invalid is pointed at the console's `CONOUT$`. A handle
/// that is already valid (`> log.txt 2>&1`, a pipe) is left alone. Rust's std writes
/// through `GetStdHandle` on every write, so `std::io::stderr` (the tracing writer) picks
/// up the new handle.
pub fn attach_parent_console() {
    // SAFETY: AttachConsole takes a plain integer and touches no Rust memory; failure is
    // reported by its return value.
    if unsafe { AttachConsole(ATTACH_PARENT_PROCESS) } == 0 {
        return;
    }
    CONSOLE_ATTACHED.store(true, Ordering::SeqCst);
    let conout = wide("CONOUT$");
    for id in [STD_OUTPUT_HANDLE, STD_ERROR_HANDLE] {
        // SAFETY: GetStdHandle takes a plain integer and returns a handle value or NULL.
        if !is_missing(unsafe { GetStdHandle(id) }) {
            continue;
        }
        // SAFETY: `conout` is a live NUL-terminated UTF-16 buffer for the whole call; the
        // security attributes and template are documented as optional (NULL).
        let handle = unsafe {
            CreateFileW(
                conout.as_ptr(),
                GENERIC_READ | GENERIC_WRITE,
                FILE_SHARE_READ | FILE_SHARE_WRITE,
                std::ptr::null(),
                OPEN_EXISTING,
                0,
                std::ptr::null_mut(),
            )
        };
        if is_missing(handle) {
            // Attached but no usable output: the log goes nowhere, so this counts as no
            // console and a failure still gets its box (#1325).
            CONSOLE_ATTACHED.store(false, Ordering::SeqCst);
            continue;
        }
        // SAFETY: plain integer plus a handle we own; the handle stays open for the
        // process lifetime as the new std handle. A failure leaves the old (missing) one.
        let _ = unsafe { SetStdHandle(id, handle) };
    }
}

/// Show `text` in an error message box, only when no console was attached (a terminal
/// launch already shows the log) and only once per process. Never panics.
fn show_error(text: &str) {
    if CONSOLE_ATTACHED.load(Ordering::SeqCst) || ERROR_SHOWN.swap(true, Ordering::SeqCst) {
        return;
    }
    let text = wide(text);
    let caption = wide("CamCraft");
    // SAFETY: both buffers are live NUL-terminated UTF-16 for the whole call; a NULL owner
    // window is allowed. The pressed button is irrelevant.
    let _ = unsafe {
        MessageBoxW(
            std::ptr::null_mut(),
            text.as_ptr(),
            caption.as_ptr(),
            MB_OK | MB_ICONERROR | MB_SETFOREGROUND,
        )
    };
}

/// `run` returned non-zero: with no console, nothing of the log is visible, so say so.
pub fn report_exit_failure(code: i32) {
    show_error(&format!(
        "CamCraft could not start or stopped with an error (exit code {code}).\n\n\
         Start camcraft-ui.exe from a terminal (Command Prompt or PowerShell) to see the log."
    ));
}

/// A panic escaped `camcraft_ui::run` (caught by `main`'s `catch_unwind`): show it in the
/// box when no console is attached. Handled panics inside the app never reach here (#1325).
pub fn report_panic(text: &str) {
    show_error(&format!(
        "CamCraft stopped unexpectedly.\n\n{text}\n\n\
         Start camcraft-ui.exe from a terminal (Command Prompt or PowerShell) to see the log."
    ));
}
