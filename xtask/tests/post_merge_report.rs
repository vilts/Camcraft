// Provenance: issue #1611 and the repository's post-merge workflow contract;
// GitHub's public Actions documentation (status check functions in `if`).
use std::{fs, path::Path};

/// The `if:` line of the `report` job.
fn report_if() -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../.github/workflows/post-merge.yml");
    let workflow = fs::read_to_string(path).expect("read the post-merge workflow");
    let report = workflow.split_once("\n  report:\n").expect("the workflow has a report job").1;
    // The job's own lines: indented past the job key, up to the next job.
    report
        .lines()
        .take_while(|line| line.is_empty() || line.starts_with("    "))
        .find_map(|line| line.trim_start().strip_prefix("if: "))
        .expect("the report job has an if")
        .to_owned()
}

#[test]
fn a_failed_gate_still_runs_the_report_job() {
    let cond = report_if();
    // Without a status function GitHub adds an implicit success(): a failed
    // `needs: gate` then skips the job and a red main opens no issue (#1611).
    assert!(cond.starts_with("${{ always() && "), "the report job's if starts with always(): {cond}");
    // always() alone would also report a cancelled or a cut-skipped run.
    assert!(
        cond.contains("(needs.gate.result == 'success' || needs.gate.result == 'failure')"),
        "only a green or a red gate reports: {cond}"
    );
    assert!(cond.contains("needs.gate.outputs.skipped != 'true'"), "a run that yields to a cut reports nothing: {cond}");
}

// The order guard of .github/scripts/post-merge-report.sh (#3184). Each push
// to main has its own run, so an older run can finish after a newer one. The
// script runs here against a real git history and a fake gh that records
// each call. The workflow runs it on Linux only, so the test is unix only.
#[cfg(unix)]
mod order {
    use std::{
        fs,
        os::unix::fs::PermissionsExt,
        path::{Path, PathBuf},
        process::Command,
    };

    const FAKE_GH: &str = r#"#!/usr/bin/env bash
printf '%s\n' "$*" >> "$FAKE_DIR/log"
case "$1 $2" in
  "issue list") case "$*" in *"--state all"*) cat "$FAKE_DIR/latest" ;; *) cat "$FAKE_DIR/open" ;; esac ;;
  "issue view") cat "$FAKE_DIR/body" ;;
  "issue edit") printf '%s' "$*" > "$FAKE_DIR/edit" ;;
esac
exit 0
"#;

    struct Repo {
        dir: PathBuf,
        older: String,
        newer: String,
    }

    fn git(dir: &Path, args: &[&str]) -> String {
        let out = Command::new("git").arg("-C").arg(dir).args(args).output().expect("run git");
        assert!(out.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&out.stderr));
        String::from_utf8_lossy(&out.stdout).trim().to_owned()
    }

    /// A git repository with two commits on one line (older, then newer), a
    /// fake gh, and a rolling issue #7 whose marker holds `marker`.
    fn repo(name: &str, open: bool, marker: impl Fn(&str, &str) -> String) -> Repo {
        let dir = std::env::temp_dir().join(format!("camcraft-post-merge-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("make the temp repo");
        git(&dir, &["init", "-q"]);
        for (i, msg) in ["older", "newer"].iter().enumerate() {
            fs::write(dir.join("f"), i.to_string()).expect("write a file");
            git(&dir, &["add", "f"]);
            git(&dir, &["-c", "user.name=t", "-c", "user.email=t@t", "-c", "commit.gpgsign=false", "commit", "-q", "-m", msg]);
        }
        let newer = git(&dir, &["rev-parse", "HEAD"]);
        let older = git(&dir, &["rev-parse", "HEAD~1"]);
        let gh = dir.join("gh");
        fs::write(&gh, FAKE_GH).expect("write the fake gh");
        fs::set_permissions(&gh, fs::Permissions::from_mode(0o755)).expect("make the fake gh executable");
        fs::write(dir.join("latest"), "7\n").expect("write the latest issue");
        fs::write(dir.join("open"), if open { "7\n" } else { "" }).expect("write the open issue");
        let body = format!("The gate failed.\n\n<!-- post-merge-last-sha: {} -->\n", marker(&older, &newer));
        fs::write(dir.join("body"), body).expect("write the issue body");
        fs::write(dir.join("log"), "").expect("write the log");
        Repo { dir, older, newer }
    }

    /// Runs the report script for `sha` with `status`; returns (stdout, gh log).
    fn report(repo: &Repo, sha: &str, status: &str) -> (String, String) {
        let script = Path::new(env!("CARGO_MANIFEST_DIR")).join("../.github/scripts/post-merge-report.sh");
        let out = Command::new("bash")
            .arg(script)
            .current_dir(&repo.dir)
            .env("GH", repo.dir.join("gh"))
            .env("FAKE_DIR", &repo.dir)
            .env("REPO", "o/r")
            .env("SHA", sha)
            .env("STATUS", status)
            .env("RUN_URL", "https://run")
            .env("OUTPUT", "/dev/null")
            .output()
            .expect("run the report script");
        assert!(out.status.success(), "the script exits 0: {}", String::from_utf8_lossy(&out.stderr));
        let log = fs::read_to_string(repo.dir.join("log")).expect("read the gh log");
        (String::from_utf8_lossy(&out.stdout).into_owned(), log)
    }

    fn writes(log: &str) -> Vec<&str> {
        log.lines()
            .filter(|l| ["issue comment", "issue close", "issue create", "issue edit"].iter().any(|w| l.starts_with(w)))
            .collect()
    }

    #[test]
    fn an_older_green_run_never_closes_a_newer_red_issue() {
        let repo = repo("older-green", true, |_, newer| newer.to_owned());
        let (stdout, log) = report(&repo, &repo.older, "success");
        assert!(writes(&log).is_empty(), "an older commit writes nothing to the issue: {log}");
        assert!(stdout.contains("::notice"), "the skip prints a notice: {stdout}");
        let _ = fs::remove_dir_all(&repo.dir);
    }

    #[test]
    fn an_older_red_run_never_reopens_after_a_newer_green_run() {
        let repo = repo("older-red", false, |_, newer| newer.to_owned());
        let (_, log) = report(&repo, &repo.older, "failure");
        assert!(writes(&log).is_empty(), "an older commit opens no issue: {log}");
        let _ = fs::remove_dir_all(&repo.dir);
    }

    #[test]
    fn a_newer_green_run_closes_the_issue_and_moves_the_marker() {
        let repo = repo("newer-green", true, |older, _| older.to_owned());
        let (_, log) = report(&repo, &repo.newer, "success");
        let w = writes(&log);
        assert!(w.iter().any(|l| l.starts_with("issue close 7")), "a newer green run closes the issue: {log}");
        let edit = fs::read_to_string(repo.dir.join("edit")).expect("the marker is written");
        assert!(edit.contains(&format!("<!-- post-merge-last-sha: {} -->", repo.newer)), "{edit}");
        assert!(!edit.contains(&repo.older), "the old marker is replaced: {edit}");
        let _ = fs::remove_dir_all(&repo.dir);
    }

    #[test]
    fn the_same_commit_reports_again() {
        // A re-run of the same commit is not older than itself.
        let repo = repo("same", true, |_, newer| newer.to_owned());
        let (_, log) = report(&repo, &repo.newer, "failure");
        assert!(writes(&log).iter().any(|l| l.starts_with("issue comment 7")), "a re-run reports: {log}");
        let _ = fs::remove_dir_all(&repo.dir);
    }

    #[test]
    fn a_green_run_without_an_open_issue_records_its_commit() {
        // A newer green run with no open issue still moves the marker of the
        // last (closed) rolling issue, so an older red run cannot open one.
        let repo = repo("green-closed", false, |older, _| older.to_owned());
        let (_, log) = report(&repo, &repo.newer, "success");
        assert!(!log.contains("issue close"), "nothing to close: {log}");
        let edit = fs::read_to_string(repo.dir.join("edit")).expect("the marker is written");
        assert!(edit.starts_with("issue edit 7"), "{edit}");
        assert!(edit.contains(&format!("<!-- post-merge-last-sha: {} -->", repo.newer)), "{edit}");
        let _ = fs::remove_dir_all(&repo.dir);
    }

    #[test]
    fn a_new_red_issue_carries_the_marker() {
        let repo = repo("new-red", false, |older, _| older.to_owned());
        let (_, log) = report(&repo, &repo.newer, "failure");
        assert!(log.contains("issue create"), "a newer red run opens the issue: {log}");
        assert!(log.contains(&format!("<!-- post-merge-last-sha: {} -->", repo.newer)), "{log}");
        let _ = fs::remove_dir_all(&repo.dir);
    }
}
