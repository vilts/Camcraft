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

#[test]
fn release_waves_mix_profiles_with_a_four_lane_bound_and_no_omissions() {
    let mut steps = Vec::new();
    for i in 0..7 {
        steps.push(Step::new(format!("debug {i}"), &["test", "-p", "camcraft-motion"], ""));
        steps.push(Step::new(format!("release {i}"), &["test", "--release", "-p", "camcraft-motion"], ""));
    }
    let waves = lanes::waves(&steps, 4);
    assert_eq!(waves[0].len(), 4, "independent suites must run concurrently");
    assert!(waves.iter().all(|wave| !wave.is_empty() && wave.len() <= 4));
    assert!(waves.iter().any(|wave| {
        wave.iter().any(|&i| steps[i].args.iter().any(|a| a == "--release"))
            && wave.iter().any(|&i| !steps[i].args.iter().any(|a| a == "--release"))
    }), "debug and release suites must share a wave");
    let mut seen: Vec<_> = waves.into_iter().flatten().collect();
    seen.sort_unstable();
    assert_eq!(seen, (0..steps.len()).collect::<Vec<_>>());
}

#[test]
fn release_builds_and_watchdog_or_window_suites_run_alone() {
    let steps = vec![
        Step::new("ordinary", &["test", "-p", "camcraft-motion"], ""),
        Step::new("debug build", &["test", "--no-run"], ""),
        Step::new("ui", &["test", "-p", "camcraft-ui"], ""),
        Step::new("app", &["test", "-p", "camcraft-app"], ""),
        Step::new("kernel", &["test", "-p", "camcraft-kernel"], ""),
        Step::new("clippy", &["clippy"], ""),
        Step::new("release build", &["test", "--release", "--no-run"], ""),
        Step::new("ui cwd", &["test"], "ui"),
    ];
    let waves = lanes::waves(&steps, 4);
    assert_eq!(&waves[..2], &[vec![1], vec![6]], "both builds precede every run");
    for &i in &[1, 2, 3, 4, 5, 6, 7] {
        assert!(waves.contains(&vec![i]), "{} must run alone", steps[i].name);
    }
    let mut seen: Vec<_> = waves.into_iter().flatten().collect();
    seen.sort_unstable();
    assert_eq!(seen, (0..steps.len()).collect::<Vec<_>>());
}

#[test]
fn selected_release_without_builds_still_keeps_watchdogs_alone() {
    let steps = vec![
        Step::new("ordinary", &["test", "-p", "camcraft-motion"], ""),
        Step::new("selected app", &["test", "--release", "-p", "camcraft-app"], ""),
        Step::new("selected ui", &["test"], "ui"),
    ];
    assert_eq!(lanes::waves(&steps, 4), vec![vec![0], vec![1], vec![2]]);
    assert!(lanes::waves(&[], 4).is_empty());
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
