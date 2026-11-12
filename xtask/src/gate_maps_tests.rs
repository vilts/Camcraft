//! Pins the quick tier's selection against the REAL suite headers (#922): a change under a crate
//! that M4.1 split out of `app` must select the app suites whose Gate map names that module.
//! Provenance: our own build law (AGENTS.md "Test the change, not everything"), #919 research.

use std::path::{Path, PathBuf};

use crate::plan::Step;
use crate::quick::{self, Diff, Quick};
use crate::scan;

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).parent().map(Path::to_path_buf).unwrap_or_default()
}

/// The `--test` binaries of the `<crate> suites` step that quick plans for one changed file.
fn selected(file: &str, krate: &str) -> Vec<String> {
    let changed = vec![file.to_string()];
    let hunks = |_: &str| -> Vec<String> { Vec::new() };
    let diff = Diff { changed: &changed, untracked: &[], hunks: &hunks };
    let Ok(Quick::Plan(p)) = quick::plan(&root(), &diff, false) else { panic!("{file} did not plan") };
    let step: Option<&Step> = p.steps.iter().find(|s| s.name == format!("{krate} suites"));
    let Some(step) = step else { return Vec::new() };
    let a = &step.args;
    eprintln!("selection {file} -> {krate}: {} suite(s)", a.iter().filter(|x| *x == "--test").count());
    a.iter().enumerate().filter(|(_, x)| *x == "--test").filter_map(|(i, _)| a.get(i + 1).cloned()).collect()
}

fn count(file: &str, krate: &str) -> usize {
    selected(file, krate).len()
}

#[test]
fn post_change_selects_the_post_dialect_suites() {
    let got = selected("crates/post/src/post/mod.rs", "app");
    for must in ["heidenhain_cycles", "heidenhain_block_form", "conformance"] {
        assert!(got.iter().any(|b| b == must), "{must} missing from {got:?}");
    }
    // #926: fanuc_post moved to the post crate, whose own suites a post change selects.
    let own = selected("crates/post/src/post/mod.rs", "post");
    assert!(own.iter().any(|b| b == "fanuc_post"), "fanuc_post missing from {own:?}");
    assert!(count("crates/post/src/post/mod.rs", "app") >= 20, "post change selects too few app suites");
}

/// A file in a submodule narrows to the suites naming that submodule, and still picks some.
#[test]
fn post_submodule_change_narrows_but_still_selects() {
    let got = selected("crates/post/src/post/engine.rs", "app");
    assert!(got.iter().any(|b| b == "heidenhain_cycles"), "{got:?}");
    assert!(!got.is_empty());
}

#[test]
fn sim_verify_change_selects_the_verify_suites() {
    let got = selected("crates/sim/src/verify/mod.rs", "app");
    for must in ["verify", "warned_moves"] {
        assert!(got.iter().any(|b| b == must), "{must} missing from {got:?}");
    }
    assert!(count("crates/sim/src/verify/mod.rs", "app") >= 20, "verify change selects too few app suites");
}

#[test]
fn motion_machining2d_change_selects_the_machining2d_suites() {
    let got = selected("crates/motion/src/machining2d/mod.rs", "app");
    assert!(got.iter().any(|b| b == "golden_cl"), "golden_cl missing from {got:?}");
    assert!(count("crates/motion/src/machining2d/mod.rs", "app") >= 20, "machining2d selects too few");
}

/// No app suite may name a module that lives in another crate as `app::<module>`.
#[test]
fn no_gate_map_names_a_moved_module_as_app() {
    let moved = [
        "post", "verify", "sim", "solid_rest", "cl", "geo2d", "stock", "tools", "orientation", "spun",
        "cutter_profile", "holder_library", "machining2d", "machining3d", "drilling", "finishing3d",
        "multiaxis", "turning", "links", "load", "plasma", "probing",
    ];
    let mut stale = Vec::new();
    let mut unmapped = Vec::new();
    for b in scan::binaries(&root(), "app") {
        let file = root().join("crates/app/tests").join(format!("{b}.rs"));
        let gm = scan::gate_map(&scan::read(&file)).unwrap_or_default();
        if gm.is_empty() {
            unmapped.push(b.clone());
        }
        for m in moved {
            let hit = gm.split(|c: char| !(c.is_alphanumeric() || c == '_' || c == ':')).any(|w| {
                w == format!("app::{m}") || w == format!("camcraft_app::{m}") || w.starts_with(&format!("app::{m}::"))
            });
            if hit {
                stale.push(format!("{b}: app::{m}"));
            }
        }
    }
    assert!(stale.is_empty(), "Gate maps naming moved modules as app::<module>: {stale:?}");
    assert!(unmapped.is_empty(), "suites without a Gate map: {unmapped:?}");
}
