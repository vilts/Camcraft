//! Pins the quick tier's asset selection (#923): a diff of only a post script or machine file runs the
//! suites that post through it, and an asset no suite covers is reported, not skipped.
//! Provenance: our own build law (AGENTS.md "Test the change, not everything"), #919 research.

use std::path::{Path, PathBuf};

use crate::quick::{self, Diff, Quick};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).parent().map(Path::to_path_buf).unwrap_or_default()
}

fn plan_for(root: &Path, files: &[&str]) -> crate::plan::Plan {
    let changed: Vec<String> = files.iter().map(|s| s.to_string()).collect();
    let hunks = |_: &str| -> Vec<String> { Vec::new() };
    let diff = Diff { changed: &changed, untracked: &[], hunks: &hunks };
    match quick::plan(root, &diff, false) {
        Ok(Quick::Plan(p)) => p,
        other => panic!("{files:?} did not plan: {other:?}"),
    }
}

/// The suites the plan runs, in every crate (`<crate> suites` steps): the
/// post-script suites live in app and, since #926, in post too.
fn app_suites(p: &crate::plan::Plan) -> Vec<String> {
    p.steps
        .iter()
        .filter(|s| s.name.ends_with(" suites"))
        .flat_map(|s| {
            s.args.iter().enumerate().filter(|(_, x)| *x == "--test").filter_map(|(i, _)| s.args.get(i + 1).cloned())
        })
        .collect()
}

#[test]
fn cps_only_change_selects_the_suites_posting_through_it() {
    let p = plan_for(&root(), &["assets/posts/fanuc.cps"]);
    let got = app_suites(&p);
    for must in ["fanuc_post", "conformance"] {
        assert!(got.iter().any(|b| b == must), "{must} missing from {got:?}");
    }
    assert!(!p.notes.iter().any(|n| n.contains("no tests to run")), "{:?}", p.notes);
}

#[test]
fn a_cps_change_skips_suites_that_never_name_it() {
    let got = app_suites(&plan_for(&root(), &["assets/posts/fanuc.cps"]));
    assert!(!got.iter().any(|b| b == "heidenhain_block_form"), "{got:?}");
    let got = app_suites(&plan_for(&root(), &["assets/posts/heidenhain.cps"]));
    assert!(got.iter().any(|b| b == "heidenhain_cycles"), "{got:?}");
}

#[test]
fn mch_only_change_selects_the_suites_using_that_machine() {
    let got = app_suites(&plan_for(&root(), &["assets/machines/3axis-vmc.mch"]));
    for must in ["arc_planes", "comment_escape", "conformance"] {
        assert!(got.iter().any(|b| b == must), "{must} missing from {got:?}");
    }
}

#[test]
fn an_asset_no_suite_covers_is_reported_not_skipped() {
    let dir = std::env::temp_dir().join(format!("xtask-asset-{}", std::process::id()));
    let posts = dir.join("assets/posts");
    assert!(std::fs::create_dir_all(&posts).is_ok());
    assert!(std::fs::write(posts.join("orphan-lathe.cps"), "// x").is_ok());
    let p = plan_for(&dir, &["assets/posts/orphan-lathe.cps"]);
    let _ = std::fs::remove_dir_all(&dir);
    assert!(
        p.notes.iter().any(|n| n.contains("assets/posts/orphan-lathe.cps") && n.contains("NO SUITE")),
        "{:?}",
        p.notes
    );
    assert!(!p.notes.iter().any(|n| n.contains("no tests to run")), "{:?}", p.notes);
}

#[test]
fn only_post_scripts_and_machine_files_count_as_assets() {
    let a = crate::assets::changed_assets(&[
        "assets/posts/AGENTS.md".into(),
        "assets/posts/fanuc.cps".into(),
        "assets/machines/vmc.mch".into(),
        "assets/posts/sub/x.cps".into(),
    ]);
    let names: Vec<_> = a.iter().map(|a| a.path.as_str()).collect();
    assert_eq!(names, ["assets/posts/fanuc.cps", "assets/machines/vmc.mch"]);
}
