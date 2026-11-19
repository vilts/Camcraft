//! Source-marked known-fail heavy tests pinned to an open issue: the release heavy lanes skip them by name, and one
//! extra step for each runs it alone and must see it fail, so a skip that starts to pass fails the gate.
//! Provenance: our own build law (AGENTS.md "Three gate tiers"; the #1950 known-fail decision, RECOMMENDED.md);
//! libtest's public `--exact` / `--skip` / `--ignored` flags and the `gh issue view` CLI.

use std::io::Read;
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

/// One `#[ignore]`d test that is red on purpose: it pins the target of an open issue.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KnownFail {
    /// Crate dir (`motion`, `app`).
    pub dir: String,
    /// Heavy target label: `lib` or the integration binary's name.
    pub target: String,
    /// The libtest path of the test inside that target.
    pub test: String,
    /// The open issue the test pins; the gate refuses the skip once the issue reads closed.
    pub issue: u32,
    /// The test's source file (from the workspace root, `/`): its `panicked at` line must name it.
    pub file: String,
    /// Source line of the marked assertion, to reject another assertion in the same test.
    pub line: usize,
    /// A stable piece of that assert's panic message: the step is ok only when the panic block holds it.
    pub reason: String,
}

/// Read source markers. Invalid markers produce no skips; `check_issues` refuses the release.
pub fn entries(root: &Path) -> Vec<KnownFail> {
    crate::known_fail_scan::discover(root).unwrap_or_default()
}

/// The known-fail tests of one heavy target.
pub fn of<'a>(entries: &'a [KnownFail], dir: &str, target: &str) -> Vec<&'a KnownFail> {
    entries.iter().filter(|k| k.dir == dir && k.target == target).collect()
}

pub fn skip_args(entries: &[KnownFail], dir: &str, target: &str) -> Vec<String> {
    let mut out = Vec::new();
    for k in of(entries, dir, target) {
        if out.is_empty() {
            out.push("--exact".to_string());
        }
        out.push("--skip".to_string());
        out.push(k.test.clone());
    }
    out
}

/// The line the release plan prints for each skipped test.
pub fn note(k: &KnownFail) -> String {
    format!(
        "SKIPPED (known fail, #{}): {} heavy: {} {} - its own step must still fail",
        k.issue, k.dir, k.target, k.test
    )
}

/// The short step name of a known-fail check.
pub fn step_name(k: &KnownFail) -> String {
    let short = k.test.rsplit("::").next().unwrap_or(&k.test);
    format!("{} heavy: known fail #{} {}", k.dir, k.issue, short)
}

/// The repository whose issues pin the entries (a fork reads upstream's issue, not its own number).
pub const REPO: &str = "NeoTech/Camcraft";
/// The longest wait for one `gh issue view`.
const GH_TIMEOUT: Duration = Duration::from_secs(30);

/// Reads each pinned issue's state with `gh`. A closed issue is an error (the skip is stale); an issue that
/// `gh` cannot read (no token, as in the cut job, or no answer in 30 s) gives a note and no verdict, so the
/// known-fail steps stay the guard.
pub fn check_issues(root: &Path, dry: bool) -> Result<Vec<String>, String> {
    let entries = crate::known_fail_scan::discover(root)?;
    check_states(&entries, dry, |n| gh_state(root, n))
}

pub fn check_states(entries: &[KnownFail], dry: bool, state: impl Fn(u32) -> Option<String>) -> Result<Vec<String>, String> {
    let mut issues: Vec<u32> = entries.iter().map(|k| k.issue).collect();
    issues.sort_unstable();
    issues.dedup();
    let mut notes = Vec::new();
    for n in issues {
        if dry {
            notes.push(format!("known fail: #{n} state not read (dry run)"));
            continue;
        }
        match state(n) {
            Some(state) => {
                if state.eq_ignore_ascii_case("closed") {
                    return Err(format!(
                        "known fail: #{n} is closed, but a known-fail marker still skips its test - remove the marker"
                    ));
                }
                notes.push(format!("known fail: #{n} is {state}"));
            }
            None => notes.push(format!(
                "known fail: could not read #{n} with gh - issue state not checked; the known-fail steps still must fail"
            )),
        }
    }
    Ok(notes)
}

/// `gh issue view <n> -R REPO` with a timeout; `None` when it fails, times out or cannot start.
fn gh_state(root: &Path, n: u32) -> Option<String> {
    let mut child = Command::new("gh")
        .args(["issue", "view", &n.to_string(), "-R", REPO, "--json", "state", "-q", ".state"])
        .current_dir(root)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    let t0 = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(st)) if st.success() => break,
            Ok(Some(_)) | Err(_) => return None,
            Ok(None) if t0.elapsed() > GH_TIMEOUT => {
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(100)),
        }
    }
    let mut out = String::new();
    child.stdout.take()?.read_to_string(&mut out).ok()?;
    Some(out.trim().to_string())
}
