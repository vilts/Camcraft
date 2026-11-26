//! Provenance: issue #2388's stale collaborator review marker regression, exercised through the real CLI.
#![cfg(unix)]

#[path = "merge_review/trial.rs"]
mod trial;

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::process::{Command, Output};
use std::sync::atomic::{AtomicUsize, Ordering};

const HEAD: &str = "0f4837f4c00000000000000000000000000000000";
const STALE: &str = "f64a7639100000000000000000000000000000000";
static NEXT: AtomicUsize = AtomicUsize::new(0);

struct Fixture(PathBuf);

impl Fixture {
    fn new(comments: &str, reviews: &str) -> Self {
        let path = std::env::temp_dir().join(format!(
            "camcraft-review-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&path).unwrap();
        for (name, body) in [("comments", comments), ("reviews", reviews)] {
            fs::write(path.join(name), body).unwrap();
        }
        fs::write(path.join("head"), HEAD).unwrap();
        let fixture = Self(path);
        fixture.script(
            "gh",
            r#"#!/bin/sh
printf '%s\n' "$*" >> "$REVIEW_FIXTURE/calls"
case "$1 $2" in
  'pr list') printf '2377\tissue/example\tmain\tmerge-ready\ttool.rs\tMERGEABLE\tExample fix\n' ;;
  'api repos/NeoTech/Camcraft/collaborators') printf 'reviewer\nmkinney\n' ;;
  'api repos/NeoTech/Camcraft/issues/2377/comments') cat "$REVIEW_FIXTURE/comments" ;;
  'api repos/NeoTech/Camcraft/pulls/2377/reviews') cat "$REVIEW_FIXTURE/reviews" ;;
  *) printf 'unexpected gh call: %s\n' "$*" >&2; exit 1 ;;
esac
"#,
        );
        fixture.script(
            "git",
            r#"#!/bin/sh
shift 2
case "$1" in
  fetch|update-ref) exit 0 ;;
  rev-parse)
    case "$2" in
      refs/merge-plan/2377) cat "$REVIEW_FIXTURE/head" ;;
      refs/remotes/upstream/main) printf '1111111111111111111111111111111111111111\n' ;;
      *) exit 1 ;;
    esac ;;
  merge-tree) printf '2222222222222222222222222222222222222222\n' ;;
  *) printf 'unexpected git call: %s\n' "$*" >&2; exit 1 ;;
esac
"#,
        );
        fixture
    }

    fn script(&self, name: &str, body: &str) {
        let path = self.0.join(name);
        fs::write(&path, body).unwrap();
        fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
    }

    fn run(&self, json: bool) -> Output {
        let mut paths = vec![self.0.clone()];
        paths.extend(std::env::split_paths(
            &std::env::var_os("PATH").unwrap_or_default(),
        ));
        let mut command = Command::new(env!("CARGO_BIN_EXE_xtask"));
        command
            .args([
                "merge-plan",
                "2377",
                "--repo",
                "NeoTech/Camcraft",
                "--remote",
                "upstream",
            ])
            .env("PATH", std::env::join_paths(paths).unwrap())
            .env("REVIEW_FIXTURE", &self.0);
        if json {
            command.arg("--json");
        }
        let output = command.output().unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        output
    }

    fn assert_both_endpoints(&self) {
        let calls = fs::read_to_string(self.0.join("calls")).unwrap();
        for endpoint in [
            "collaborators",
            "issues/2377/comments",
            "pulls/2377/reviews",
        ] {
            assert!(
                calls
                    .lines()
                    .any(|line| line.contains(endpoint) && line.contains("--paginate")),
                "missing paginated {endpoint}: {calls}"
            );
        }
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn marker(login: &str, date: &str, sha: &str) -> String {
    format!("{login}\t2026-11-{date}T12:00:00Z\t{sha}\n")
}

fn assert_warning(output: &Output) {
    let stderr = String::from_utf8_lossy(&output.stderr);
    for text in [
        "WARNING",
        "#2377",
        "stale review marker",
        "f64a76391",
        "0f4837f4c",
        "re-check",
    ] {
        assert!(
            stderr.contains(text),
            "missing {text:?} in stderr: {stderr}"
        );
    }
}

#[test]
fn stale_review_marker_warns_from_issue_comment() {
    let fixture = Fixture::new(&marker("reviewer", "17", STALE), "");
    let output = fixture.run(false);
    assert_warning(&output);
    assert!(String::from_utf8_lossy(&output.stdout).contains("WAVE 1"));
    fixture.assert_both_endpoints();
}

#[test]
fn stale_review_marker_warns_from_latest_pull_review() {
    let fixture = Fixture::new(
        &marker("reviewer", "16", HEAD),
        &marker("reviewer", "17", STALE),
    );
    let output = fixture.run(true);
    assert_warning(&output);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.starts_with("{\"base\":"), "{stdout}");
    assert!(
        !stdout.contains("WARNING"),
        "warnings must leave JSON stdout intact"
    );
    fixture.assert_both_endpoints();
}

#[test]
fn latest_current_marker_supersedes_older_stale_review() {
    let fixture = Fixture::new(
        &marker("reviewer", "18", HEAD),
        &marker("reviewer", "17", STALE),
    );
    let output = fixture.run(false);
    assert!(
        output.stderr.is_empty(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    fixture.assert_both_endpoints();
}

#[test]
fn outsider_marker_does_not_supersede_collaborator() {
    let fixture = Fixture::new(
        &marker("reviewer", "17", STALE),
        &marker("outsider", "18", HEAD),
    );
    assert_warning(&fixture.run(false));
}

#[test]
fn pending_review_marker_does_not_supersede_published_review() {
    let pending = format!("reviewer\t\t{HEAD}\n");
    let fixture = Fixture::new(&marker("reviewer", "17", STALE), &pending);
    assert_warning(&fixture.run(false));
    let pending_only = format!("reviewer\t\t{STALE}\n");
    let fixture = Fixture::new("", &pending_only);
    let output = fixture.run(false);
    assert!(
        output.stderr.is_empty(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn current_short_marker_and_absent_marker_do_not_warn() {
    for comments in [
        marker("reviewer", "17", "0f4837f4c"),
        String::new(),
        marker("outsider", "17", STALE),
    ] {
        let fixture = Fixture::new(&comments, "");
        let output = fixture.run(false);
        assert!(
            output.stderr.is_empty(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        fixture.assert_both_endpoints();
    }
}
