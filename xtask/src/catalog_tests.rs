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

fn links_fixture(tag: &str) -> TempDir {
    let tmp = TempDir::new(tag);
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("workspace root");
    let kinds = "crates/app/src/operation/kinds.rs";
    write_file(
        &tmp.0.join(kinds),
        &fs::read_to_string(root.join(kinds)).expect("kind registry"),
    );
    for page in [
        "operations/operations-2d.md",
        "operations/operations-3d.md",
        "operations/operations-turning.md",
        "operations/operations-edm.md",
        "operations/operations-plasma.md",
        "operations/operations-probing.md",
        "multi-axis/operations-multiaxis.md",
    ] {
        let path = format!("site/content/guide/{page}");
        let text = fs::read_to_string(root.join(&path)).expect("operation page");
        let headings = text
            .lines()
            .filter(|line| line.starts_with("## "))
            .collect::<Vec<_>>()
            .join("\n\n");
        write_file(&tmp.0.join(&path), &format!("{headings}\n"));
    }
    write_file(
        &tmp.0.join("examples/catalog/2d/sample/case.toml"),
        "kinds = [\"Face\"]\nmachine = \"3-axis mill\"\nexpect = \"post\"\n",
    );
    tmp
}

#[test]
fn catalog_check_rejects_missing_case_metadata() {
    let tmp = links_fixture("missing-metadata");
    fs::remove_file(tmp.0.join("examples/catalog/2d/sample/case.toml")).expect("remove metadata");
    let err = catalog::run(&tmp.0, &["--check".into()]).expect_err("missing metadata must refuse");
    assert!(
        err.contains("sample") && err.contains("case.toml"),
        "got: {err}"
    );
}

#[test]
fn catalog_check_rejects_unknown_case_kind() {
    let tmp = links_fixture("unknown-kind");
    write_file(
        &tmp.0.join("examples/catalog/2d/sample/case.toml"),
        "kinds = [\"Imaginary operation\"]\nmachine = \"3-axis mill\"\nexpect = \"post\"\n",
    );
    let err = catalog::run(&tmp.0, &["--check".into()]).expect_err("unknown kind must refuse");
    assert!(
        err.contains("sample") && err.contains("Imaginary operation"),
        "got: {err}"
    );
}

#[test]
fn catalog_check_rejects_missing_case_link() {
    let tmp = links_fixture("missing-link");
    let err = catalog::run(&tmp.0, &["--check".into()]).expect_err("unlinked case must refuse");
    assert!(err.contains("example links"), "got: {err}");
}

#[test]
fn catalog_check_rejects_dangling_case_link() {
    let tmp = links_fixture("dangling-link");
    let page = tmp.0.join("site/content/guide/operations/operations-2d.md");
    let text = fs::read_to_string(&page).expect("read page");
    write_file(&page, &text.replacen(
        "## Face\n",
        "## Face\n\n**Example cases**\n\n- [2d/no-such-case](https://github.com/NeoTech/Camcraft/tree/main/examples/catalog/2d/no-such-case)\n",
        1,
    ));
    let err = catalog::run(&tmp.0, &["--check".into()]).expect_err("dangling link must refuse");
    assert!(
        err.contains("no-such-case") && err.contains("unknown case"),
        "got: {err}"
    );
}

#[test]
fn catalog_links_generate_idempotently_and_check_without_touching_prose() {
    let tmp = links_fixture("roundtrip");
    let page = tmp.0.join("site/content/guide/operations/operations-2d.md");
    let prose = "A paragraph with **formatting** and a [guide link](@/guide/_index.md).";
    let source = fs::read_to_string(&page).expect("read page");
    write_file(
        &page,
        &source.replacen("## Face\n", &format!("## Face\n\n{prose}\n"), 1),
    );
    let cases = catalog::discover_cases(&tmp.0).expect("discover fixture");
    catalog::links::sync(&tmp.0, &cases, false).expect("generate links");
    let once: Vec<_> = catalog::links::PAGES
        .iter()
        .map(|p| fs::read(tmp.0.join("site/content/guide").join(p)).expect("generated page"))
        .collect();
    let text = fs::read_to_string(&page).expect("generated 2D page");
    assert!(text.contains(prose));
    assert_eq!(
        text.matches(
            "[2d/sample](https://github.com/NeoTech/Camcraft/tree/main/examples/catalog/2d/sample)"
        )
        .count(),
        1
    );
    catalog::links::sync(&tmp.0, &cases, false).expect("regenerate links");
    catalog::links::sync(&tmp.0, &cases, true).expect("generated links check");
    for (p, expected) in catalog::links::PAGES.iter().zip(once) {
        assert_eq!(
            fs::read(tmp.0.join("site/content/guide").join(p)).expect("page"),
            expected,
            "{p}"
        );
    }
}

#[test]
fn catalog_metadata_rejects_missing_duplicate_and_empty_fields() {
    let valid = "kinds = [\"Face\"]\nmachine = \"mill\"\nexpect = \"post\"\n";
    for key in ["kinds", "machine", "expect"] {
        let missing = valid
            .lines()
            .filter(|line| !line.starts_with(key))
            .collect::<Vec<_>>()
            .join("\n");
        let err = catalog::links::metadata::parse(&missing).expect_err("missing field");
        assert!(err.contains("missing") && err.contains(key), "got: {err}");
        let duplicate = valid
            .lines()
            .find(|line| line.starts_with(key))
            .expect("field");
        let err = catalog::links::metadata::parse(&format!("{valid}{duplicate}\n"))
            .expect_err("duplicate field");
        assert!(err.contains("duplicate") && err.contains(key), "got: {err}");
    }
    for value in ["[]", "[\"\"]", "[\"Face\", \"Face\"]"] {
        let text = valid.replace("[\"Face\"]", value);
        assert!(
            catalog::links::metadata::parse(&text).is_err(),
            "accepted kinds = {value}"
        );
    }
    for key in ["machine", "expect"] {
        let line = valid
            .lines()
            .find(|line| line.starts_with(key))
            .expect("field");
        assert!(
            catalog::links::metadata::parse(&valid.replace(line, &format!("{key} = \" \"")))
                .is_err()
        );
    }
}

#[test]
fn catalog_metadata_rejects_invalid_syntax_by_name() {
    for text in [
        "kinds = [\"Face\",,]\nmachine = \"mill\"\nexpect = \"post\"",
        "kinds = [\"Face\"\nmachine = \"mill\"\nexpect = \"post\"",
        "kinds = \"Face\"\nmachine = \"mill\"\nexpect = \"post\"",
        "kinds = [\"Face\"]\nmachine = mill\nexpect = \"post\"",
        "kinds [\"Face\"]\nmachine = \"mill\"\nexpect = \"post\"",
        "kinds = [\"Face\"]\nmachine = \"mill\"\nexpect = \"unterminated",
        "kinds = [\"Face\"]\nmachine = \"mill\"\nexpect = \"post\"\nextra = \"x\"",
        "kinds = [\"Face\"]\nmachine = \"mill\"expect = \"post\"",
    ] {
        let err = catalog::links::metadata::parse(text).expect_err("bad metadata must refuse");
        assert!(!err.is_empty(), "must explain rejected metadata: {text}");
    }
}

#[test]
fn catalog_metadata_accepts_multiline_arrays_comments_and_literal_strings() {
    let parsed = catalog::links::metadata::parse(
        "# case metadata\nkinds = [\n  \"Face\", # first\n  '2D Pocket',\n]\nmachine = \"mill's #1 axis\"\nexpect = 'post' # trailing comment #1\n# final comment\n",
    ).expect("supported flat TOML");
    assert_eq!(parsed["kinds"], ["Face", "2D Pocket"]);
    assert_eq!(parsed["machine"], ["mill's #1 axis"]);
    assert_eq!(parsed["expect"], ["post"]);
}

#[test]
fn catalog_seed_links_face_pocket_and_drill() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("workspace root");
    let cases = catalog::discover_cases(root).expect("catalog cases");
    catalog::links::sync(root, &cases, true).expect("committed metadata and links check");
    let text = fs::read_to_string(root.join("site/content/guide/operations/operations-2d.md"))
        .expect("2D page");
    for heading in ["Face", "2D Pocket", "Drill"] {
        let section = text
            .split(&format!("## {heading}\n"))
            .nth(1)
            .expect("kind heading")
            .split("\n## ")
            .next()
            .expect("kind section");
        assert!(section.contains("[2d/first-part](https://github.com/NeoTech/Camcraft/tree/main/examples/catalog/2d/first-part)"), "{heading} lacks seed link");
    }
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
