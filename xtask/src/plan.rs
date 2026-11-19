//! The gate's fixed lists and the batch / release plans (AGENTS.md "Three gate tiers").
//! Provenance: our own build law, ported from scripts/gate.ps1 (#748); drives only the public cargo / libtest CLI.

use std::cmp::Ordering;
use std::path::Path;

use crate::scan;

/// Debug test binaries over ~40 s: skipped by batch, run by release (and by quick when the diff maps to
/// them). Format `crate-dir:binary`. Measured 2026-11-07 (debug, -j 6, 6 test threads).
pub const SLOW_BINARIES: &[&str] = &[
    "motion:finish3d",          // ~50 s
    "motion:pivot5d",           // ~46 s
    "motion:flow",              // ~42 s
    "app:center_cut_multiaxis", // ~40 s
];
/// Crate dirs in dependency order. kernel builds a C chain (truck dev-dep) and its debug suites take
/// about 3 min: batch runs them since #1702, so the post-merge gate finds a kernel failure before a cut.
pub const BATCH_CRATES: &[&str] = &["geometry", "kernel", "cl", "tools", "post", "sim", "motion", "app"];
pub const ALL_CRATES: &[&str] = &[
    "geometry", "kernel", "cl", "tools", "post", "sim", "motion", "app", "ui",
];
/// A touch to a shared crate (or the workspace manifest) makes quick a batch.
pub const SHARED: &[&str] = &["geometry", "cl"];
pub const CLIPPY: &[&str] = &["geometry", "cl", "tools", "post", "sim", "motion", "app", "ui"];
pub const J: &str = "6";
pub const T: &str = "--test-threads=6";

/// One cargo command of a plan. `in_ui` runs it with CWD `crates/ui` (the snapshot goldens resolve
/// from there); every other step runs at the workspace root.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Step {
    pub name: String,
    pub args: Vec<String>,
    pub in_ui: bool,
    /// A known-fail check (`known_fail`): the step is ok only when libtest reports this test as FAILED by
    /// its pinned assert.
    pub expect_fail: Option<crate::known_fail::KnownFail>,
}

impl Step {
    pub fn new(name: impl Into<String>, args: &[&str], dir: &str) -> Step {
        Step {
            name: name.into(),
            args: args.iter().map(|a| (*a).to_string()).collect(),
            in_ui: dir == "ui",
            expect_fail: None,
        }
    }
    pub fn with(mut self, more: &[String]) -> Step {
        self.args.extend(more.iter().cloned());
        self
    }
}

/// A plan: the lines it prints first, then its steps.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Plan {
    pub notes: Vec<String>,
    pub steps: Vec<Step>,
}

pub fn pkg(dir: &str) -> String {
    format!("camcraft-{dir}")
}

pub fn is_slow(dir: &str, bin: &str) -> bool {
    let key = format!("{dir}:{bin}");
    SLOW_BINARIES.contains(&key.as_str())
}

/// PowerShell's `Sort-Object` order for the ASCII names the gate sorts (crate dirs, suite names, fn
/// names): case-insensitive, punctuation before digits before letters. Keeps step order identical to
/// what gate.ps1 printed.
pub fn ps_cmp(a: &str, b: &str) -> Ordering {
    fn key(c: char) -> (u8, char) {
        let class = if c.is_alphabetic() {
            2
        } else if c.is_numeric() {
            1
        } else {
            0
        };
        (class, c.to_ascii_lowercase())
    }
    a.chars().map(key).cmp(b.chars().map(key)).then_with(|| a.cmp(b))
}

pub fn ps_sorted<I: IntoIterator<Item = String>>(items: I) -> Vec<String> {
    let mut v: Vec<String> = items.into_iter().collect();
    v.sort_by(|a, b| ps_cmp(a, b));
    v.dedup();
    v
}

fn strs(v: &[String]) -> Vec<&str> {
    v.iter().map(String::as_str).collect()
}

/// One test target of a crate: the step label and its cargo selector.
fn suite_targets(root: &Path, d: &str, skip_slow: bool) -> Vec<(String, Vec<String>)> {
    let mut t: Vec<(String, Vec<String>)> = vec![("lib".into(), vec!["--lib".into()])];
    if !skip_slow {
        t.push(("bins".into(), vec!["--bins".into()]));
    }
    for b in scan::binaries(root, d) {
        if !(skip_slow && is_slow(d, &b)) {
            t.push((b.clone(), vec!["--test".into(), b]));
        }
    }
    if !skip_slow {
        t.push(("doc".into(), vec!["--doc".into()]));
    }
    t
}

/// The debug suites of `dirs` as one step for each test target (#1941), as the heavy lanes do (#1714):
/// `--lib`, with `skip_slow` false also `--bins`, every integration binary by name and `--doc`. With
/// `skip_slow` a binary of `SLOW_BINARIES` gets no step (batch; the release tier runs it). Together the
/// steps of a crate run exactly the targets of `cargo test -p <crate>` (batch: minus bins, doc and the
/// slow binaries, as before). A red step names one target; `--only "app suites: <binary>"` runs it alone.
pub fn run_suites(root: &Path, dirs: &[&str], skip_slow: bool) -> Vec<Step> {
    let mut out = Vec::new();
    for &d in dirs {
        let p = pkg(d);
        for (label, sel) in suite_targets(root, d, skip_slow) {
            let mut args = vec!["test", "-j", J, "-p", &p, "--no-fail-fast"];
            args.extend(strs(&sel));
            args.extend(["--", T]);
            out.push(Step::new(format!("{d} suites: {label}"), &args, d));
        }
    }
    out
}

/// The name of the one build step of batch and release; the executor stops when it fails.
pub const DEBUG_BUILD: &str = "debug build (--no-run)";

/// One debug `--no-run` build step for `dirs` (`-p` each), run before their per-target steps (#1941).
pub fn debug_build(dirs: &[&str]) -> Step {
    let mut args: Vec<String> = vec!["test".into(), "-j".into(), J.into()];
    for d in dirs {
        args.push("-p".into());
        args.push(pkg(d));
    }
    args.push("--no-run".into());
    Step::new(DEBUG_BUILD, &[], "").with(&args)
}

pub fn clippy() -> Step {
    let mut args: Vec<String> = vec!["clippy".into(), "-j".into(), J.into()];
    for d in CLIPPY {
        args.push("-p".into());
        args.push(pkg(d));
    }
    for a in ["--all-targets", "--", "-D", "warnings"] {
        args.push(a.into());
    }
    Step::new("clippy -D warnings", &[], "").with(&args)
}

pub fn batch(root: &Path) -> Plan {
    let mut plan = Plan::default();
    plan.notes.push(format!(
        "batch: debug suites (kernel included) minus SLOW_BINARIES ({}), ui, clippy",
        SLOW_BINARIES.join(", ")
    ));
    plan.notes.push("one build (--no-run), then one step for each test target (#1941)".into());
    let built: Vec<&str> = BATCH_CRATES.iter().copied().chain(["ui"]).collect();
    plan.steps.push(debug_build(&built));
    plan.steps.extend(run_suites(root, BATCH_CRATES, true));
    plan.steps.extend(run_suites(root, &["ui"], false));
    plan.steps.push(clippy());
    plan
}

/// Crate dirs whose `#[ignore]`d tests run in release (the heavy lanes).
pub const HEAVY_CRATES: &[&str] = &["kernel", "motion", "app"];

/// True when the integration binary `b` can hold an `#[ignore]`d test: its source text names `ignore`, or it
/// pulls in other files (`mod x;`, `include!`, `#[path]`) the entry file does not show. A binary with no
/// ignored test has nothing for a heavy step to run, so it gets none (526 steps otherwise, one cargo
/// start each); the test `heavy_steps_skip_only_binaries_without_ignored_tests` pins that no binary is
/// skipped by mistake. Unreadable source counts as heavy: the gate never drops a binary by accident.
fn may_run_ignored(root: &Path, d: &str, b: &str) -> bool {
    let entry = crate::suites::entry(root, d, b);
    let Ok(text) = std::fs::read_to_string(&entry) else { return true };
    if text.contains("ignore") || text.contains("include!") || text.contains("#[path") {
        return true;
    }
    text.lines().any(|l| {
        let t = l.trim_start();
        let t = t.strip_prefix("pub ").unwrap_or(t);
        t.starts_with("mod ") && t.trim_end().ends_with(';')
    })
}

/// The heavy lanes as one step for each test target (#1714): one release build of the heavy crates
/// (`--no-run`), then for each crate its `--lib`, `--bins`, every integration binary by name and its
/// `--doc` tests, each running only its `--ignored` tests. Together the targets are exactly what
/// `cargo test --release -p <crate> -- --ignored` runs. Kernel joins the heavy coverage in #1783;
/// a red step names one binary, and `--only "app heavy: <binary>"` re-runs it alone.
pub fn heavy_steps(root: &Path) -> Vec<Step> {
    let entries = crate::known_fail::entries(root);
    let mut out = Vec::new();
    let mut build: Vec<String> = vec!["test".into(), "--release".into(), "-j".into(), J.into()];
    for d in HEAVY_CRATES {
        build.push("-p".into());
        build.push(pkg(d));
    }
    build.push("--no-run".into());
    out.push(Step::new("heavy build (release --no-run)", &[], "").with(&build));
    for &d in HEAVY_CRATES {
        let p = pkg(d);
        let mut targets: Vec<(String, Vec<String>)> = vec![
            ("lib".into(), vec!["--lib".into()]),
            ("bins".into(), vec!["--bins".into()]),
        ];
        for b in scan::binaries(root, d) {
            if may_run_ignored(root, d, &b) {
                targets.push((b.clone(), vec!["--test".into(), b]));
            }
        }
        targets.push(("doc".into(), vec!["--doc".into()]));
        for (label, sel) in targets {
            let mut args = vec!["test", "--release", "-j", J, "-p", &p, "--no-fail-fast"];
            args.extend(strs(&sel));
            args.extend(["--", T]);
            let skip = crate::known_fail::skip_args(&entries, d, &label);
            args.extend(strs(&skip));
            args.push("--ignored");
            out.push(Step::new(format!("{d} heavy: {label}"), &args, ""));
            // Each skipped test runs alone and must fail: a pass means the skip is stale (#1950 decision).
            for k in crate::known_fail::of(&entries, d, &label) {
                let mut args = vec!["test", "--release", "-j", J, "-p", &p, "--no-fail-fast"];
                args.extend(strs(&sel));
                args.extend(["--", T, "--exact", &k.test, "--ignored"]);
                let mut step = Step::new(crate::known_fail::step_name(k), &args, "");
                step.expect_fail = Some(k.clone());
                out.push(step);
            }
        }
    }
    out
}

pub fn release(root: &Path) -> Plan {
    release_with(root, true)
}

/// The release plan; `snapshots: false` (`gate release --no-snapshots`, #1413) leaves out the two ui
/// pixel-snapshot lanes, whose goldens are made on Windows: the cut workflow's Linux runner runs every
/// other lane, and the snapshot lanes stay a local step at the cut.
pub fn release_with(root: &Path, snapshots: bool) -> Plan {
    let mut plan = Plan::default();
    plan.notes.push(
        "release: prebuild both profiles, then heavy targets before debug suites; each phase runs a longest-first shared queue, then ui, clippy and lanes::ISOLATED targets alone"
            .into(),
    );
    if !snapshots {
        plan.notes.push(
            "--no-snapshots: the ui pixel-snapshot lanes are skipped (Windows-made goldens); run them locally at the cut".into(),
        );
    }
    plan.notes.extend(crate::known_fail::entries(root).iter().map(crate::known_fail::note));
    let non_ui: Vec<&str> = ALL_CRATES.iter().copied().filter(|d| *d != "ui").collect();
    let mut build = vec!["test", "-j", J];
    let packages: Vec<String> = ALL_CRATES.iter().map(|d| pkg(d)).collect();
    for p in &packages {
        build.extend(["-p", p]);
    }
    build.push("--no-run");
    plan.steps.push(Step::new(DEBUG_BUILD, &build, ""));
    let mut heavy = heavy_steps(root).into_iter();
    if let Some(build) = heavy.next() {
        plan.steps.push(build);
    }
    plan.steps.extend(heavy);
    plan.steps.extend(run_suites(root, &non_ui, false));
    plan.steps.extend(run_suites(root, &["ui"], false));
    plan.steps.push(clippy());
    if !snapshots {
        return plan;
    }
    // The ui pixel snapshot goldens (#255): #[ignore]d out of quick/batch, run here only, debug like the
    // goldens were made, CWD crates/ui so they resolve tests/snapshots/. Regenerated at the cut with
    // UPDATE_SNAPSHOTS=1.
    let p = pkg("ui");
    plan.steps.push(Step::new(
        "ui snapshot lane (debug --ignored, cwd crates/ui)",
        &["test", "-j", J, "-p", &p, "--lib", "--no-fail-fast", "--", T, "--ignored", "snapshot"],
        "ui",
    ));
    // The docs-picture goldens (#525, #854) live in the gpu_render integration binary, which `--lib`
    // above does not reach: run its `--ignored snapshot` test(s) here too.
    plan.steps.push(Step::new(
        "ui gpu_render snapshot lane (debug --ignored)",
        &["test", "-j", J, "-p", &p, "--test", "gpu_render", "--no-fail-fast", "--", T, "--ignored", "snapshot"],
        "ui",
    ));
    plan
}
