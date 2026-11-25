//! Provenance: issue #3303 - a gate step whose process tree writes nothing for the stall time is killed
//! as a tree and named; output of the child or of a descendant keeps it alive.
//!
//! The children are this test executable itself, started on `watchdog_helper` with a mode in `HELPER`:
//! it exists on every platform that xtask builds on. The stall time goes in as a parameter (one or two
//! seconds), not through `CAMCRAFT_GATE_STALL_MIN`: that value is in whole minutes, and `set_var` is
//! unsafe in edition 2024 and races with the tests that run in parallel.

use std::fs::{self, File};
use std::io::Write;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use crate::watchdog::{self, End};

const HELPER: &str = "XTASK_WATCHDOG_HELPER";

/// The child side. Without `HELPER` it returns at once (an ordinary test run).
#[test]
fn watchdog_helper() {
    let Ok(mode) = std::env::var(HELPER) else { return };
    let me = std::env::current_exe().expect("exe");
    let spawn = |m: &str| helper(&me, m, Stdio::inherit(), Stdio::inherit()).expect("grandchild");
    match mode.as_str() {
        "silent" => std::thread::sleep(Duration::from_secs(120)),
        "tree" => {
            let mut g = spawn("silent");
            println!("grandchild pid {}", g.id());
            std::thread::sleep(Duration::from_secs(120));
            let _ = g.wait();
        }
        "tick" => {
            for i in 0..30 {
                println!("tick {i}");
                std::thread::sleep(Duration::from_millis(100));
            }
        }
        "quiet-parent" => {
            let mut g = spawn("tick");
            g.wait().expect("wait");
        }
        // As a build that starts the sccache server (#3675): a descendant that outlives the build holds
        // its stdout, and the build exits at once.
        "orphan" => {
            #[expect(clippy::zombie_processes, reason = "the descendant must outlive this process; the test kills it by pid")]
            let g = spawn("silent");
            println!("{{\"reason\":\"build-finished\",\"success\":true}}");
            println!("grandchild pid {}", g.id());
        }
        "fail" => {
            println!("{{\"reason\":\"build-finished\",\"success\":false}}");
            std::process::exit(3);
        }
        _ => {}
    }
}

fn helper(me: &PathBuf, mode: &str, out: Stdio, err: Stdio) -> std::io::Result<Child> {
    Command::new(me)
        .args(["--exact", "watchdog_tests::watchdog_helper", "--nocapture", "--test-threads=1"])
        .env(HELPER, mode)
        .stdin(Stdio::null())
        .stdout(out)
        .stderr(err)
        .spawn()
}

/// Starts the helper in `mode` with its output in a fresh log, and waits under the watchdog.
fn watched(mode: &str, limit: Duration) -> (End, String, Duration) {
    let dir = std::env::temp_dir().join(format!("xtask-watchdog-{mode}-{}", std::process::id()));
    fs::create_dir_all(&dir).expect("dir");
    let path = dir.join("step.log");
    let mut log = File::create(&path).expect("log");
    writeln!(log, "     Running helper {mode}").expect("write");
    let me = std::env::current_exe().expect("exe");
    let mut child = helper(&me, mode, log.try_clone().expect("clone").into(), log.try_clone().expect("clone").into()).expect("spawn");
    let t0 = Instant::now();
    let end = watchdog::wait(&mut child, &log, &|| 0, limit).expect("wait");
    let took = t0.elapsed();
    let text = fs::read_to_string(&path).expect("read");
    let _ = fs::remove_dir_all(&dir);
    (end, text, took)
}

#[test]
fn a_silent_step_is_killed_as_a_tree_and_named() {
    let (end, text, took) = watched("tree", Duration::from_secs(2));
    assert!(matches!(end, End::Stalled), "a silent tree must stall: {end:?}\n{text}");
    assert!(took < Duration::from_secs(60), "the watchdog must end the step soon after the stall time: {took:?}");
    let line = text.lines().find(|l| l.starts_with(watchdog::LINE)).expect("the log has the STALL line");
    let alive = line.split_once("alive: ").map(|(_, a)| a.split(", ").count()).unwrap_or(0);
    assert!(alive >= 2, "the child and the grandchild were alive: {line}");
    assert!(crate::select::log_failed(&text), "--failed must select a stalled step");
    let pid: u32 = text
        .lines()
        .find_map(|l| l.split_once("grandchild pid ").and_then(|(_, p)| p.trim().parse().ok()))
        .unwrap_or_else(|| panic!("the helper names its grandchild:
{text}"));
    // taskkill and kill return before the process is gone: give it a moment.
    let gone = (0..50).any(|_| {
        std::thread::sleep(Duration::from_millis(100));
        !watchdog::processes().iter().any(|p| p.0 == pid)
    });
    assert!(gone, "the grandchild {pid} must die with the tree");
    let note = watchdog::report("post suites: lib", &text);
    assert!(note.starts_with("stall: no output for 1min"), "{note}");
}

#[test]
fn output_of_the_child_or_a_descendant_keeps_the_step_alive() {
    for mode in ["tick", "quiet-parent"] {
        let (end, text, _) = watched(mode, Duration::from_secs(2));
        assert!(matches!(end, End::Exited(st) if st.success()), "{mode}: the step writes output, so it must run to its end: {end:?}\n{text}");
        assert!(!text.contains(watchdog::LINE), "{mode}: no STALL line:\n{text}");
    }
}

/// #3675: a recording build ends when cargo exits, also when a descendant that outlives it (the
/// sccache server) still holds its stdout. Before the fix the gate waited for that descendant, with no
/// watchdog: the helper's descendant sleeps 120 s, so the old code took 120 s or more here.
#[test]
fn a_recording_build_ends_when_cargo_exits_not_when_its_descendants_do() {
    let dir = json_dir("orphan");
    let path = crate::direct::json_path(&dir.join("01-step.log"));
    let t0 = Instant::now();
    let (code, json, stalled) = recording(None, "orphan", &dir, Some(&path)).expect("run");
    let took = t0.elapsed();
    let json = json.and_then(Result::ok).unwrap_or_default();
    let pid: Option<u32> = json.lines().find_map(|l| l.split_once("grandchild pid ").and_then(|(_, p)| p.trim().parse().ok()));
    if let Some(pid) = pid.map(|p| p.to_string()) {
        let (prog, kill) = if cfg!(windows) { ("taskkill", vec!["/F", "/PID", &pid]) } else { ("kill", vec!["-KILL", &pid]) };
        let _ = Command::new(prog).args(kill).stdout(Stdio::null()).stderr(Stdio::null()).status();
    }
    let removed = !path.exists();
    let _ = fs::remove_dir_all(&dir);
    assert!(took < Duration::from_secs(30), "the step must end when cargo exits: {took:?}");
    assert!(!stalled && code == 0, "the build exits green: code {code}, stalled {stalled}");
    assert!(json.contains("\"reason\":\"build-finished\""), "the JSON of the build comes back: {json}");
    assert!(pid.is_some(), "the helper names its descendant: {json}");
    assert!(removed, "the JSON file is removed after a green build: {}", path.display());
}

/// #3675 review: the JSON file goes away after a red build, and when cargo cannot start (an early return).
#[test]
fn the_json_file_is_removed_after_a_failed_build_and_a_failed_start() {
    let dir = json_dir("removed");
    let path = dir.join("02-step.json");
    let (code, json, _) = recording(None, "fail", &dir, Some(&path)).expect("run");
    let red_removed = !path.exists();
    let start = recording(Some(&dir.join("no-such-cargo.exe")), "", &dir, Some(&path));
    let start_removed = !path.exists();
    let _ = fs::remove_dir_all(&dir);
    assert!(code == 3, "the red build's code comes back: {code}");
    assert!(json.is_some_and(|j| j.is_ok_and(|t| t.contains("\"success\":false"))), "the JSON of a red build still comes back");
    assert!(red_removed, "the JSON file is removed after a red build");
    assert!(start.is_err(), "a cargo that cannot start is an error, as before");
    assert!(start_removed, "the JSON file is removed on the early return");
}

/// #3675 review: an I/O error of the JSON file is "nothing recorded", not a gate error.
#[test]
fn an_io_error_of_the_json_file_records_nothing_and_the_build_still_runs() {
    let dir = json_dir("io");
    let path = dir.join("missing-dir").join("03-step.json");
    let (code, json, stalled) = recording(None, "", &dir, Some(&path)).expect("an I/O error is not a gate error");
    let _ = fs::remove_dir_all(&dir);
    assert!(code == 0 && !stalled, "the build runs to its end: code {code}");
    let why = json.and_then(Result::err).unwrap_or_default();
    assert!(why.starts_with("create ") && why.contains("03-step.json"), "the reason names the file: {why}");
}

/// #3675 review: an entry that is at the name already (a stale file, a planted link) is not followed,
/// not written and not removed.
#[test]
fn an_entry_at_the_json_name_is_not_followed() {
    let dir = json_dir("exists");
    let path = dir.join("04-step.json");
    fs::write(&path, "keep").expect("seed");
    let (code, json, _) = recording(None, "", &dir, Some(&path)).expect("run");
    let kept = fs::read_to_string(&path).unwrap_or_default();
    let _ = fs::remove_dir_all(&dir);
    assert_eq!(code, 0);
    assert!(json.is_some_and(|j| j.is_err()), "nothing is recorded from a file that was there before");
    assert_eq!(kept, "keep", "the entry at the name is not written and not removed");
}

fn json_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("xtask-json-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("dir");
    dir
}

/// A recording build of the helper in `mode` (or of the program `prog`) with its JSON at `json`.
fn recording(prog: Option<&std::path::Path>, mode: &str, dir: &std::path::Path, json: Option<&std::path::Path>) -> Result<crate::direct::Watched, String> {
    let log = File::create(dir.join("step.log")).expect("log");
    let me = std::env::current_exe().expect("exe");
    let prog = prog.unwrap_or(&me).to_string_lossy().into_owned();
    let args: Vec<String> = ["--exact", "watchdog_tests::watchdog_helper", "--nocapture", "--test-threads=1"].map(String::from).to_vec();
    crate::direct::cargo(&prog, vec![(HELPER, mode.into())], &args, dir, log, json, Duration::from_secs(60))
}

#[test]
fn the_stall_time_reads_as_the_job_budget_does() {
    assert_eq!(watchdog::from_value(None).0, watchdog::DEFAULT_MIN);
    assert_eq!(watchdog::from_value(Some(" ")).0, watchdog::DEFAULT_MIN);
    let (n, line) = watchdog::from_value(Some("30"));
    assert!(n == 30 && line.contains("CAMCRAFT_GATE_STALL_MIN=30"), "{line}");
    for bad in ["0", "-3", "ten", "1.5"] {
        let (n, line) = watchdog::from_value(Some(bad));
        assert_eq!(n, watchdog::DEFAULT_MIN, "{bad}");
        assert!(line.starts_with("note: ") && line.contains(watchdog::ENV) && line.contains(bad), "{line}");
    }
}

#[test]
fn release_and_ignored_steps_get_the_longer_stall_time() {
    let args = |a: &[&str]| a.iter().map(|s| s.to_string()).collect::<Vec<_>>();
    assert_eq!(watchdog::limit_for(&args(&["test", "-p", "x", "--lib"]), 15), Duration::from_secs(15 * 60));
    assert_eq!(watchdog::limit_for(&args(&["test", "--release", "-p", "x"]), 15), Duration::from_secs(60 * 60));
    assert_eq!(watchdog::limit_for(&args(&["test", "-p", "x", "--", "--ignored"]), 15), Duration::from_secs(60 * 60));
    assert_eq!(watchdog::limit_for(&args(&["--release"]), u64::MAX), Duration::from_secs(u64::MAX), "no overflow");
}

#[test]
fn a_stall_has_its_own_row_and_exit_code() {
    assert_eq!(crate::exec::row(true, false), "ok");
    assert_eq!(crate::exec::row(false, false), "FAIL");
    assert_eq!(crate::exec::row(false, true), "STALL");
    assert_eq!(watchdog::exit_code(false, 0, 0), 0);
    assert_eq!(watchdog::exit_code(false, 0, 2), watchdog::EXIT);
    assert_eq!(watchdog::exit_code(false, 1, 2), 1, "a FAIL row wins: main is red");
    assert_eq!(watchdog::exit_code(true, 0, 1), 1, "a void run stays 1");
    assert_ne!(watchdog::EXIT, 1);
}

#[test]
fn a_stall_report_without_the_watchdog_line_still_names_the_step() {
    let note = watchdog::report("post suites: lib", "running 1 test
");
    assert!(note.starts_with("stall: no output for ?min"), "{note}");
}

#[test]
fn the_process_tree_follows_the_parent_links() {
    let all = watchdog::parse_processes("  1 0 init\n 10 1 cargo\n 11 10 rustc\n 12 11 my test exe\n 13 1 other\nbad line\n");
    let tree = watchdog::descendants(10, &all);
    assert_eq!(tree, vec![(10, "cargo".into()), (11, "rustc".into()), (12, "my test exe".into())]);
}
