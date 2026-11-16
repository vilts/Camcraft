//! Pins the one-step-for-each-test-binary plans of the debug suites (#1941): no test target is dropped,
//! a step names one target, and `--only` / `--failed` can pick one such step.
//! Provenance: our own build law (AGENTS.md "Three gate tiers"); reads only this repository's test layout.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use crate::plan::{self, ALL_CRATES, BATCH_CRATES, Plan, Step};
use crate::{scan, select};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).parent().map(Path::to_path_buf).unwrap_or_default()
}

/// The `-p <crate>` selectors (`--lib`, `--bins`, `--test x`, `--doc`) of the `<d> suites: ...` steps.
fn targets(p: &Plan, d: &str) -> BTreeSet<String> {
    let prefix = format!("{d} suites: ");
    p.steps
        .iter()
        .filter(|s| s.name.starts_with(&prefix))
        .map(|s| {
            let end = s.args.iter().position(|a| a == "--").unwrap_or(s.args.len());
            let sel = s.args[..end].iter().skip_while(|a| !a.starts_with("--lib") && *a != "--bins" && *a != "--test" && *a != "--doc");
            sel.cloned().collect::<Vec<_>>().join(" ")
        })
        .collect()
}

fn one_target(s: &Step) -> bool {
    let n = s.args.iter().filter(|a| matches!(a.as_str(), "--lib" | "--bins" | "--doc" | "--test")).count();
    n == 1
}

#[test]
fn release_runs_every_target_of_every_crate_one_step_each() {
    let root = root();
    let p = plan::release(&root);
    for d in ALL_CRATES {
        let mut want: BTreeSet<String> = ["--lib", "--bins", "--doc"].iter().map(|s| s.to_string()).collect();
        want.extend(scan::binaries(&root, d).into_iter().map(|b| format!("--test {b}")));
        assert_eq!(targets(&p, d), want, "release targets of {d}");
    }
    assert!(p.steps.iter().filter(|s| s.name.contains(" suites: ")).all(one_target));
}

#[test]
fn batch_runs_lib_and_each_non_slow_binary_one_step_each() {
    let root = root();
    let p = plan::batch(&root);
    for d in BATCH_CRATES.iter().chain(&["ui"]) {
        let mut want: BTreeSet<String> = BTreeSet::new();
        if *d == "ui" {
            want.extend(["--bins", "--doc"].iter().map(|s| s.to_string()));
        }
        want.insert("--lib".into());
        want.extend(
            scan::binaries(&root, d).into_iter().filter(|b| *d == "ui" || !plan::is_slow(d, b)).map(|b| format!("--test {b}")),
        );
        assert_eq!(targets(&p, d), want, "batch targets of {d}");
    }
    assert!(p.steps.iter().filter(|s| s.name.contains(" suites: ")).all(one_target));
    // The ui steps keep the crates/ui working directory; the build step comes first.
    assert!(p.steps.iter().filter(|s| s.name.starts_with("ui suites: ")).all(|s| s.in_ui));
    assert_eq!(p.steps[0].name, "debug build (--no-run)");
}

#[test]
fn only_and_failed_select_one_binary_step() {
    let root = root();
    let p = plan::batch(&root);
    let one = select::apply(p.clone(), &["app suites: lib".into()], None).expect("only");
    assert_eq!(one.steps.len(), 1);
    assert_eq!(one.steps[0].name, "app suites: lib");
    let stems = vec!["app_suites_lib".to_string()];
    let again = select::apply(p, &[], Some(&stems)).expect("failed");
    assert_eq!(again.steps.len(), 1);
}
