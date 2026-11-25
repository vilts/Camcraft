//! Provenance: our own generator refusal tests for malformed guide blocks (#2855).

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::PathBuf;

use super::{END, PAGES, START, render, sync};
use crate::catalog::CatalogCase;

fn refuses(text: &str, message: &str) {
    let err = render(
        "operations/operations-2d.md",
        text,
        &BTreeSet::from(["Face".to_string()]),
        &BTreeMap::new(),
        &mut BTreeSet::new(),
    )
    .expect_err("malformed block must refuse");
    assert!(err.contains(message), "got: {err}");
}

#[test]
fn misplaced_example_markers_refuse() {
    for marker in [START, END] {
        refuses(&format!("# Guide\n{marker}\n"), "misplaced");
    }
}

#[test]
fn unclosed_example_blocks_refuse() {
    refuses(&format!("## Face\n\n{START}\nA link\n"), "unclosed");
}

#[test]
fn nested_markers_and_headings_inside_example_blocks_refuse() {
    for line in [START, "## Drill"] {
        refuses(&format!("## Face\n\n{START}\n{line}\n{END}\n"), "unclosed");
    }
}

struct TempRoot(PathBuf);
impl Drop for TempRoot {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn a_case_kind_without_a_guide_heading_refuses_before_writing() {
    let tmp =
        TempRoot(std::env::temp_dir().join(format!("catalog-no-heading-{}", std::process::id())));
    let case_dir = tmp.0.join("examples/catalog/2d/sample");
    fs::create_dir_all(&case_dir).expect("case dir");
    fs::write(
        case_dir.join("case.toml"),
        "kinds = [\"Face\"]\nmachine = \"mill\"\nexpect = \"post\"\n",
    )
    .expect("metadata");
    let registry = tmp.0.join("crates/app/src/operation");
    fs::create_dir_all(&registry).expect("registry dir");
    fs::write(
        registry.join("kinds.rs"),
        "pub const ALL: &[KindInfo] = &[\n label: \"Face\",\n    ];\n",
    )
    .expect("registry");
    let guide = tmp.0.join("site/content/guide");
    for page in PAGES {
        let path = guide.join(page);
        fs::create_dir_all(path.parent().expect("page parent")).expect("guide dir");
        fs::write(path, "# Guide\n").expect("guide page");
    }
    let case = CatalogCase {
        family: "2d".into(),
        name: "sample".into(),
        dir: case_dir,
    };
    let err = sync(&tmp.0, &[case], false).expect_err("uncovered kind must refuse");
    assert!(
        err.contains("Face") && err.contains("no operation guide heading"),
        "got: {err}"
    );
    for page in PAGES {
        assert_eq!(
            fs::read_to_string(guide.join(page)).expect("page"),
            "# Guide\n"
        );
    }
}

#[test]
fn committed_slot_undercut_has_its_generated_catalog_block() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("workspace root");
    let cases = crate::catalog::discover_cases(root).expect("catalog cases");
    sync(root, &cases, true).expect("committed metadata and links check");
    let text = fs::read_to_string(root.join("site/content/guide/operations/operations-2d.md"))
        .expect("2D page");
    let undercut = text
        .split("## Slot Undercut\n")
        .nth(1)
        .expect("Slot Undercut heading")
        .split("\n## ")
        .next()
        .expect("Slot Undercut section");
    assert!(
        undercut.contains(&format!(
            "{START}\n**Example cases**\n\nNo catalog case yet.\n{END}"
        )),
        "Slot Undercut lacks its generated catalog block"
    );
}
