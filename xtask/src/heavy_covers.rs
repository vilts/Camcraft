//! `gate heavy --covers <PR>:<sha>` (#3460): one heavy run on the trial tree of a merge wave, and the
//! record of the PR heads that it covers (the `covers:` lines, the tree hash, `covers.txt` in the logs).
//! Provenance: the maintainer's decision on #3460 (AGENTS.md "Heavy lanes at review"); plain git plumbing
//! (`rev-parse --verify`, `merge-base --is-ancestor`, `HEAD^{tree}`, `status --porcelain`).

use std::path::Path;
use std::process::Command;

use crate::exec::Run;

/// One `--covers` value: a PR number and the head sha that the trial tree must contain.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cover {
    pub pr: u32,
    pub sha: String,
}

/// Parses `<PR>:<sha>` (a `#` before the number is permitted). The sha is 7 to 40 hex digits.
pub fn parse(v: &str) -> Result<Cover, String> {
    let bad = |why: &str| format!("--covers '{v}': {why} (the form is --covers <PR>:<head sha>)");
    let (pr, sha) = v.split_once(':').ok_or_else(|| bad("no ':' between the PR and the sha"))?;
    let pr: u32 = pr
        .trim_start_matches('#')
        .parse()
        .ok()
        .filter(|n| *n > 0)
        .ok_or_else(|| bad("the PR is not a positive number"))?;
    if !(7..=40).contains(&sha.len()) || !sha.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(bad("the sha is not 7 to 40 hex digits"));
    }
    Ok(Cover { pr, sha: sha.to_ascii_lowercase() })
}

/// `git -C root <args>`: the exit code and the trimmed stdout, whatever the code.
fn git_status(root: &Path, args: &[&str]) -> Result<(i32, String), String> {
    let out = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(args)
        .output()
        .map_err(|e| format!("git {}: {e}", args.join(" ")))?;
    Ok((out.status.code().unwrap_or(-1), String::from_utf8_lossy(&out.stdout).trim().to_string()))
}

/// The trial tree that a heavy run covers: each named head (full sha), the hash of `HEAD^{tree}`, and
/// the `--base` ref with the merge base of `HEAD` and that ref (the main that the tree was built on).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Trial {
    pub covers: Vec<Cover>,
    pub tree: String,
    pub base: String,
    pub base_sha: String,
}

/// Checks the trial tree before the run. Refuses a sha that is not a commit, a head that is not an
/// ancestor of `HEAD` (the tree lacks that PR's final head, so the run cannot cover it), and a working
/// tree with changes (then `HEAD^{tree}` is not the tree that the run tests).
pub fn check(root: &Path, covers: &[Cover], base: &str) -> Result<Trial, String> {
    let mut full = Vec::new();
    for c in covers {
        let (code, sha) = git_status(root, &["rev-parse", "--verify", "--quiet", &format!("{}^{{commit}}", c.sha)])?;
        if code != 0 || sha.is_empty() {
            return Err(format!("--covers #{} {}: no such commit in this repository (fetch the PR head first)", c.pr, c.sha));
        }
        let (code, _) = git_status(root, &["merge-base", "--is-ancestor", &sha, "HEAD"])?;
        if code != 0 {
            return Err(format!(
                "--covers #{} {sha}: the head is not an ancestor of HEAD - this trial tree does not contain the final head of #{}, so the run cannot cover it",
                c.pr, c.pr
            ));
        }
        full.push(Cover { pr: c.pr, sha });
    }
    let (code, dirty) = git_status(root, &["status", "--porcelain", "--untracked-files=normal"])?;
    if code != 0 {
        return Err("git status failed: cannot read the trial tree".into());
    }
    if !dirty.is_empty() {
        return Err("--covers needs a clean trial tree: commit or remove the changes, so that HEAD^{tree} is the tree that the run tests".into());
    }
    let (code, tree) = git_status(root, &["rev-parse", "HEAD^{tree}"])?;
    if code != 0 || tree.is_empty() {
        return Err("git rev-parse HEAD^{tree} failed: no tree hash for the record".into());
    }
    let (code, base_sha) = git_status(root, &["merge-base", base, "HEAD"])?;
    if code != 0 || base_sha.is_empty() {
        return Err(format!("no merge base of HEAD with '{base}': the trial tree must be built on {base}"));
    }
    Ok(Trial { covers: full, tree, base: base.to_string(), base_sha })
}

/// The header lines: one `covers: #<PR> <sha>` for each head, `tree: <hash>`, then `base: <ref> <sha>`.
pub fn header(t: &Trial) -> Vec<String> {
    let mut l: Vec<String> = t.covers.iter().map(|c| format!("covers: #{} {}", c.pr, c.sha)).collect();
    l.push(format!("tree: {}", t.tree));
    l.push(format!("base: {} {}", t.base, t.base_sha));
    l
}

/// The line for a plan with no step: the record says so, never `PASS` with 0 steps.
pub const NO_LANE: &str = "the wave reaches no heavy lane";

/// The verdict of a covered run. Not run is never a pass: a planned step that did not run (a stopped
/// run, or a build failure that skipped the rest) gives `INCOMPLETE`.
pub fn verdict(planned: usize, ran: usize, failed: usize, void: bool) -> &'static str {
    if void {
        "VOID"
    } else if planned == 0 {
        "NO LANE"
    } else if failed > 0 {
        "FAIL"
    } else if ran < planned {
        "INCOMPLETE"
    } else {
        "PASS"
    }
}

/// One step of a covered run: name, ok, seconds, note.
pub type StepResult = (String, bool, u64, String);

/// The text of `covers.txt`: the header, one line for each step (`lanes:`), the counts and the verdict.
pub fn record(t: &Trial, planned: usize, steps: &[StepResult], wall_min: f64, void: bool) -> String {
    let mut s = header(t).join("\n");
    s.push_str("\nlanes:\n");
    if planned == 0 {
        s.push_str(&format!("  {NO_LANE}\n"));
    }
    for (name, ok, secs, note) in steps {
        let note = if note.is_empty() { String::new() } else { format!("  {note}") };
        s.push_str(&format!("  {:<4} {secs:>5}s  {name}{note}\n", if *ok { "ok" } else { "FAIL" }));
    }
    let bad = steps.iter().filter(|s| !s.1).count();
    let ok = steps.len() - bad;
    s.push_str(&format!("counts: {planned} planned, {} ran, {ok} ok, {bad} failed\n", steps.len()));
    s.push_str(&format!("wall: {wall_min:.1} min\n"));
    s.push_str(&format!("verdict: {}\n", verdict(planned, steps.len(), bad, void)));
    s
}

/// The lines printed under the summary: the header, the counts and the verdict. A real run also writes
/// `covers.txt` into the run's log directory; a dry run writes nothing and says that nothing ran.
pub fn write(run: &Run, t: &Trial, planned: usize) -> Result<Vec<String>, String> {
    let mut lines = header(t);
    if run.dry {
        lines.push(format!("planned: {planned} step(s), dry run: nothing ran"));
        if planned == 0 {
            lines.push(NO_LANE.to_string());
        }
        return Ok(lines);
    }
    let steps: Vec<StepResult> = run.done.iter().map(|d| (d.name.clone(), d.ok, d.secs, d.note.clone())).collect();
    let text = record(t, planned, &steps, run.clock.elapsed().as_secs_f64() / 60.0, run.void.is_some());
    let path = run.log_dir.join("covers.txt");
    std::fs::write(&path, &text).map_err(|e| format!("write {}: {e}", path.display()))?;
    lines.extend(text.lines().filter(|l| l.starts_with("counts:") || l.starts_with("verdict:")).map(String::from));
    if planned == 0 {
        lines.push(NO_LANE.to_string());
    }
    lines.push(format!("record: {}", path.display()));
    Ok(lines)
}
