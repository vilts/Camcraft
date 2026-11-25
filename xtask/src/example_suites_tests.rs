//! Pins the quick tier's shipped-example rule (#3521): a post script, machine file, shipped example or
//! engine change runs the suites that post the shipped examples; a docs-only change does not.
//! Provenance: our own build law (AGENTS.md "Test the change, not everything").

use std::path::{Path, PathBuf};

use crate::example_suites::SUITES;
use crate::quick::{self, Diff, Quick};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).parent().map(Path::to_path_buf).unwrap_or_default()
}

/// The step names of the quick plan for a diff of `files` against the real tree.
fn step_names(files: &[&str]) -> Vec<String> {
    let changed: Vec<String> = files.iter().map(|s| s.to_string()).collect();
    let hunks = |_: &str| -> Vec<String> { Vec::new() };
    let diff = Diff { changed: &changed, untracked: &[], hunks: &hunks, before: &|_| None };
    match quick::plan(&root(), &diff, false) {
        Ok(Quick::Plan(p)) => p.steps.into_iter().map(|s| s.name).collect(),
        other => panic!("{files:?} did not plan: {other:?}"),
    }
}

const FOUR: [&str; 4] = [
    "app suites: example_catalog",
    "app suites: example_catalog_adaptive",
    "app suites: example_catalog_edge_work",
    "app suites: first_part_programs",
];

fn has_all(names: &[String], want: &[&str]) {
    for w in want {
        assert!(names.iter().any(|n| n == w), "{w} missing from {names:?}");
    }
}

#[test]
fn every_listed_example_suite_exists() {
    for bin in SUITES {
        assert!(root().join(format!("crates/app/tests/{bin}.rs")).is_file(), "{bin} is not an app test binary");
    }
}

#[test]
fn a_post_script_change_selects_the_shipped_example_suites() {
    has_all(&step_names(&["assets/posts/grblhal.cps"]), &FOUR);
}

#[test]
fn a_machine_file_change_selects_the_shipped_example_suites() {
    has_all(&step_names(&["assets/machines/3axis-vmc.mch"]), &FOUR);
}

#[test]
fn a_shipped_program_change_selects_first_part_programs() {
    let names = step_names(&["examples/first-router-part.nc"]);
    has_all(&names, &["app suites: first_part_programs", "app suites: first_router_part"]);
}

#[test]
fn a_catalog_case_change_selects_every_catalog_suite() {
    has_all(&step_names(&["examples/catalog/2d/first-part/case.toml"]), &FOUR);
}

#[test]
fn a_post_engine_change_selects_the_shipped_example_suites() {
    has_all(&step_names(&["crates/post/src/post/cnc.rs"]), &FOUR);
}

#[test]
fn a_docs_only_change_selects_no_shipped_example_suite() {
    let names = step_names(&["docs/decisions/README.md"]);
    for bin in SUITES {
        let step = format!("app suites: {bin}");
        assert!(!names.contains(&step), "{step} selected for a docs-only change: {names:?}");
    }
}
