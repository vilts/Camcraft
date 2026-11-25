//! Provenance: our own unit tests for the catalog refusal-case contract (#2856).

use std::fs;
use std::path::{Path, PathBuf};

use super::super::links::metadata::{self, Expect};
use super::super::{validate_layout, CatalogCase};
use super::{check, lines, FILE};

struct Scratch(PathBuf);

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn write(path: &Path, text: &str) {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).expect("create parent");
    }
    fs::write(path, text).expect("write file");
}

/// A built case `2d/sample` with `expect`, its project and no program.
fn case(tag: &str, expect: &str) -> (Scratch, CatalogCase) {
    let dir = std::env::temp_dir()
        .join(format!("xtask-catalog-refusal-{tag}-{}", std::process::id()))
        .join("2d/sample");
    let _ = fs::remove_dir_all(&dir);
    write(&dir.join("model/sample.obj"), "v 0 0 0\n");
    write(&dir.join("build.camscript"), "cam.log('x');\n");
    write(&dir.join("README.md"), "# Sample\n");
    write(&dir.join("sample.camproj"), "{}\n");
    write(
        &dir.join("case.toml"),
        &format!("kinds = [\"Drill\"]\nmachine = \"3-axis mill\"\nexpect = \"{expect}\"\n"),
    );
    let root = dir.parent().and_then(Path::parent).expect("scratch root").to_path_buf();
    let case = CatalogCase { family: "2d".into(), name: "sample".into(), dir };
    (Scratch(root), case)
}

#[test]
fn catalog_expect_takes_post_or_refusal_only() {
    let text = |v: &str| format!("kinds = [\"Drill\"]\nmachine = \"mill\"\nexpect = \"{v}\"\n");
    assert!(metadata::parse(&text("post")).is_ok());
    assert!(metadata::parse(&text("refusal")).is_ok());
    let err = metadata::parse(&text("Drill the holes")).expect_err("free text must refuse");
    assert!(err.contains("expect") && err.contains("refusal"), "got: {err}");
    let (_tmp, c) = case("expect-of", "refusal");
    assert_eq!(metadata::expect_of(&c.dir), Ok(Expect::Refusal));
}

#[test]
fn catalog_layout_accepts_a_refusal_case_without_nc() {
    let (_tmp, c) = case("layout", "refusal");
    let err = validate_layout(&c, true).expect_err("a refusal case needs refusal.txt");
    assert!(err.contains(FILE), "got: {err}");
    write(&c.dir.join(FILE), "post: refused\n");
    validate_layout(&c, true).expect("a refusal case has no .nc");
    write(&c.dir.join("sample.nc"), "%\n");
    let err = validate_layout(&c, true).expect_err("a refusal case that posts must refuse");
    assert!(err.contains("sample.nc"), "got: {err}");
}

#[test]
fn catalog_layout_still_requires_nc_for_a_post_case() {
    let (_tmp, c) = case("post", "post");
    write(&c.dir.join(FILE), "post: refused\n");
    let err = validate_layout(&c, true).expect_err("a post case needs its .nc");
    assert!(err.contains("sample.nc"), "got: {err}");
}

#[test]
fn refusal_lines_keep_the_cli_error_lines_without_the_prefix() {
    let stderr = "2026-11-21T12:01:07Z WARN posting: not written\ncamcraft-cli: post: refused\ncamcraft-cli: save: refused\n";
    assert_eq!(lines(stderr), "post: refused\nsave: refused\n");
}

#[test]
fn a_changed_refusal_text_fails_the_check() {
    let (_tmp, c) = case("changed", "refusal");
    write(&c.dir.join(FILE), "post: refused at 115\n");
    check(&c, "post: refused at 115\n").expect("same lines pass");
    let err = check(&c, "post: refused at 120\n").expect_err("changed lines must fail");
    assert!(err.contains("refusal lines differ"), "got: {err}");
}
