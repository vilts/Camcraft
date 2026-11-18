//! Provenance: release-gate prebuild requirements from issue #1703.

use crate::{lanes, plan::Step};

#[test]
fn release_prebuilds_both_profiles_before_running_suites() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).parent().expect("workspace");
    let plan = crate::plan::release(root);
    let first = &plan.steps[0];
    assert!(first.args.iter().any(|a| a == "--no-run"), "debug prebuild must come before any suite: {}", first.name);
    assert!(!first.args.iter().any(|a| a == "--release"));
    let second = &plan.steps[1];
    assert!(second.args.iter().any(|a| a == "--no-run"), "release prebuild must come before any suite: {}", second.name);
    assert!(second.args.iter().any(|a| a == "--release"));
}

fn none() -> std::collections::HashMap<String, u64> {
    std::collections::HashMap::new()
}

#[test]
fn release_queue_holds_every_ordinary_suite_of_both_profiles_and_loses_none() {
    let mut steps = Vec::new();
    for i in 0..7 {
        steps.push(Step::new(format!("debug {i}"), &["test", "-p", "camcraft-motion", "--test", "a"], ""));
        steps.push(Step::new(format!("release {i}"), &["test", "--release", "-p", "camcraft-motion", "--test", "a"], ""));
    }
    let s = lanes::schedule(&steps, &none());
    assert!(s.builds.is_empty() && s.alone.is_empty(), "{s:?}");
    assert_eq!(s.queue, (0..steps.len()).collect::<Vec<_>>(), "no history keeps plan order");
}

#[test]
fn release_builds_ui_clippy_and_isolated_targets_run_alone() {
    let steps = vec![
        Step::new("ordinary", &["test", "-p", "camcraft-motion", "--lib"], ""),
        Step::new("debug build", &["test", "--no-run"], ""),
        Step::new("ui", &["test", "-p", "camcraft-ui", "--lib"], ""),
        Step::new("app whole crate", &["test", "-p", "camcraft-app"], ""),
        Step::new("kernel whole crate", &["test", "-p", "camcraft-kernel"], ""),
        Step::new("clippy", &["clippy"], ""),
        Step::new("release build", &["test", "--release", "--no-run"], ""),
        Step::new("ui cwd", &["test"], "ui"),
        Step::new("app watchdog", &["test", "-p", "camcraft-app", "--test", "import_step_guard", "--", "--test-threads=6"], ""),
        Step::new("app lib", &["test", "--release", "-p", "camcraft-app", "--lib", "--", "--ignored"], ""),
        Step::new("kernel watchdog", &["test", "-p", "camcraft-kernel", "--test", "step_import"], ""),
        Step::new("app ordinary", &["test", "-p", "camcraft-app", "--test", "cmds", "--", "--test-threads=6"], ""),
        Step::new("app doc", &["test", "-p", "camcraft-app", "--doc"], ""),
        Step::new("kernel ordinary", &["test", "--release", "-p", "camcraft-kernel", "--test", "smb_roundtrip"], ""),
    ];
    let s = lanes::schedule(&steps, &none());
    assert_eq!(s.builds, vec![1, 6], "both builds come first, in plan order");
    assert_eq!(s.alone, vec![2, 3, 4, 5, 7, 8, 9, 10]);
    assert_eq!(s.queue, vec![0, 11, 12, 13], "ordinary app and kernel targets join the lanes");
}

#[test]
fn every_isolated_entry_names_a_real_target() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).parent().expect("workspace");
    for key in lanes::ISOLATED {
        let (dir, target) = key.split_once(':').expect("crate-dir:target");
        assert!(matches!(dir, "app" | "kernel"), "{key}: only app and kernel suites are listed");
        if target != "lib" {
            let file = root.join("crates").join(dir).join("tests").join(format!("{target}.rs"));
            assert!(file.is_file(), "{key}: no {}", file.display());
        }
    }
}

#[test]
fn target_keys_name_one_target_of_one_crate() {
    let k = |args: &[&str]| lanes::target_key(&Step::new("s", args, ""));
    assert_eq!(k(&["test", "-p", "camcraft-app", "--test", "x", "--", "--ignored"]).as_deref(), Some("app:x"));
    assert_eq!(k(&["test", "-p", "camcraft-kernel", "--lib"]).as_deref(), Some("kernel:lib"));
    assert_eq!(k(&["test", "-p", "camcraft-app"]), None, "a whole crate is not one target");
    assert_eq!(k(&["test", "-p", "camcraft-app", "-p", "camcraft-cl", "--lib"]), None);
    assert_eq!(k(&["run", "--", "--test", "x", "-p", "camcraft-app"]), None, "child args are not cargo's");
}

#[test]
fn queue_runs_longest_first_and_unknown_steps_early() {
    let steps: Vec<Step> = ["short", "long", "new", "mid"]
        .iter()
        .map(|n| Step::new(*n, &["test", "-p", "camcraft-motion", "--test", n], ""))
        .collect();
    let hist = lanes::parse_timings("3\tshort\n900\tlong\n40\tmid\nnot a row\nx\tbad secs\n");
    assert_eq!(hist.len(), 3, "unreadable rows are skipped: {hist:?}");
    assert_eq!(lanes::schedule(&steps, &hist).queue, vec![1, 2, 3, 0], "a step with no history ranks with the longest");
    let text = lanes::format_timings([("a b", 5), ("c", 0)]);
    assert_eq!(lanes::parse_timings(&text).get("a b"), Some(&5));
}

#[test]
fn history_reads_the_newest_release_timings_only() {
    use std::fs;
    let root = std::env::temp_dir().join(format!("xtask-history-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    assert!(lanes::history(&root).0.is_empty(), "no gate-logs: plan order");
    for (dir, body) in [("release-20261101-000000", "1\told\n"), ("release-20261102-000000", "2\tnew\n"), ("batch-20261103-000000", "3\tbatch\n")] {
        fs::create_dir_all(root.join("gate-logs").join(dir)).expect("log dir");
        fs::write(root.join("gate-logs").join(dir).join(lanes::TIMINGS), body).expect("timings");
    }
    fs::create_dir_all(root.join("gate-logs/release-20261104-000000")).expect("a run with no timings yet");
    let (h, from) = lanes::history(&root);
    assert_eq!(h.get("new"), Some(&2));
    assert!(!h.contains_key("old") && !h.contains_key("batch"), "{h:?}");
    assert!(from.is_some_and(|f| f.to_string_lossy().contains("release-20261102")));
    fs::remove_dir_all(root).expect("remove fixture");
}

#[test]
fn release_executor_overlaps_lanes_and_keeps_failure_and_checkout_guards() {
    use std::{fs, process::Command};
    let root = std::env::temp_dir().join(format!("xtask-lanes-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(root.join("src")).expect("fixture dir");
    fs::write(root.join("Cargo.toml"), "[package]\nname = \"lane-fixture\"\nversion = \"0.1.0\"\nedition = \"2021\"\n[workspace]\n").expect("manifest");
    fs::write(root.join(".gitignore"), "target/\ngate-logs/\nmarkers/\n").expect("ignore");
    fs::write(root.join("tracked"), "original").expect("tracked fixture");
    fs::write(root.join("src/main.rs"), r#"
use std::{env, fs, path::Path, thread, time::{Duration, Instant}};
fn main() {
    fs::create_dir_all("markers").unwrap();
    let args: Vec<_> = env::args().collect();
    let own = &args[1];
    fs::write(format!("markers/{own}"), "ran").unwrap();
    if own == "fail" { std::process::exit(7); }
    if own == "move" { fs::write("tracked", "changed").unwrap(); }
    if let Some(peer) = args.get(2) {
        let start = Instant::now();
        while !Path::new(&format!("markers/{peer}")).exists() {
            assert!(start.elapsed() < Duration::from_secs(5), "peer lane never started");
            thread::sleep(Duration::from_millis(10));
        }
    }
}
"#).expect("fixture source");
    let cargo = Command::new(crate::exec::cargo()).args(["build", "--quiet"]).current_dir(&root).status().expect("build fixture");
    assert!(cargo.success());
    for args in [vec!["init", "--quiet"], vec!["add", "."], vec!["-c", "user.name=fixture", "-c", "user.email=fixture@example.invalid", "commit", "--quiet", "-m", "fixture"]] {
        assert!(Command::new("git").args(args).current_dir(&root).status().expect("git fixture").success());
    }
    let run = |steps| {
        let mut runner = crate::exec::Run::new(&root, "release", false).expect("runner");
        runner.budget = crate::budget::DEFAULT; // two lanes must overlap whatever CAMCRAFT_GATE_JOBS says
        runner.plan(&crate::plan::Plan { notes: Vec::new(), steps }).expect("execute fixture");
        runner.summary()
    };
    let step = |name: &str, extra: &[&str]| Step::new(name, &["run", "--quiet", "--"], "").with(&extra.iter().map(|s| s.to_string()).collect::<Vec<_>>());
    assert_eq!(run(vec![step("first", &["first", "second"]), step("second", &["second", "first"])]), 0, "both child commands must overlap");
    assert_eq!(run(vec![step("failure", &["fail"]), step("survivor", &["survivor"])]), 1);
    assert!(root.join("markers/survivor").exists(), "a failed suite must not discard other lane results");
    assert_eq!(run(vec![Step::new("bad build", &["test", "--no-run", "--invalid-build-flag"], ""), step("must not run", &["after-build-failure"])]), 1);
    assert!(!root.join("markers/after-build-failure").exists(), "a failed prebuild must stop suites");
    // Batch runs its steps one by one: a failed whole-gate build stops the later steps too (review of #1952).
    let mut seq = crate::exec::Run::new(&root, "batch", false).expect("runner");
    let bad = Step::new(crate::plan::DEBUG_BUILD, &["test", "--no-run", "--invalid-build-flag"], "");
    seq.plan(&crate::plan::Plan { notes: Vec::new(), steps: vec![bad, step("must not run", &["after-batch-build"])] }).expect("execute fixture");
    assert_eq!(seq.summary(), 1);
    assert!(!root.join("markers/after-batch-build").exists(), "a failed batch build must stop the later steps");
    assert_eq!(run(vec![step("checkout movement", &["move"])]), 1, "successful children still yield VOID after checkout movement");
    fs::remove_dir_all(root).expect("remove fixture");
}
