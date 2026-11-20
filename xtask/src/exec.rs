//! Running a gate plan: one cargo command per step, its output to its own log under
//! gate-logs/<tier>-<timestamp>/, pass/fail and failing suites read back from the log, then the summary.
//! Provenance: our own build law (AGENTS.md "Three gate tiers"), ported from scripts/gate.ps1 (#748).

use std::fs::File;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use crate::artifacts::{Built, Package};
use crate::checkout::{self, Reading};
use crate::plan::{self, Plan, Step};
use crate::scan::re;

pub struct Run {
    pub root: PathBuf,
    pub label: String,
    /// Batch and release run their steps in the parallel lanes of `queue` (#2727); a quick gate that falls
    /// back to the batch plan sets it too. Else the steps run one at a time.
    pub lanes: bool,
    pub dry: bool,
    /// The job budget of the whole run (#2057): the release queue's lanes share it, a step alone gets all of it.
    pub budget: usize,
    budget_note: Option<String>,
    pub(crate) log_dir: PathBuf,
    clock: Instant,
    pub(crate) done: Vec<Done>,
    /// HEAD and the working tree when the gate started (#1201); every later reading is compared with it.
    pub(crate) guard: Reading,
    /// The VOID line, once the checkout was seen to move; a void run has no verdict.
    pub(crate) void: Option<String>,
    /// The test executables of the crates that this run built; their suite steps start no cargo (#1951).
    pub(crate) built: Built,
}

pub(crate) struct Done {
    pub(crate) name: String,
    pub(crate) ok: bool,
    pub(crate) secs: u64,
    pub(crate) note: String,
    /// What a green recording build step read from cargo's JSON messages (`direct::records`, #1951).
    pub(crate) built: Option<(String, Package)>,
}

/// The cargo that launched us (`cargo xtask` sets CARGO), else the one on PATH.
pub fn cargo() -> String {
    std::env::var("CARGO").unwrap_or_else(|_| "cargo".into())
}

/// The compile-cache environment for a gate's cargo calls (#900): `RUSTC_WRAPPER=sccache` and a cache size
/// cap, but only when `sccache` is on PATH and the caller has not chosen a wrapper (an existing
/// `RUSTC_WRAPPER`, even an empty one, wins; `CAMCRAFT_NO_SCCACHE=1` opts out). A machine without sccache
/// builds exactly as before. Release packaging never goes through this (it builds the shipped binary).
pub fn cache_env(sccache: Option<&str>, wrapper_set: bool, opted_out: bool, size_set: bool) -> Vec<(&'static str, String)> {
    let mut env = Vec::new();
    if let Some(path) = sccache
        && !wrapper_set
        && !opted_out
    {
        env.push(("RUSTC_WRAPPER", path.to_string()));
        if !size_set {
            env.push(("SCCACHE_CACHE_SIZE", "20G".to_string()));
        }
    }
    env
}

/// Compiler-wrapper variables the release packaging build must not inherit (#935): the shipped binary never
/// depends on a compile cache, even when the maintainer sets `RUSTC_WRAPPER` machine-wide. Returns the ones
/// present (`is_set` asks the environment); the release GATE tier may still use the cache, only packaging clears it.
pub fn packaging_env_clear(is_set: impl Fn(&str) -> bool) -> Vec<&'static str> {
    ["RUSTC_WRAPPER", "CARGO_BUILD_RUSTC_WRAPPER"].into_iter().filter(|v| is_set(v)).collect()
}

/// The first `sccache` (or `sccache.exe`) file on PATH, as a path string.
pub fn find_sccache() -> Option<String> {
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .flat_map(|d| [d.join("sccache"), d.join("sccache.exe")])
        .find(|p| p.is_file())
        .map(|p| p.to_string_lossy().into_owned())
}

pub(crate) fn gate_cache_env() -> Vec<(&'static str, String)> {
    cache_env(
        find_sccache().as_deref(),
        std::env::var_os("RUSTC_WRAPPER").is_some(),
        std::env::var_os("CAMCRAFT_NO_SCCACHE").is_some_and(|v| v == "1"),
        std::env::var_os("SCCACHE_CACHE_SIZE").is_some(),
    )
}

/// `git -C root <args>`, stdout as text; `Err` when git fails.
pub fn git(root: &Path, args: &[&str]) -> Result<String, String> {
    let out = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(args)
        .output()
        .map_err(|e| format!("git {}: {e}", args.join(" ")))?;
    if !out.status.success() {
        return Err(format!(
            "git {} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

/// `yyyyMMdd-HHmmss` (UTC) for the log dir name.
pub fn stamp(now: SystemTime) -> String {
    let secs = now.duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    let (days, rem) = ((secs / 86_400) as i64, secs % 86_400);
    // Civil date from days since 1970-01-01 (H. Hinnant, "chrono-Compatible Low-Level Date Algorithms").
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!("{y:04}{m:02}{d:02}-{:02}{:02}{:02}", rem / 3600, rem % 3600 / 60, rem % 60)
}

/// A log file name: the step name with each run of non-word characters folded to `_`.
pub fn log_name(n: usize, name: &str) -> String {
    let mut s = String::new();
    let mut gap = false;
    for c in name.chars() {
        if c.is_alphanumeric() || c == '_' || c == '-' {
            s.push(c);
            gap = false;
        } else if !gap {
            s.push('_');
            gap = true;
        }
    }
    format!("{n:02}-{s}.log")
}

impl Run {
    pub fn new(root: &Path, label: &str, dry: bool) -> Result<Run, String> {
        let log_dir = root.join("gate-logs").join(format!("{label}-{}", stamp(SystemTime::now())));
        std::fs::create_dir_all(&log_dir).map_err(|e| format!("create {}: {e}", log_dir.display()))?;
        let guard = checkout::read(root)?;
        let (budget, budget_note) = crate::budget::from_env();
        Ok(Run {
            budget,
            budget_note,
            root: root.to_path_buf(),
            label: label.into(),
            lanes: matches!(label, "batch" | "release"),
            dry,
            log_dir,
            clock: Instant::now(),
            done: Vec::new(),
            guard,
            void: None,
            built: Built::new(),
        })
    }

    pub fn plan(&mut self, plan: &Plan) -> Result<(), String> {
        for n in &plan.notes {
            println!("{n}");
        }
        let lanes = self.lanes;
        if let Some(n) = &self.budget_note {
            println!("{n}");
        }
        println!("{}", crate::budget::header(self.budget, lanes));
        if lanes {
            self.run_lanes(plan)?;
        } else {
            for s in &plan.steps {
                if self.check(&format!("before {}", s.name)) {
                    break;
                }
                self.step(s)?;
                // A failed whole-gate build fails every later step with the same error: stop (as release does).
                let failed = self.done.last().is_some_and(|d| !d.ok);
                if failed && s.name == plan::DEBUG_BUILD {
                    break;
                }
            }
        }
        if self.void.is_none() {
            self.check("at the end");
        }
        Ok(())
    }

    /// Compares the checkout with the start reading. Returns true (and prints VOID) once it has moved.
    pub(crate) fn check(&mut self, at: &str) -> bool {
        if self.void.is_some() {
            return true;
        }
        if self.dry {
            return false; // a dry run runs nothing, so the checkout cannot void it (one git call per step)
        }
        match moved(&self.root, &self.guard, at) {
            Some(m) => {
                println!("{m}");
                self.void = Some(m);
                true
            }
            None => false,
        }
    }

    fn step(&mut self, s: &Step) -> Result<(), String> {
        let done = Self::execute(&self.root, &self.log_dir, self.dry, self.done.len() + 1, s, self.budget, &self.built)?;
        self.keep(done);
        Ok(())
    }

    /// Adds a finished step; the executables that a build step recorded go to `built` for the later steps.
    pub(crate) fn keep(&mut self, mut d: Done) {
        self.built.extend(d.built.take());
        self.done.push(d);
    }

    /// Runs one step with its `-j` and `--test-threads` set to `jobs` (`budget::scale`). A debug suite
    /// step of a crate in `built` starts its test executable; each other step starts cargo (`direct`).
    pub(crate) fn execute(root: &Path, log_dir: &Path, dry: bool, n: usize, s: &Step, jobs: usize, built: &Built) -> Result<Done, String> {
        let args = crate::budget::scale(&s.args, jobs);
        let log = log_dir.join(log_name(n, &s.name));
        let dir = if s.in_ui { root.join("crates").join("ui") } else { root.to_path_buf() };
        let leaf = |d: &Path| d.file_name().map(|x| x.to_string_lossy().into_owned()).unwrap_or_default();
        let direct = crate::direct::chosen(s, &args, built, &cargo());
        println!("[{n:02}] {}", s.name);
        match &direct {
            Some(d) => println!("     (cwd {}) {}", leaf(&d.cwd), crate::direct::describe(d, root)),
            None => println!("     (cwd {}) cargo {}", leaf(&dir), args.join(" ")),
        }
        if dry {
            return Ok(Done { name: s.name.clone(), ok: true, secs: 0, note: "dry run".into(), built: None });
        }
        let cache = if direct.is_some() { Vec::new() } else { gate_cache_env() };
        if !cache.is_empty() {
            println!("     (compile cache: sccache on)");
            if std::env::var_os("CARGO_TARGET_DIR").is_some() {
                println!("     note: CARGO_TARGET_DIR is set; sccache hashes it, so hits stop across worktrees - use the worktree's own target/ instead");
            }
        }
        let t0 = Instant::now();
        let file = File::create(&log).map_err(|e| format!("create {}: {e}", log.display()))?;
        let records = crate::direct::records(s);
        let (code, json) = match &direct {
            Some(d) => (crate::direct::run(d, root, file)?, None),
            None => crate::direct::cargo(&cargo(), cache, &args, &dir, file, records.is_some())?,
        };
        let mut recorded = None;
        if let (Some(name), Some(json), 0) = (records.filter(|_| !crate::direct::opted_out()), json, code) {
            match crate::direct::record(root, &cargo(), &name, &json) {
                Ok(p) => {
                    println!("     {} test executable(s) of {name} recorded: their steps start no cargo", p.count());
                    recorded = Some((name, p));
                }
                Err(e) => println!("     note: no test executable of {name} recorded ({e}); its steps start cargo"),
            }
        }
        let secs = t0.elapsed().as_secs();
        let text = std::fs::read_to_string(&log).unwrap_or_default();
        let mut note = read_log(&text, code)?;
        let ok = match &s.expect_fail {
            Some(k) => expect_fail_note(code, &mut note, &text, k),
            None => code == 0,
        };
        println!("     {} in {secs}s {note}", if ok { "ok" } else { "FAIL" });
        Ok(Done { name: s.name.clone(), ok, secs, note, built: recorded })
    }

    /// Prints the summary table; the process exit code (0 all passed, 1 any failed).
    pub fn summary(self) -> i32 {
        println!("\n== gate summary ({}) - logs in {}", self.label, self.log_dir.display());
        for d in &self.done {
            println!("  {:<4} {:>5}s  {}  {}", if d.ok { "ok" } else { "FAIL" }, d.secs, d.name, d.note);
        }
        let bad = self.done.iter().filter(|d| !d.ok).count();
        let mins = self.clock.elapsed().as_secs_f64() / 60.0;
        if self.lanes {
            let serial: u64 = self.done.iter().map(|d| d.secs).sum();
            println!("  summed lane wall time {serial}s (serial estimate; not a serial benchmark)");
        }
        println!("  wall {mins:.1} min, {} step(s), {bad} failed", self.done.len());
        if self.void.is_some() {
            println!("== gate VOID ({}) - the checkout moved during the run; no verdict, re-run on a checkout nobody pulls into", self.label);
            return 1;
        }
        i32::from(bad > 0)
    }
}

/// A known-fail step (`known_fail`) is ok only when libtest reports `k.test` itself as FAILED (the `read_log`
/// note `failed: ... [tests: ...]` names it exactly) AND the test's panic block in `log` is the pinned
/// assert: its `panicked at` line names `k.file` and `k.line`, and a message line starts with `k.reason`. A pass, a renamed test, a
/// compile error, a crash of the test binary (no `test ... FAILED` line) or a failure for another reason (a
/// planner panic, a gouge warning, another assert) is a FAIL.
pub fn expect_fail_note(code: i32, note: &mut String, log: &str, k: &crate::known_fail::KnownFail) -> bool {
    let test = &k.test;
    if code == 0 {
        *note = "KNOWN FAIL NOW PASSES (or no such test): remove its known-fail marker".into();
        return false;
    }
    let named = note.starts_with("failed:")
        && note
            .split_once("[tests: ")
            .and_then(|(_, rest)| rest.split_once(']'))
            .is_some_and(|(list, _)| list.split(", ").any(|t| t == test));
    if !named {
        *note = format!("known fail did not fail as a test (crash, compile error or other test): {note}");
        return false;
    }
    let pinned = |(at, msg): (String, String)| at.contains(&format!("{}:{}:", k.file, k.line))
        && msg.lines().any(|l| l.starts_with(&k.reason));
    if !panic_block(log, test).is_some_and(pinned) {
        *note = format!("KNOWN FAIL, DIFFERENT REASON (want {:?} at {}): {note}", k.reason, k.file);
        return false;
    }
    *note = format!("known fail, red as expected ({note})");
    true
}

/// The `panicked at` line (slashes made `/`) and the message lines after it of the thread named `test`;
/// the message ends at the next `thread '` line or the `failures:` list.
fn panic_block(log: &str, test: &str) -> Option<(String, String)> {
    let head = format!("thread '{test}'");
    let mut lines = log.lines().skip_while(|l| !(l.starts_with(&head) && l.contains("panicked at ")));
    let at = lines.next()?.replace('\\', "/");
    let msg: Vec<&str> = lines.take_while(|l| !l.starts_with("thread '") && !l.starts_with("failures:")).collect();
    Some((at, msg.join("\n")))
}

/// The VOID line when the checkout under `root` no longer reads as `guard` (or cannot be read); `None` while
/// it is unchanged. Shared by the one-at-a-time loop and the release queue's lanes.
pub(crate) fn moved(root: &Path, guard: &Reading, at: &str) -> Option<String> {
    match checkout::read(root) {
        Ok(now) => checkout::void_message(guard, &now, at),
        Err(e) => Some(format!(
            "VOID: could not read the checkout ({e}) during the gate ({at}) - this run is void, not a verdict on any commit"
        )),
    }
}

/// The step's note from its log: a compile error, the suites that failed (with the failed tests by name,
/// #1714), or a bare exit code. A test that libtest reports as "has been running for over N seconds" is
/// added as `slow: <name>` so a long heavy test shows up by name.
pub fn read_log(text: &str, code: i32) -> Result<String, String> {
    let running = re(r"^\s+Running (\S+)")?;
    let failed_test = re(r"^test (\S+) \.\.\. FAILED")?;
    let slow_test = re(r"^test (\S+) has been running for over (\d+) seconds")?;
    let (mut cur, mut fails, mut compile) = (String::new(), Vec::new(), false);
    let (mut tests, mut slow) = (Vec::new(), Vec::new());
    for l in text.lines() {
        if let Some(c) = running.captures(l) {
            cur = c[1].to_string();
        } else if l.starts_with("test result: FAILED") {
            fails.push(cur.clone());
        } else if l.starts_with("error: could not compile") {
            compile = true;
        } else if let Some(c) = failed_test.captures(l) {
            tests.push(c[1].to_string());
        } else if let Some(c) = slow_test.captures(l) {
            slow.push(format!("{} (>{}s)", &c[1], &c[2]));
        }
    }
    let mut note = if compile {
        "compile error".into()
    } else if !fails.is_empty() {
        let mut n = format!("failed: {}", fails.join(", "));
        if !tests.is_empty() {
            n.push_str(&format!(" [tests: {}]", tests.join(", ")));
        }
        n
    } else if code != 0 {
        format!("exit {code}")
    } else {
        String::new()
    };
    if !slow.is_empty() {
        if !note.is_empty() {
            note.push(' ');
        }
        note.push_str(&format!("slow: {}", slow.join(", ")));
    }
    Ok(note)
}
