//! Pins the stale gate base warning (#3739) on a throw-away repository with fake remote refs.

use std::path::{Path, PathBuf};
use std::process::Command;

use crate::stale_base;

struct Repo(PathBuf);

impl Drop for Repo {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn git(dir: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(["-c", "user.name=xtask test", "-c", "user.email=xtask@test.invalid", "-c", "commit.gpgsign=false"])
        .args(args)
        .output()
        .expect("git runs");
    assert!(out.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&out.stderr));
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

fn commit(r: &Repo, name: &str) -> String {
    std::fs::write(r.0.join(name), name).expect("write");
    git(&r.0, &["add", name]);
    git(&r.0, &["commit", "-q", "-m", name]);
    git(&r.0, &["rev-parse", "HEAD"])
}

/// `main` at `c1`; `origin/main` at `c3` (two commits ahead); `upstream/main` at `c1` (even).
fn repo() -> Repo {
    let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
    let dir = std::env::temp_dir().join(format!("xtask-stale-base-{}-{nanos}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("temp dir");
    let r = Repo(dir);
    git(&r.0, &["init", "-q", "-b", "main"]);
    let c1 = commit(&r, "c1");
    commit(&r, "c2");
    let c3 = commit(&r, "c3");
    git(&r.0, &["update-ref", "refs/remotes/origin/main", &c3]);
    git(&r.0, &["update-ref", "refs/remotes/upstream/main", &c1]);
    git(&r.0, &["reset", "-q", "--hard", &c1]);
    r
}

#[test]
fn gate_warns_when_local_main_is_behind_origin() {
    let r = repo();
    let w = stale_base::warnings(&r.0, "main");
    assert_eq!(w.len(), 1, "only origin/main is ahead: {w:?}");
    assert!(
        w[0].contains("'main' is 2 commits behind 'origin/main'") && w[0].contains("--base origin/main"),
        "{w:?}"
    );

    // A remote ref or a sha as the base: nothing to compare, no warning.
    assert!(stale_base::warnings(&r.0, "origin/main").is_empty());
    assert!(stale_base::warnings(&r.0, &git(&r.0, &["rev-parse", "HEAD"])).is_empty());

    // The local main caught up: no warning.
    git(&r.0, &["reset", "-q", "--hard", "origin/main"]);
    assert!(stale_base::warnings(&r.0, "main").is_empty());
}

#[test]
fn stale_base_warning_text_counts_commits() {
    assert_eq!(stale_base::warning("main", "upstream/main", 0), None);
    let one = stale_base::warning("main", "upstream/main", 1).expect("one behind");
    assert!(one.contains("is 1 commit behind 'upstream/main'"), "{one}");
}
