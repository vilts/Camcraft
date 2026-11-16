//! Running a gate plan: one cargo command per step, its output to its own log under
//! gate-logs/<tier>-<timestamp>/, pass/fail and failing suites read back from the log, then the summary.
//! Provenance: our own build law (AGENTS.md "Three gate tiers"), ported from scripts/gate.ps1 (#748).

use std::fs::File;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use crate::checkout::{self, Reading};
use crate::plan::{self, Plan, Step};
use crate::scan::re;

pub struct Run {
    pub root: PathBuf,
    pub label: String,
    pub dry: bool,
    /// The job budget of the whole run (#2057): a wave's lanes share it, a step alone gets all of it.
    pub budget: usize,
    budget_note: Option<String>,
    log_dir: PathBuf,
    clock: Instant,
    done: Vec<Done>,
    /// HEAD and the working tree when the gate started (#1201); every later reading is compared with it.
    guard: Reading,
    /// The VOID line, once the checkout was seen to move; a void run has no verdict.
    void: Option<String>,
}

struct Done {
    name: String,
    ok: bool,
    secs: u64,
    note: String,
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

fn gate_cache_env() -> Vec<(&'static str, String)> {
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
            dry,
            log_dir,
            clock: Instant::now(),
            done: Vec::new(),
            guard,
            void: None,
        })
    }

    pub fn plan(&mut self, plan: &Plan) -> Result<(), String> {
        for n in &plan.notes {
            println!("{n}");
        }
        let lanes = self.label == "release";
        if let Some(n) = &self.budget_note {
            println!("{n}");
        }
        println!("{}", crate::budget::header(self.budget, lanes));
        if lanes {
            let waves = crate::lanes::waves(&plan.steps, crate::budget::lane_limit(self.budget));
            for (k, wave) in waves.iter().enumerate() {
                if self.check("before release wave") {
                    break;
                }
                let jobs = crate::budget::per_lane(self.budget, wave.len());
                println!("-- wave {}: {} lane(s) x -j {jobs}", k + 1, wave.len());
                let results = std::thread::scope(|scope| {
                    let handles: Vec<_> = wave.iter().map(|&i| {
                        let (root, logs, dry) = (&self.root, &self.log_dir, self.dry);
                        let step = &plan.steps[i];
                        scope.spawn(move || Self::execute(root, logs, dry, i + 1, step, jobs))
                    }).collect();
                    // Join every lane even when one returns an error or panics.
                    let joined: Vec<_> = handles.into_iter().map(|h| {
                        h.join().map_err(|_| "gate lane thread failed".to_string()).and_then(|r| r)
                    }).collect();
                    joined.into_iter().collect::<Result<Vec<_>, String>>()
                })?;
                let build_failed = wave.iter().any(|&i| plan.steps[i].args.iter().any(|a| a == "--no-run"))
                    && results.iter().any(|d| !d.ok);
                self.done.extend(results);
                if build_failed {
                    break;
                }
            }
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
    fn check(&mut self, at: &str) -> bool {
        if self.void.is_some() {
            return true;
        }
        if self.dry {
            return false; // a dry run runs nothing, so the checkout cannot void it (one git call per step)
        }
        let msg = match checkout::read(&self.root) {
            Ok(now) => checkout::void_message(&self.guard, &now, at),
            Err(e) => Some(format!(
                "VOID: could not read the checkout ({e}) during the gate ({at}) - this run is void, not a verdict on any commit"
            )),
        };
        match msg {
            Some(m) => {
                println!("{m}");
                self.void = Some(m);
                true
            }
            None => false,
        }
    }

    fn step(&mut self, s: &Step) -> Result<(), String> {
        let done = Self::execute(&self.root, &self.log_dir, self.dry, self.done.len() + 1, s, self.budget)?;
        self.done.push(done);
        Ok(())
    }

    /// Runs one step with its `-j` and `--test-threads` set to `jobs` (`budget::scale`).
    fn execute(root: &Path, log_dir: &Path, dry: bool, n: usize, s: &Step, jobs: usize) -> Result<Done, String> {
        let args = crate::budget::scale(&s.args, jobs);
        let log = log_dir.join(log_name(n, &s.name));
        let dir = if s.in_ui { root.join("crates").join("ui") } else { root.to_path_buf() };
        let leaf = dir.file_name().map(|x| x.to_string_lossy().into_owned()).unwrap_or_default();
        println!("[{n:02}] {}", s.name);
        println!("     (cwd {leaf}) cargo {}", args.join(" "));
        if dry {
            return Ok(Done { name: s.name.clone(), ok: true, secs: 0, note: "dry run".into() });
        }
        let cache = gate_cache_env();
        if !cache.is_empty() {
            println!("     (compile cache: sccache on)");
            if std::env::var_os("CARGO_TARGET_DIR").is_some() {
                println!("     note: CARGO_TARGET_DIR is set; sccache hashes it, so hits stop across worktrees - use the worktree's own target/ instead");
            }
        }
        let t0 = Instant::now();
        let file = File::create(&log).map_err(|e| format!("create {}: {e}", log.display()))?;
        let err = file.try_clone().map_err(|e| format!("log {}: {e}", log.display()))?;
        let code = Command::new(cargo())
            .envs(cache)
            .args(&args)
            .current_dir(&dir)
            .stdin(Stdio::null())
            .stdout(file)
            .stderr(err)
            .status()
            .map(|st| st.code().unwrap_or(-1))
            .map_err(|e| format!("cargo {}: {e}", args.join(" ")))?;
        let secs = t0.elapsed().as_secs();
        let note = read_log(&std::fs::read_to_string(&log).unwrap_or_default(), code)?;
        let ok = code == 0;
        println!("     {} in {secs}s {note}", if ok { "ok" } else { "FAIL" });
        Ok(Done { name: s.name.clone(), ok, secs, note })
    }

    /// Prints the summary table; the process exit code (0 all passed, 1 any failed).
    pub fn summary(self) -> i32 {
        println!("\n== gate summary ({}) - logs in {}", self.label, self.log_dir.display());
        for d in &self.done {
            println!("  {:<4} {:>5}s  {}  {}", if d.ok { "ok" } else { "FAIL" }, d.secs, d.name, d.note);
        }
        let bad = self.done.iter().filter(|d| !d.ok).count();
        let mins = self.clock.elapsed().as_secs_f64() / 60.0;
        if self.label == "release" {
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
