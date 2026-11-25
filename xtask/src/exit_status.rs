//! How the process of a gate step stopped, in words: the signal on Unix, the crash status on Windows.
//! A test executable that a signal kills has no exit code and writes no panic, so the step log showed
//! only `exit -1` (#3258). Provenance: POSIX `signal.h` numbers; Microsoft's published NTSTATUS values.

use std::process::ExitStatus;

/// The start of the log line that says how a test executable stopped without an exit code of its own.
pub const STOPPED: &str = "gate: the test executable stopped:";

/// The gate's code of `st` (-1 when the process has no exit code) and, for a crash, how it stopped.
pub fn read(st: ExitStatus) -> (i32, Option<String>) {
    #[cfg(unix)]
    {
        use std::os::unix::process::ExitStatusExt;
        if let Some(sig) = st.signal() {
            let core = if st.core_dumped() { ", core dumped" } else { "" };
            return (-1, Some(format!("killed by signal {sig} ({}){core}", signal_name(sig))));
        }
    }
    match st.code() {
        Some(c) => (c, crash_name(c).map(|n| format!("status 0x{:08X} ({n})", c as u32))),
        None => (-1, Some("no exit code and no signal".into())),
    }
}

/// The name of a signal whose number is the same on Linux and macOS.
#[cfg_attr(not(unix), allow(dead_code))]
pub fn signal_name(sig: i32) -> &'static str {
    match sig {
        1 => "SIGHUP",
        2 => "SIGINT",
        4 => "SIGILL",
        6 => "SIGABRT",
        8 => "SIGFPE",
        9 => "SIGKILL",
        11 => "SIGSEGV",
        13 => "SIGPIPE",
        15 => "SIGTERM",
        _ => "no portable name",
    }
}

/// The meaning of a Windows exit code that is a crash status, not a code that the program chose.
pub fn crash_name(code: i32) -> Option<&'static str> {
    match code as u32 {
        0xC000_0005 => Some("access violation"),
        0xC000_001D => Some("illegal instruction"),
        0xC000_0094 => Some("integer division by zero"),
        0xC000_00FD => Some("stack overflow"),
        0xC000_0135 => Some("a DLL was not found"),
        0xC000_0374 => Some("heap corruption"),
        0xC000_0409 => Some("fast fail: an abort or a stack buffer overrun"),
        0xC000_013A => Some("stopped by Ctrl+C"),
        0xC000_0142 => Some("a DLL did not initialize"),
        _ => None,
    }
}

/// The step note of a red step with no failed test and no compile error: the code, and how the
/// process stopped when the log has a `STOPPED` line.
pub fn note(code: i32, log: &str) -> String {
    match log.lines().rev().find_map(|l| l.strip_prefix(STOPPED)) {
        Some(how) => format!("exit {code} ({})", how.trim()),
        None => format!("exit {code}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(unix)]
    #[test]
    fn a_signal_death_names_the_signal_and_the_core_dump() {
        use std::os::unix::process::ExitStatusExt;
        // The raw wait status of a process killed by SIGSEGV (11) that dumped core (0x80).
        let (code, how) = read(ExitStatus::from_raw(11 | 0x80));
        assert_eq!(code, -1);
        assert_eq!(how.as_deref(), Some("killed by signal 11 (SIGSEGV), core dumped"));
        let (code, how) = read(ExitStatus::from_raw(9));
        assert_eq!((code, how.as_deref()), (-1, Some("killed by signal 9 (SIGKILL)")));
    }

    #[cfg(windows)]
    #[test]
    fn a_crash_status_names_the_crash() {
        use std::os::windows::process::ExitStatusExt;
        let (code, how) = read(ExitStatus::from_raw(0xC000_0005));
        assert_eq!(code, 0xC000_0005_u32 as i32);
        assert_eq!(how.as_deref(), Some("status 0xC0000005 (access violation)"));
        let (_, how) = read(ExitStatus::from_raw(0xC000_0374));
        assert_eq!(how.as_deref(), Some("status 0xC0000374 (heap corruption)"));
    }

    #[test]
    fn a_plain_exit_code_has_no_crash_words() {
        assert_eq!(crash_name(101), None);
        assert_eq!(crash_name(1), None);
        assert_eq!(signal_name(64), "no portable name");
    }

    /// The pin through the gate's own runner: a step whose executable crashes writes how it stopped
    /// into its log, and the step note says it. Before #3258 the note was only `exit -1`.
    #[test]
    fn a_crashed_step_executable_says_how_it_stopped_in_its_log_and_note() {
        use crate::artifacts::TestExe;
        use crate::direct::{run, Direct};
        // Reserve a fresh directory with an atomic create_dir: a shared /tmp can hold an old one of
        // the same PID that another runner account owns (#3290).
        let base = std::env::temp_dir();
        let dir = (0..1000)
            .map(|n| base.join(format!("camcraft-xtask-3258-{}-{n}", std::process::id())))
            .find(|d| match std::fs::create_dir(d) {
                Ok(()) => true,
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => false,
                Err(e) => panic!("temp dir {}: {e}", d.display()),
            })
            .expect("a free temp dir");
        let (exe, args, want) = if cfg!(windows) {
            ("cmd", vec!["/c".to_string(), "exit -1073741819".to_string()], "(status 0xC0000005 (access violation))")
        } else {
            ("sh", vec!["-c".to_string(), "kill -SEGV $$".to_string()], "(killed by signal 11 (SIGSEGV)")
        };
        let d = Direct { exes: vec![TestExe { exe: exe.into(), label: "crash".into() }], cwd: dir.clone(), args, env: Vec::new() };
        let log_path = dir.join("step.log");
        let code = run(&d, &dir, std::fs::File::create(&log_path).expect("log"), std::time::Duration::from_secs(600)).expect("run").0;
        let text = std::fs::read_to_string(&log_path).expect("read log");
        let _ = std::fs::remove_dir_all(&dir);
        assert_ne!(code, 0, "{text}");
        assert!(text.contains(STOPPED), "no stop line in: {text}");
        let note = crate::exec::read_log(&text, code).expect("note");
        assert!(note.contains(want), "note {note:?} lacks {want:?}");
    }

    #[test]
    fn the_note_carries_how_the_executable_stopped() {
        let log = "running 8 tests\ntest a ... ok\ngate: the test executable stopped: killed by signal 11 (SIGSEGV)\n";
        assert_eq!(note(-1, log), "exit -1 (killed by signal 11 (SIGSEGV))");
        assert_eq!(note(-1, "running 8 tests\ntest a ... ok\n"), "exit -1");
    }
}
