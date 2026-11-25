//! Tests of `build_stop` (#3601): a failed quick per-crate test build stops the plan, and a metadata error
//! of rustc gets one diagnostic block.

use crate::build_stop::{metadata_note, stops};
use crate::plan::{Plan, Step};

fn args(a: &[&str]) -> Vec<String> {
    a.iter().map(|s| s.to_string()).collect()
}

const UI_BUILD: [&str; 7] = ["test", "-j", "16", "-p", "camcraft-ui", "--no-run", "--lib"];

#[test]
fn a_failed_per_crate_build_stops_the_plan_like_the_whole_gate_build() {
    assert!(stops(crate::plan::DEBUG_BUILD));
    assert!(stops("ui build (--no-run)"));
    assert!(stops("app build (--no-run)"));
    assert!(!stops("ui suites: lib"));
    assert!(!stops("xtask unit tests"));
}

#[test]
fn metadata_errors_name_the_crate_the_jobs_and_the_package_to_clean() {
    let logs = [
        ("error[E0786]: found invalid metadata files for crate `accesskit` which `camcraft_ui` depends on", "`accesskit`"),
        ("error[E0786]: found invalid metadata files for crate `test`", "`test`"),
        ("error[E0460]: found possibly newer version of crate `png` which `camcraft_app` depends on", "`png`"),
    ];
    for (log, krate) in logs {
        let block = metadata_note(log, &args(&UI_BUILD), "gate quick").expect("pattern");
        let text = block.join("\n");
        assert!(text.contains(&format!("crate {krate}")), "the crate of the error must be named: {text}");
        assert!(text.contains("not the tests"), "{text}");
        assert!(text.contains("CAMCRAFT_GATE_JOBS=4 cargo xtask gate quick"), "{text}");
        assert!(text.contains("cargo clean -p camcraft-ui"), "the step's package, not the crate of the error: {text}");
        assert!(!text.contains(&format!("cargo clean -p {}", krate.trim_matches('`'))), "{text}");
    }
    // The code alone (a log cut short) still gives the block.
    assert!(!metadata_note("error[E0460]", &args(&UI_BUILD), "gate quick").expect("pattern").is_empty());
}

#[test]
fn an_ordinary_compile_error_gives_no_metadata_block() {
    let log = "error[E0308]: mismatched types\n  --> src/lib.rs:3:5\nerror: could not compile `camcraft-ui` (lib) due to 1 previous error";
    assert!(metadata_note(log, &args(&UI_BUILD), "gate quick").expect("pattern").is_empty());
    assert!(metadata_note("", &args(&UI_BUILD), "gate quick").expect("pattern").is_empty());
}

/// A quick plan whose per-crate build fails: the suite after it does not start, it is listed as not run,
/// and the gate exits red. The build fails on a bad cargo flag, so nothing compiles.
#[test]
fn quick_plan_stops_after_a_failed_per_crate_build() {
    use std::{fs, process::Command};
    struct Cleanup(std::path::PathBuf);
    impl Drop for Cleanup {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    let root = std::env::temp_dir().join(format!("xtask-build-stop-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    let _cleanup = Cleanup(root.clone());
    fs::create_dir_all(root.join("src")).expect("fixture dir");
    fs::write(root.join("Cargo.toml"), "[package]\nname = \"build-stop-fixture\"\nversion = \"0.1.0\"\nedition = \"2021\"\n[workspace]\n").expect("manifest");
    fs::write(root.join("src/lib.rs"), "").expect("lib");
    fs::write(root.join(".gitignore"), "target/\ngate-logs/\n").expect("ignore");
    for a in [vec!["init", "--quiet"], vec!["add", "."], vec!["-c", "user.name=fixture", "-c", "user.email=fixture@example.invalid", "commit", "--quiet", "-m", "fixture"]] {
        assert!(Command::new("git").args(a).current_dir(&root).status().expect("git fixture").success());
    }
    let mut run = crate::exec::Run::new(&root, "quick", false).expect("runner");
    assert!(!run.lanes, "quick runs its steps one at a time");
    let steps = vec![
        Step::new("ui build (--no-run)", &["test", "--no-run", "--invalid-build-flag"], ""),
        Step::new("ui suites: lib", &["test", "--invalid-suite-flag"], ""),
        Step::new("ui suites: boundary", &["test", "--invalid-suite-flag"], ""),
    ];
    run.plan(&Plan { notes: Vec::new(), steps: steps.clone() }).expect("execute fixture");
    assert_eq!(run.done.len(), 1, "no step may start after the failed build");
    assert!(!run.done[0].ok);
    let names: Vec<&str> = run.not_run.iter().map(|(n, _)| n.as_str()).collect();
    assert_eq!(names, ["ui suites: lib", "ui suites: boundary"]);
    assert!(run.not_run.iter().all(|(_, note)| note == "not run (build failed: ui build (--no-run))"));
    // A rerun with `--failed` on this run's logs selects the build step and each stopped step.
    let failed = crate::select::failed_in_dir(&run.log_dir).expect("failed logs");
    let again = crate::select::apply(Plan { notes: Vec::new(), steps }, &[], Some(&failed)).expect("selection");
    let names: Vec<&str> = again.steps.iter().map(|s| s.name.as_str()).collect();
    assert_eq!(names, ["ui build (--no-run)", "ui suites: lib", "ui suites: boundary"]);
    assert_eq!(run.summary(), 1, "the gate exits red");
}

#[test]
fn a_test_step_that_quotes_a_metadata_error_gives_no_block() {
    let log = "error[E0786]: found invalid metadata files for crate `accesskit`
test result: FAILED. 0 passed; 1 failed";
    let suite = args(&["test", "-j", "16", "-p", "camcraft-ui", "--test", "boundary"]);
    assert!(metadata_note(log, &suite, "gate quick").expect("pattern").is_empty());
    assert!(!metadata_note(log, &args(&UI_BUILD), "gate quick").expect("pattern").is_empty());
}
