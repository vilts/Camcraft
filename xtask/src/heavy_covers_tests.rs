//! Pins `gate heavy --covers <PR>:<sha>` (#3460): the flag parsing, the refusal of a head that the trial
//! tree does not contain, and the text of the header and of `covers.txt`.
//! Provenance: the maintainer's decision on #3460 (AGENTS.md "Heavy lanes at review"); a temporary git
//! repository in the system temp directory, made by the test.

use std::path::{Path, PathBuf};
use std::process::Command;

use crate::heavy_covers::{self, Cover, Trial};
use crate::parse_gate;

fn a(s: &str) -> Vec<String> {
    s.split_whitespace().map(String::from).collect()
}

const SHA: &str = "0123456789abcdef0123456789abcdef01234567";

#[test]
fn covers_is_repeatable_on_the_heavy_tier() {
    let g = parse_gate(&a(&format!("heavy --base origin/main --covers 3401:{SHA} --covers #3402:ABCDEF1 --dry-run")))
        .expect("heavy --covers");
    assert_eq!(
        g.covers,
        [Cover { pr: 3401, sha: SHA.into() }, Cover { pr: 3402, sha: "abcdef1".into() }]
    );
    assert!(g.dry);
    assert!(parse_gate(&a("heavy")).expect("heavy").covers.is_empty(), "no flag, no covers");
}

#[test]
fn covers_refuses_other_tiers_and_bad_values() {
    for tier in ["quick", "batch", "light", "release"] {
        let e = parse_gate(&a(&format!("{tier} --covers 1:{SHA}"))).expect_err("heavy only");
        assert!(e.starts_with(&format!("--covers applies to the heavy tier only, not '{tier}'")), "{e}");
        assert!(e.contains("usage:"), "the refusal carries the usage text: {e}");
    }
    let bad: Vec<(String, &str)> = vec![
        ("3401".into(), "no ':' between the PR and the sha"),
        (format!("x:{SHA}"), "the PR is not a positive number"),
        (format!("0:{SHA}"), "the PR is not a positive number"),
        ("3401:abc".into(), "the sha is not 7 to 40 hex digits"),
        ("3401:xyzxyzx".into(), "the sha is not 7 to 40 hex digits"),
        (format!("3401:{SHA}0"), "the sha is not 7 to 40 hex digits"),
    ];
    for (v, why) in &bad {
        let e = parse_gate(&a(&format!("heavy --covers {v}"))).expect_err(v);
        assert!(e.contains(why) && e.contains("usage:"), "{v}: {e}");
    }
    for sel in ["--only motion", "--failed gate-logs/heavy-1"] {
        let e = parse_gate(&a(&format!("heavy --covers 7:{SHA} {sel}"))).expect_err(sel);
        assert!(e.starts_with("--covers records a full heavy run: it does not take --only or --failed"), "{e}");
        assert!(e.contains("usage:"), "{e}");
    }
    let e = parse_gate(&a(&format!("heavy --covers 7:{SHA} --covers 7:abcdef1"))).expect_err("repeat");
    assert!(e.starts_with("--covers names #7 two times"), "{e}");
    assert!(parse_gate(&a("heavy --covers")).expect_err("no value").contains("--covers needs a value"));
}

#[test]
fn header_and_record_name_each_head_the_tree_and_the_counts() {
    let t = Trial {
        covers: vec![Cover { pr: 3401, sha: "a".repeat(40) }, Cover { pr: 3402, sha: "b".repeat(40) }],
        tree: "c".repeat(40),
        base: "origin/main".into(),
        base_sha: "d".repeat(40),
    };
    let head = heavy_covers::header(&t);
    assert_eq!(
        head,
        [
            format!("covers: #3401 {}", "a".repeat(40)),
            format!("covers: #3402 {}", "b".repeat(40)),
            format!("tree: {}", "c".repeat(40)),
            format!("base: origin/main {}", "d".repeat(40)),
        ]
    );
    let steps = vec![
        ("heavy build (release --no-run)".to_string(), true, 300, String::new()),
        ("motion heavy: lib".to_string(), true, 1200, "SKIPPED (known fail, #3100): x::y".to_string()),
        ("app heavy: ops".to_string(), false, 90, "2 failed".to_string()),
    ];
    let text = heavy_covers::record(&t, 3, &steps, 26.5, false);
    let want = format!(
        "{}\nlanes:\n  ok     300s  heavy build (release --no-run)\n  ok    1200s  motion heavy: lib  SKIPPED (known fail, #3100): x::y\n  FAIL    90s  app heavy: ops  2 failed\ncounts: 3 planned, 3 ran, 2 ok, 1 failed\nwall: 26.5 min\nverdict: FAIL\n",
        head.join("\n")
    );
    assert_eq!(text, want);
    assert!(heavy_covers::record(&t, 1, &steps[..1], 1.0, false).ends_with("verdict: PASS\n"));
    assert!(heavy_covers::record(&t, 1, &steps[..1], 1.0, true).ends_with("verdict: VOID\n"));
}

#[test]
fn not_run_is_never_a_pass_in_the_record() {
    let t = Trial { covers: vec![Cover { pr: 1, sha: "a".repeat(40) }], tree: "c".repeat(40), base: "main".into(), base_sha: "d".repeat(40) };
    // A stopped run: three steps planned, one ran green.
    let one = vec![("heavy build (release --no-run)".to_string(), true, 300, String::new())];
    let text = heavy_covers::record(&t, 3, &one, 5.0, false);
    assert!(text.contains("counts: 3 planned, 1 ran, 1 ok, 0 failed\n"), "{text}");
    assert!(text.ends_with("verdict: INCOMPLETE\n"), "{text}");
    // A wave that reaches no heavy lane says so; it is not PASS with 0 steps.
    let none = heavy_covers::record(&t, 0, &[], 0.0, false);
    assert!(none.contains(&format!("lanes:\n  {}\n", heavy_covers::NO_LANE)), "{none}");
    assert_eq!(heavy_covers::NO_LANE, "the wave reaches no heavy lane");
    assert!(none.ends_with("verdict: NO LANE\n"), "{none}");
    assert_eq!(heavy_covers::verdict(2, 2, 0, false), "PASS");
    assert_eq!(heavy_covers::verdict(2, 1, 1, false), "FAIL");
    assert_eq!(heavy_covers::verdict(2, 2, 0, true), "VOID");
}

/// A throw-away repository: `base` -> `pr` on a side branch, `main` checked out at `base`.
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

fn repo() -> (Repo, String, String) {
    let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
    let dir = std::env::temp_dir().join(format!("xtask-covers-{}-{nanos}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("temp dir");
    let r = Repo(dir);
    git(&r.0, &["init", "-q", "-b", "main"]);
    std::fs::write(r.0.join("a.txt"), "base\n").expect("write");
    git(&r.0, &["add", "a.txt"]);
    git(&r.0, &["commit", "-q", "-m", "base"]);
    let base = git(&r.0, &["rev-parse", "HEAD"]);
    git(&r.0, &["switch", "-q", "-c", "pr"]);
    std::fs::write(r.0.join("b.txt"), "pr\n").expect("write");
    git(&r.0, &["add", "b.txt"]);
    git(&r.0, &["commit", "-q", "-m", "pr"]);
    let pr = git(&r.0, &["rev-parse", "HEAD"]);
    git(&r.0, &["switch", "-q", "main"]);
    (r, base, pr)
}

#[test]
fn a_head_that_the_trial_tree_lacks_is_refused() {
    let (r, base, pr) = repo();
    let e = heavy_covers::check(&r.0, &[Cover { pr: 9, sha: pr.clone() }], "main").expect_err("not merged");
    assert!(e.contains("the head is not an ancestor of HEAD") && e.contains("#9"), "{e}");
    let e = heavy_covers::check(&r.0, &[Cover { pr: 9, sha: "deadbeef".into() }], "main").expect_err("unknown");
    assert!(e.contains("no such commit"), "{e}");

    // The trial tree: main with the PR merged (--no-ff, as the merge-ready skill makes it).
    git(&r.0, &["merge", "-q", "--no-ff", "-m", "trial", "pr"]);
    let short: String = pr.chars().take(10).collect();
    let t = heavy_covers::check(&r.0, &[Cover { pr: 9, sha: short }, Cover { pr: 8, sha: base.clone() }], "main").expect("covered");
    assert_eq!(t.covers, [Cover { pr: 9, sha: pr }, Cover { pr: 8, sha: base }], "full shas in the record");
    assert_eq!(t.tree, git(&r.0, &["rev-parse", "HEAD^{tree}"]));
    assert_eq!((t.base.as_str(), t.base_sha.clone()), ("main", git(&r.0, &["merge-base", "main", "HEAD"])));

    // A change in the working tree makes HEAD^{tree} a wrong record: refused.
    std::fs::write(r.0.join("a.txt"), "edited\n").expect("write");
    let e = heavy_covers::check(&r.0, &t.covers, "main").expect_err("dirty");
    assert!(e.starts_with("--covers needs a clean trial tree"), "{e}");
}
