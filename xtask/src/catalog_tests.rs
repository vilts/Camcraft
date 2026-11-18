//! Unit tests for `cargo xtask catalog`, branch-safety guards, PDF command
//! construction, and `quick.rs` catalog file mapping (#2393).
//!
//! Provenance: our own unit tests for xtask catalog tooling.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

use crate::catalog::{self, CatalogCase};
use crate::catalog_media;
use crate::catalog_pdf;
use crate::quick::{self, Diff, Quick};

struct TempDir(PathBuf);

impl TempDir {
    fn new(tag: &str) -> Self {
        static N: AtomicUsize = AtomicUsize::new(0);
        let dir = std::env::temp_dir().join(format!(
            "xtask-catalog-unit-{tag}-{}-{}",
            std::process::id(),
            N.fetch_add(1, Ordering::SeqCst)
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("create temp dir");
        Self(dir)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn write_file(path: &Path, text: &str) {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).expect("create parent");
    }
    fs::write(path, text).expect("write file");
}

#[test]
fn parse_flags_and_rejects_contradictions() {
    let ok = catalog::parse(&[
        "--case".into(),
        "2d/first-part".into(),
        "--check".into(),
        "--pdf".into(),
    ])
    .expect("valid flags");
    assert_eq!(ok.case.as_deref(), Some("2d/first-part"));
    assert!(ok.check);
    assert!(ok.media.pdf);

    let err = catalog::parse(&["--check".into(), "--rebuild".into()])
        .expect_err("contradictory flags must refuse");
    assert!(err.contains("--check"), "got: {err}");

    let err = catalog::parse(&["--with-guide".into()])
        .expect_err("--with-guide without --screenshots must refuse");
    assert!(err.contains("--screenshots"), "got: {err}");
}

#[test]
fn discovers_seed_case_and_rejects_missing_layout_files() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("workspace root");
    let cases = catalog::select_cases(root, Some("2d/first-part")).expect("seed case exists");
    assert_eq!(cases.len(), 1);
    catalog::validate_layout(&cases[0], false).expect("seed layout pre-outputs valid");

    let err = catalog::select_cases(root, Some("2d/no-such-case"))
        .expect_err("unknown case must refuse");
    assert!(err.contains("no-such-case"), "got: {err}");

    let tmp = TempDir::new("layout");
    let case_dir = tmp.0.join("examples/catalog/2d/sample");
    write_file(&case_dir.join("build.camscript"), "cam.log('ok');");
    write_file(&case_dir.join("README.md"), "# Sample\n");
    let case = CatalogCase {
        family: "2d".into(),
        name: "sample".into(),
        dir: case_dir,
    };
    let err = catalog::validate_layout(&case, false).expect_err("missing model/ must refuse");
    assert!(err.contains("model/"), "got: {err}");
}

#[test]
fn screenshots_branch_guards_and_orphan_commit_stay_off_main() {
    let tmp = TempDir::new("git");
    let run = |args: &[&str]| {
        let out = Command::new("git")
            .arg("-C")
            .arg(&tmp.0)
            .args(args)
            .output()
            .expect("git runs");
        assert!(
            out.status.success(),
            "git {args:?} failed: {}",
            String::from_utf8_lossy(&out.stderr)
        );
    };
    run(&["init", "-b", "main"]);
    run(&["config", "user.name", "Tester"]);
    run(&["config", "user.email", "tester@example.invalid"]);
    write_file(&tmp.0.join("Cargo.toml"), "[workspace]\n");
    run(&["add", "Cargo.toml"]);
    run(&["commit", "-m", "init"]);
    run(&["branch", "code-branch"]);

    for bad in ["main", "master", "HEAD", "code-branch", "../bad"] {
        let err = catalog_media::validate_screenshots_branch(&tmp.0, bad)
            .expect_err("must refuse unsafe branch");
        assert!(!err.is_empty(), "expected message for {bad}");
    }

    let err = catalog_media::ensure_outside_tree(&tmp.0, &tmp.0.join("docs/imgs"), "--screenshots-out")
        .expect_err("path inside repo must refuse");
    assert!(err.contains("outside"), "got: {err}");

    let png_src = TempDir::new("png");
    let png = png_src.0.join("first-part.png");
    fs::write(&png, b"\x89PNG\r\n\x1a\nfake").expect("write png");
    let files = vec![("catalog/2d/first-part/first-part.png".to_owned(), png)];
    let oid = catalog_media::commit_images_to_branch(&tmp.0, "catalog-screenshots", &files)
        .expect("commit to orphan screenshots branch");
    assert!(!oid.is_empty());

    let listed = Command::new("git")
        .arg("-C")
        .arg(&tmp.0)
        .args(["ls-tree", "-r", "--name-only", "catalog-screenshots"])
        .output()
        .expect("ls-tree");
    let text = String::from_utf8_lossy(&listed.stdout);
    assert_eq!(text.trim(), "catalog/2d/first-part/first-part.png");

    let status = Command::new("git")
        .arg("-C")
        .arg(&tmp.0)
        .args(["status", "--porcelain"])
        .output()
        .expect("status");
    assert!(status.stdout.is_empty(), "working tree stayed clean");
}

#[test]
fn pdf_command_builders_and_quick_gate_catalog_mapping() {
    let p_args = catalog_pdf::pdf_pandoc_args(
        Path::new("README.md"),
        Path::new("out.html"),
        "first-part",
    );
    assert!(p_args.contains(&"--standalone".to_owned()));
    assert!(p_args.contains(&"title=first-part".to_owned()));

    let c_args = catalog_pdf::pdf_chrome_args(Path::new("out.html"), Path::new("out.pdf"));
    assert!(c_args.contains(&"--headless".to_owned()));
    assert!(c_args.iter().any(|a| a.starts_with("--print-to-pdf=")));

    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("workspace root");
    let changed = vec!["examples/catalog/2d/first-part/first-part.nc".to_owned()];
    let hunks = |_: &str| Vec::new();
    let diff = Diff {
        changed: &changed,
        untracked: &[],
        hunks: &hunks,
        before: &|_| None,
    };
    let Ok(Quick::Plan(plan)) = quick::plan(root, &diff, false) else {
        panic!("catalog diff should produce quick plan");
    };
    assert!(
        plan.steps
            .iter()
            .any(|s| s.name == "app suites: example_catalog"),
        "quick gate must select example_catalog when examples/catalog/ changes: {:?}",
        plan.steps
    );
}
