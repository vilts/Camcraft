//! Pins the release tier's heavy lanes: one step for each test target, the same tests as the old single step (#1714).
//! Provenance: our own build law (AGENTS.md "Three gate tiers"); libtest's `--ignored` and cargo's target selectors.

use std::collections::BTreeSet;
use std::path::PathBuf;

use crate::plan::{self, Step};
use crate::{exec, scan, select};

fn root() -> PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("..")
}

fn heavy() -> Vec<Step> {
    plan::release(&root()).steps.into_iter().filter(|s| s.name.contains("heavy")).collect()
}

#[test]
fn heavy_lanes_are_one_build_then_one_step_per_target() {
    let steps = heavy();
    assert_eq!(steps[0].name, "heavy build (release --no-run)");
    assert!(steps[0].args.contains(&"--no-run".to_string()) && steps[0].args.contains(&"--release".to_string()));
    let names: Vec<&str> = steps.iter().map(|s| s.name.as_str()).collect();
    let unique: BTreeSet<&str> = names.iter().copied().collect();
    assert_eq!(unique.len(), names.len(), "step names must be unique: {names:?}");
    assert!(!names.iter().any(|n| n.ends_with("heavy lane (release --ignored)")), "{names:?}");
    for want in ["app heavy: lib", "app heavy: doc", "motion heavy: bins", "app heavy: pocket3d_hsm", "motion heavy: finish3d"] {
        assert!(names.contains(&want), "{want} missing in {names:?}");
    }
}

#[test]
fn heavy_steps_skip_only_binaries_without_ignored_tests() {
    // The old step was `cargo test --release -p <crate> --no-fail-fast -- --test-threads=6 --ignored`: it ran the
    // lib, the bins, every tests/ binary and the doc tests. The lib, bins and doc steps are always planned; a
    // tests/ binary gets a step when any of its files holds an `#[ignore]` (the others run no test), none dropped.
    for d in plan::HEAVY_CRATES {
        let pkg = plan::pkg(d);
        let all = heavy();
        let mine: Vec<&Step> = all.iter().filter(|s| s.args.contains(&pkg) && !s.args.contains(&"--no-run".to_string())).collect();
        let mut sel: Vec<String> = Vec::new();
        for s in &mine {
            assert!(s.args.ends_with(&["--".into(), plan::T.into(), "--ignored".into()]), "{}: {:?}", s.name, s.args);
            assert!(s.args.contains(&"--release".to_string()) && s.args.contains(&"--no-fail-fast".to_string()));
            let i = s.args.iter().position(|a| a == "--no-fail-fast").unwrap_or(0) + 1;
            let target: Vec<&str> = s.args[i..].iter().take_while(|a| *a != "--").map(String::as_str).collect();
            sel.push(target.join(" "));
        }
        // Every binary that holds an `#[ignore]` anywhere in its files must have its step.
        let tests = root().join("crates").join(d).join("tests");
        let bins = scan::binaries(&root(), d);
        let mut want: Vec<String> = vec!["--lib".into(), "--bins".into(), "--doc".into()];
        for f in scan::rs_files_recursive(&tests) {
            if !scan::read(&f).contains("#[ignore") {
                continue;
            }
            let rel = f.strip_prefix(&tests).map(PathBuf::from).unwrap_or_default();
            let first = rel.components().next().map(|c| c.as_os_str().to_string_lossy().into_owned()).unwrap_or_default();
            let bin = first.trim_end_matches(".rs").to_string();
            if bins.contains(&bin) {
                want.push(format!("--test {bin}"));
            }
        }
        want.sort();
        want.dedup();
        sel.sort();
        let missing: Vec<&String> = want.iter().filter(|w| !sel.contains(w)).collect();
        assert!(missing.is_empty(), "{d}: binaries with ignored tests have no heavy step: {missing:?}");
        for s in ["--lib", "--bins", "--doc"] {
            assert!(sel.iter().any(|x| x == s), "{d}: {s} step missing");
        }
    }
}

#[test]
fn kernel_ignored_binaries_have_release_heavy_steps() {
    let root = root();
    let tests = root.join("crates/kernel/tests");
    let bins = scan::binaries(&root, "kernel");
    let mut ignored = BTreeSet::new();
    for file in scan::rs_files_recursive(&tests) {
        let text = std::fs::read_to_string(&file).expect("read kernel test source");
        if !text.contains("#[ignore") {
            continue;
        }
        let rel = file.strip_prefix(&tests).expect("kernel test path");
        let first = rel.components().next().expect("test source name");
        let bin = first.as_os_str().to_string_lossy().trim_end_matches(".rs").to_string();
        if bins.contains(&bin) {
            ignored.insert(bin);
        }
    }
    assert!(!ignored.is_empty(), "kernel ignored-test fixture must exercise the planner");
    let release = plan::release(&root);
    for bin in ignored {
        let name = format!("kernel heavy: {bin}");
        let steps: Vec<&Step> = release.steps.iter().filter(|s| s.name == name).collect();
        assert_eq!(steps.len(), 1, "{name} must have exactly one release step");
        assert_eq!(steps[0].args, [
            "test", "--release", "-j", plan::J, "-p", "camcraft-kernel", "--no-fail-fast",
            "--test", &bin, "--", plan::T, "--ignored",
        ]);
    }
    let build = release.steps.iter().find(|s| s.name == "heavy build (release --no-run)").expect("heavy prebuild");
    assert!(build.args.windows(2).any(|w| w == ["-p", "camcraft-kernel"]), "kernel needs the release prebuild");
}

#[test]
fn heavy_only_selects_one_binary_and_the_failed_log_picks_it_back() {
    let p = plan::release(&root());
    let one = select::apply(p, &["app heavy: pocket3d_hsm".to_string()], None).expect("one binary");
    assert_eq!(one.steps.len(), 1);
    assert!(one.steps[0].args.windows(2).any(|w| w == ["--test", "pocket3d_hsm"]));
    let stem = exec::log_name(0, &one.steps[0].name);
    let stem = stem.split_once('-').map(|x| x.1.trim_end_matches(".log").to_string()).unwrap_or_default();
    let failed = select::apply(plan::release(&root()), &[], Some(&[stem])).expect("failed");
    assert_eq!(failed.steps.len(), 1);
}

#[test]
fn log_note_names_failed_tests_and_slow_tests() {
    let log = "     Running tests/shared_face_bodies.rs (t)\ntest a::slow_one has been running for over 60 seconds\ntest a::bad ... FAILED\ntest a::good ... ok\ntest result: FAILED. 1 passed\n";
    assert_eq!(
        exec::read_log(log, 101).expect("note"),
        "failed: tests/shared_face_bodies.rs [tests: a::bad] slow: a::slow_one (>60s)"
    );
    let ok = "test b::long has been running for over 120 seconds\ntest result: ok. 1 passed\n";
    assert_eq!(exec::read_log(ok, 0).expect("note"), "slow: b::long (>120s)");
}
