//! The stall watchdog of a gate step (#3303): a step whose process tree writes no output for the stall
//! time is killed as a tree, named in a `STALL` line and recorded as a STALL row, so a hung step ends by
//! itself and does not look like a red main. Provenance: our own build law (AGENTS.md "Three gate
//! tiers"); POSIX `ps` and `kill`; Microsoft's documented `taskkill /T` and the `Win32_Process` class.

use std::fs::File;
use std::io::Write;
use std::process::{Child, Command, ExitStatus, Stdio};
use std::time::{Duration, Instant};

/// The environment variable that overrides the stall time, in whole minutes (1 or more).
pub const ENV: &str = "CAMCRAFT_GATE_STALL_MIN";
/// The stall time of a debug step, in minutes, when the environment does not set one.
pub const DEFAULT_MIN: u64 = 15;
/// A `--release` or `--ignored` step (a release link, a heavy test) may be quiet longer: this many times the stall time.
pub const SLOW_FACTOR: u64 = 4;
/// The start of the log line that the watchdog writes when it kills a step. It starts with `error:`, so
/// `--failed` selects the step again (`select::log_failed`).
pub const LINE: &str = "error: gate watchdog: STALL ";
/// The exit code of a gate with a STALL row and no FAIL row: not 0 (no pass), not 1 (not a red main).
pub const EXIT: i32 = 3;

/// The stall time in minutes from the environment value, and a line for the plan output. A bad value
/// uses the default and says so, as `budget::from_cpu_value` does for `CAMCRAFT_GATE_JOBS`.
pub fn from_value(v: Option<&str>) -> (u64, String) {
    let tail = format!("({}x for --release and --ignored steps)", SLOW_FACTOR);
    match v.map(str::trim) {
        None | Some("") => (DEFAULT_MIN, format!("gate stall watchdog: {DEFAULT_MIN} min with no step output {tail}; source: default")),
        Some(s) => match s.parse::<u64>() {
            Ok(n) if n >= 1 => (n, format!("gate stall watchdog: {n} min with no step output {tail}; source: {ENV}={n}")),
            _ => (
                DEFAULT_MIN,
                format!("note: {ENV}={s} is not a whole number of 1 or more; gate stall watchdog: {DEFAULT_MIN} min with no step output {tail}"),
            ),
        },
    }
}

/// The stall time of this process, in minutes, and its plan line.
pub fn from_env() -> (u64, String) {
    from_value(std::env::var(ENV).ok().as_deref())
}

/// The stall time of a step with `args` when the base stall time is `minutes`.
pub fn limit_for(args: &[String], minutes: u64) -> Duration {
    let slow = args.iter().any(|a| a == "--release" || a == "--ignored");
    let m = if slow { minutes.saturating_mul(SLOW_FACTOR) } else { minutes };
    Duration::from_secs(m.saturating_mul(60))
}

/// How a watched process ended.
#[derive(Debug)]
pub enum End {
    Exited(ExitStatus),
    /// No output for the stall time: the tree was killed. The log line names the processes that were alive.
    Stalled,
}

/// Waits for `child`. Output is the growth of `log` (the child and each descendant that inherits it
/// write there) plus `extra()` (bytes of a piped stdout). When neither grows for `limit`, the process
/// tree is killed, the `LINE` is written to `log`, and the result is `Stalled`.
pub fn wait(child: &mut Child, log: &File, extra: &dyn Fn() -> u64, limit: Duration) -> std::io::Result<End> {
    let poll = (limit / 4).clamp(Duration::from_millis(50), Duration::from_secs(1));
    let seen = || log.metadata().map(|m| m.len()).unwrap_or(0).saturating_add(extra());
    let (mut last, mut quiet_since) = (seen(), Instant::now());
    loop {
        if let Some(st) = child.try_wait()? {
            return Ok(End::Exited(st));
        }
        std::thread::sleep(poll);
        let now = seen();
        if now != last {
            (last, quiet_since) = (now, Instant::now());
        } else if quiet_since.elapsed() >= limit {
            let tree = descendants(child.id(), &processes());
            kill_tree(child, &tree);
            let _ = child.wait();
            let alive: Vec<String> = tree.iter().map(|(pid, name)| format!("{name} ({pid})")).collect();
            let mins = limit.as_secs().div_ceil(60);
            let mut w = log;
            // A new line first: the step may have stopped in the middle of a line (libtest's `test x ... `).
            writeln!(w, "\n{LINE}{mins}min: no output; killed the process tree; alive: {}", alive.join(", "))?;
            return Ok(End::Stalled);
        }
    }
}

/// Every process of the machine as `(pid, parent pid, name)`; empty when the list cannot be read.
pub fn processes() -> Vec<(u32, u32, String)> {
    let out = if cfg!(windows) {
        let q = "Get-CimInstance Win32_Process | ForEach-Object { \"$($_.ProcessId) $($_.ParentProcessId) $($_.Name)\" }";
        Command::new("powershell").args(["-NoProfile", "-NonInteractive", "-Command", q]).stdin(Stdio::null()).output()
    } else {
        Command::new("ps").args(["-A", "-o", "pid=", "-o", "ppid=", "-o", "comm="]).stdin(Stdio::null()).output()
    };
    let Ok(out) = out else { return Vec::new() };
    parse_processes(&String::from_utf8_lossy(&out.stdout))
}

/// Lines of `pid ppid name` (the name can have spaces).
pub fn parse_processes(text: &str) -> Vec<(u32, u32, String)> {
    let mut all = Vec::new();
    for l in text.lines() {
        let mut it = l.split_whitespace();
        let (Some(pid), Some(ppid)) = (it.next().and_then(|p| p.parse().ok()), it.next().and_then(|p| p.parse().ok())) else { continue };
        let name: Vec<&str> = it.collect();
        all.push((pid, ppid, name.join(" ")));
    }
    all
}

/// `root` and its descendants in `all`, root first, as `(pid, name)`.
pub fn descendants(root: u32, all: &[(u32, u32, String)]) -> Vec<(u32, String)> {
    let name = |pid: u32| all.iter().find(|p| p.0 == pid).map(|p| p.2.clone()).unwrap_or_else(|| "?".into());
    let mut out = vec![(root, name(root))];
    let mut i = 0;
    while let Some(&(pid, _)) = out.get(i) {
        for p in all.iter().filter(|p| p.1 == pid && p.0 != pid) {
            if !out.iter().any(|o| o.0 == p.0) {
                out.push((p.0, p.2.clone()));
            }
        }
        i += 1;
    }
    out
}

/// Kills `child` and the `tree` that was read before: `taskkill /F /T` on Windows, `kill -KILL` elsewhere.
fn kill_tree(child: &mut Child, tree: &[(u32, String)]) {
    let pids: Vec<String> = tree.iter().map(|(p, _)| p.to_string()).collect();
    let quiet = |c: &mut Command| c.stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).status();
    if cfg!(windows) {
        let _ = quiet(Command::new("taskkill").args(["/F", "/T", "/PID", &child.id().to_string()]));
    } else {
        let _ = quiet(Command::new("kill").arg("-KILL").args(&pids));
    }
    let _ = child.kill();
}

/// For a step that the watchdog killed (`End::Stalled`; never decided from the log text): prints
/// `STALL <minutes>min <step>`, the last lines the step wrote and the processes that were alive, from the
/// watchdog's last line in the log `text`, and gives the step note.
pub fn report(step: &str, text: &str) -> String {
    let lines: Vec<&str> = text.lines().collect();
    let at = lines.iter().rposition(|l| l.starts_with(LINE)).unwrap_or(lines.len());
    let rest = lines.get(at).and_then(|l| l.strip_prefix(LINE)).unwrap_or("?min: no watchdog line in the log");
    let (mins, alive) = rest.split_once(':').unwrap_or((rest, ""));
    println!("STALL {mins} {step}");
    println!("     last lines of the step:");
    lines[at.saturating_sub(20)..at].iter().for_each(|l| println!("     | {l}"));
    let alive = alive.split_once("alive: ").map(|(_, a)| a).unwrap_or("");
    println!("     processes alive at the kill: {alive}");
    format!("stall: no output for {mins}; process tree killed (alive: {alive})")
}

/// The gate's exit code: 1 for a void run or any FAIL row, `EXIT` for STALL rows only, else 0.
pub fn exit_code(void: bool, failed: usize, stalled: usize) -> i32 {
    if void || failed > 0 {
        1
    } else if stalled > 0 {
        EXIT
    } else {
        0
    }
}
