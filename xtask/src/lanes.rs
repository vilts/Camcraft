//! Provenance: bounded release-gate lanes requested in issue #1703, sized by the job budget (#2057); the
//! longest-processing-time-first queue is Graham's LPT list-scheduling rule (R. L. Graham, "Bounds on
//! multiprocessing timing anomalies", SIAM J. Appl. Math. 17, 1969). Standard Rust scoped threads.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::plan::Step;

/// The app and kernel test targets that must not share the machine with another suite, as
/// `crate-dir:target` (`lib` for `--lib`, else the integration binary's name). Each holds a wall-clock
/// assertion that a busy neighbour could trip: a thread watchdog that panics when an import does not
/// answer within N seconds (`recv_timeout`), a polling loop with a deadline, or an `elapsed() < limit`
/// assert. Found by grepping the suites for `recv_timeout`, `deadline`, `Instant`/`elapsed` and
/// `Duration::from_secs` (2026-11-17); suites that only print a timing are not listed. A new suite with a
/// wall-clock assertion is added here; one listed by mistake only costs speed.
pub const ISOLATED: &[&str] = &[
    // app: watchdogs and wall-clock asserts in the lib's own tests (step_guard_tests, step_division_tests).
    "app:lib",
    // app: STEP import watchdogs (`within(limit, ..)` panics on a timeout).
    "app:import_step_closed_shell",
    "app:import_step_corner_blend",
    "app:import_step_curve_guard",
    "app:import_step_guard",
    "app:import_step_guard_default_cap",
    "app:import_step_open_shell",
    // app: `elapsed() < limit` asserts and deadline loops.
    "app:large_models",           // streaming import < 60 s (heavy)
    "app:mcp_live",               // TCP client/listener deadlines of 0.3-20 s
    "app:shoulder_adaptive",      // band plans in < 60 s
    "app:step_nesting",           // refusal in < 1 s
    "app:stock_field_cost",       // per-program cost budget (heavy)
    "app:stock_play",             // worker publishes within 60 s
    "app:stock_play_partial",     // worker publishes within 60 s
    "app:stock_play_precalc",     // precalc finishes within 120 s
    // kernel: lib (solidcore_step's guarded import) and the suites on step_util / smb_util watchdogs.
    "kernel:lib",
    "kernel:heal_import",
    "kernel:import_corpus",
    "kernel:mesh_deviation_bench",
    "kernel:solidcraft_hostile",
    "kernel:solidcraft_import",
    "kernel:step_agreement",
    "kernel:step_import",
];

/// The `crate-dir:target` key of a step that runs one target of one camcraft crate (`-p camcraft-x` with
/// `--lib`, `--bins`, `--doc` or `--test <name>`); `None` for anything else (a whole crate, several crates).
pub fn target_key(s: &Step) -> Option<String> {
    let a = &s.args;
    let end = a.iter().position(|x| x == "--").unwrap_or(a.len());
    let a = &a[..end];
    let mut pkgs = a.windows(2).filter(|w| w[0] == "-p").map(|w| w[1].as_str());
    let dir = pkgs.next()?.strip_prefix("camcraft-")?;
    if pkgs.next().is_some() {
        return None;
    }
    let target = if a.iter().any(|x| x == "--lib") {
        "lib".to_string()
    } else if a.iter().any(|x| x == "--bins") {
        "bins".to_string()
    } else if a.iter().any(|x| x == "--doc") {
        "doc".to_string()
    } else {
        a.windows(2).find(|w| w[0] == "--test").map(|w| w[1].clone())?
    };
    Some(format!("{dir}:{target}"))
}

pub fn is_build(s: &Step) -> bool {
    s.args.iter().any(|a| a == "--no-run")
}

/// Runs with the whole budget and no neighbour: builds, clippy, every UI step (windows), an app or kernel
/// step that is not one named target (a whole-crate run), and the `ISOLATED` targets.
pub fn alone(s: &Step) -> bool {
    if s.in_ui || s.args.iter().any(|a| matches!(a.as_str(), "--no-run" | "clippy" | "camcraft-ui")) {
        return true;
    }
    let guarded = s.args.iter().any(|a| a == "camcraft-app" || a == "camcraft-kernel");
    match target_key(s) {
        Some(k) => ISOLATED.contains(&k.as_str()),
        None => guarded,
    }
}

/// A release plan split for the executor: the builds (in order, each alone, first), the queued steps
/// (shared lanes, in the order they are pulled) and the alone steps (after the queue, one at a time).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Schedule {
    pub builds: Vec<usize>,
    pub queue: Vec<usize>,
    pub alone: Vec<usize>,
}

/// Splits `steps` and orders the queue longest-first by `history` (seconds by step name, from an
/// earlier run). A step with no history counts as the longest known one, so a new suite is never left
/// for the tail; ties keep plan order. With no history at all the queue keeps plan order.
pub fn schedule(steps: &[Step], history: &HashMap<String, u64>) -> Schedule {
    let mut out = Schedule::default();
    for (i, s) in steps.iter().enumerate() {
        if is_build(s) {
            out.builds.push(i);
        } else if alone(s) {
            out.alone.push(i);
        } else {
            out.queue.push(i);
        }
    }
    let top = out.queue.iter().filter_map(|&i| history.get(&steps[i].name)).copied().max().unwrap_or(0);
    let cost = |i: usize| history.get(&steps[i].name).copied().unwrap_or(top);
    out.queue.sort_by_key(|&i| std::cmp::Reverse(cost(i))); // stable: ties keep plan order
    out
}

/// The timings file a release run writes into its log dir: one `seconds<TAB>step name` line per step.
pub const TIMINGS: &str = "timings.tsv";

/// The step seconds of the newest `gate-logs/release-*/timings.tsv` under `root` (the stamp sorts by
/// time), with the file it came from; empty when no release run has recorded one. Unreadable lines are
/// skipped: history only orders the queue, it never selects or drops a step.
pub fn history(root: &Path) -> (HashMap<String, u64>, Option<PathBuf>) {
    let Ok(rd) = std::fs::read_dir(root.join("gate-logs")) else { return (HashMap::new(), None) };
    let mut dirs: Vec<PathBuf> = rd
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.file_name().is_some_and(|n| n.to_string_lossy().starts_with("release-")))
        .filter(|p| p.join(TIMINGS).is_file())
        .collect();
    dirs.sort();
    let Some(dir) = dirs.pop() else { return (HashMap::new(), None) };
    let file = dir.join(TIMINGS);
    let text = std::fs::read_to_string(&file).unwrap_or_default();
    (parse_timings(&text), Some(file))
}

pub fn parse_timings(text: &str) -> HashMap<String, u64> {
    text.lines()
        .filter_map(|l| {
            let (secs, name) = l.split_once('\t')?;
            Some((name.to_string(), secs.trim().parse().ok()?))
        })
        .collect()
}

pub fn format_timings<'a>(rows: impl IntoIterator<Item = (&'a str, u64)>) -> String {
    rows.into_iter().map(|(name, secs)| format!("{secs}\t{name}\n")).collect()
}
