//! Provenance: issue #1951 - a debug suite step starts its test executable from the build's cargo JSON
//! artifact list, with the environment of a `cargo test` process, and never starts cargo itself.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::exec::Run;
use crate::plan::{self, Plan};

struct Cleanup(PathBuf);
impl Drop for Cleanup {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// A one-crate workspace (`camcraft-post`) in a git checkout: a lib test, a green and a red integration
/// binary. The green one asserts the working directory and the variables that cargo gives a test process.
fn fixture(tag: &str) -> (PathBuf, Cleanup) {
    let fx = std::env::temp_dir().join(format!("xtask-direct-{tag}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&fx);
    let write = |rel: &str, body: &str| {
        let p = fx.join(rel);
        fs::create_dir_all(p.parent().expect("parent")).expect("dir");
        fs::write(p, body).expect("write");
    };
    write("Cargo.toml", "[workspace]\nmembers = [\"crates/post\"]\nresolver = \"3\"\n");
    write(".gitignore", "target/\ngate-logs/\nCargo.lock\n");
    write(
        "crates/post/Cargo.toml",
        "[package]\nname = \"camcraft-post\"\nversion = \"0.3.1\"\nedition = \"2021\"\ndescription = \"fixture\"\n",
    );
    write("crates/post/src/lib.rs", "pub fn p() -> u8 { 7 }\n#[cfg(test)]\nmod t {\n    #[test]\n    fn lib_ok() { assert_eq!(super::p(), 7); }\n}\n");
    write("crates/post/src/main.rs", "fn main() {}\n");
    write(
        "crates/post/tests/green.rs",
        r#"#[test]
fn green_sees_the_cargo_environment() {
    let var = |k: &str| std::env::var(k).unwrap_or_else(|_| format!("<{k} not set>"));
    let dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    assert_eq!(std::env::current_dir().unwrap().canonicalize().unwrap(), dir.canonicalize().unwrap());
    assert_eq!(var("CARGO_MANIFEST_DIR"), env!("CARGO_MANIFEST_DIR"));
    assert_eq!(var("CARGO_PKG_NAME"), "camcraft-post");
    assert_eq!(var("CARGO_PKG_VERSION"), "0.3.1");
    assert_eq!(var("CARGO_PKG_VERSION_MINOR"), "3");
    assert_eq!(var("CARGO_PKG_DESCRIPTION"), "fixture");
    assert!(std::path::Path::new(env!("CARGO_BIN_EXE_camcraft-post")).is_file());
}
"#,
    );
    write("crates/post/tests/red.rs", "#[test]\nfn red_fails() { panic!(\"red on purpose\"); }\n");
    for args in [vec!["init", "--quiet"], vec!["add", "."], vec!["-c", "user.name=fixture", "-c", "user.email=fixture@example.invalid", "commit", "--quiet", "-m", "fixture"]] {
        assert!(Command::new("git").args(args).current_dir(&fx).status().expect("git fixture").success());
    }
    (fx.clone(), Cleanup(fx))
}

fn log_of(run: &Run, stem: &str) -> String {
    let file = fs::read_dir(&run.log_dir)
        .expect("log dir")
        .filter_map(Result::ok)
        .map(|e| e.path())
        .find(|p| p.file_name().is_some_and(|n| n.to_string_lossy().ends_with(&format!("-{stem}.log"))))
        .unwrap_or_else(|| panic!("no log for {stem}"));
    fs::read_to_string(file).expect("read log")
}

/// cargo prints `Finished` each time it starts for a step; a direct run of the test executable does not.
fn cargo_started(log: &str) -> bool {
    log.lines().any(|l| l.trim_start().starts_with("Finished"))
}

fn suites(fx: &Path) -> Vec<plan::Step> {
    plan::run_suites(fx, &["post"], true)
}

#[test]
fn batch_suite_steps_run_the_built_test_executables_without_a_cargo_start() {
    let (fx, _cleanup) = fixture("run");
    let mut steps = vec![plan::debug_build_crate("post")];
    steps.extend(suites(&fx));
    let names: Vec<&str> = steps.iter().map(|s| s.name.as_str()).collect();
    assert_eq!(names, ["debug build: post (--no-run)", "post suites: lib", "post suites: green", "post suites: red"], "the steps keep their names");
    let mut run = Run::new(&fx, "batch", false).expect("runner");
    run.plan(&Plan { notes: Vec::new(), steps }).expect("execute");
    let done: Vec<(String, bool, String)> = run.done.iter().map(|d| (d.name.clone(), d.ok, d.note.clone())).collect();
    assert_eq!(done.len(), 4, "{done:?}");
    assert!(done[0].1, "the build is green: {done:?}");
    assert!(cargo_started(&log_of(&run, "debug_build_post_--no-run_")), "the one build step is a cargo start");
    for (stem, head) in [("post_suites_lib", "unittests src/lib.rs"), ("post_suites_green", "tests/green.rs"), ("post_suites_red", "tests/red.rs")] {
        let log = log_of(&run, stem);
        assert!(!cargo_started(&log), "{stem}: the step must start the test executable, not cargo:\n{log}");
        assert!(log.lines().any(|l| l.starts_with(&format!("     Running {head} ("))), "{stem}: the log keeps cargo's Running line:\n{log}");
        assert!(log.contains("test result:"), "{stem}: the log keeps the libtest output:\n{log}");
    }
    assert!(done[1].1 && done[2].1, "lib and green pass, with the cargo environment and the package cwd: {done:?}\n{}", log_of(&run, "post_suites_green"));
    assert!(!done[3].1, "a red test executable fails its step: {done:?}");
    assert_eq!(done[3].2, "failed: tests/red.rs [tests: red_fails]", "the note names the suite and the test as before");
    assert!(crate::select::log_failed(&log_of(&run, "post_suites_red")), "--failed still finds the red step");
    assert_eq!(run.summary(), 1);
}

#[test]
fn a_selected_suite_step_without_its_build_step_starts_cargo_as_before() {
    let (fx, _cleanup) = fixture("only");
    let p = Plan { notes: Vec::new(), steps: suites(&fx) };
    let one = crate::select::apply(p, &["post suites: green".into()], None).expect("--only");
    let mut run = Run::new(&fx, "batch", false).expect("runner");
    run.plan(&one).expect("execute");
    assert!(run.done.len() == 1 && run.done[0].ok, "the step builds and runs through cargo");
    assert!(cargo_started(&log_of(&run, "post_suites_green")), "no artifact list in this run: cargo runs the step");
    assert_eq!(run.summary(), 0);
}
