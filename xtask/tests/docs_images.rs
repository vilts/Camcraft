//! Pins `cargo xtask docs-images` CLI contract, validation, and refusal behavior (#1902).
//! Provenance: CamCraft repo process/docs image tooling tests.

use std::path::PathBuf;
use std::process::Command;

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("workspace root")
        .to_path_buf()
}

fn xtask_bin() -> &'static str {
    env!("CARGO_BIN_EXE_xtask")
}

#[test]
fn help_lists_docs_images_command() {
    let out = Command::new(xtask_bin())
        .arg("--help")
        .output()
        .expect("run xtask --help");
    assert!(out.status.success(), "--help should succeed: {out:?}");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("docs-images"),
        "USAGE must list `cargo xtask docs-images`, got:\n{stdout}"
    );
}

#[test]
fn docs_images_check_verifies_guide_pngs_and_manifest() {
    let out = Command::new(xtask_bin())
        .current_dir(workspace_root())
        .args(["docs-images", "--check"])
        .output()
        .expect("run xtask docs-images --check");
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        out.status.success(),
        "docs-images --check failed (status {:?})\nstdout:\n{stdout}\nstderr:\n{stderr}",
        out.status.code()
    );
    assert!(
        stdout.contains("docs-images: 13 guide PNGs ok"),
        "expected 13-PNG summary output from docs-images --check, got:\n{stdout}"
    );
}

#[test]
fn docs_images_refuses_unknown_option_by_name() {
    let out = Command::new(xtask_bin())
        .current_dir(workspace_root())
        .args(["docs-images", "--bogus-flag"])
        .output()
        .expect("run xtask docs-images --bogus-flag");
    assert!(!out.status.success(), "unknown flag must fail");
    let combined = format!(
        "{}\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        combined.contains("--bogus-flag"),
        "refusal must name the offending flag `--bogus-flag`, got:\n{combined}"
    );
}

#[test]
fn guide_markdown_relative_urls_and_demo_projects_are_valid() {
    let root = workspace_root();
    for demo in [
        "first-part.camproj",
        "guide-workholding.camproj",
        "guide-3d.camproj",
        "guide-multiaxis.camproj",
    ] {
        let path = root.join("examples").join(demo);
        let text = std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("missing demo {}: {e}", path.display()));
        assert!(text.contains("\"version\""), "{} lacks project JSON structure", path.display());
    }

    let pages = [
        ("site/content/guide/set-up/setups.md", vec!["setup-overview.png"]),
        ("site/content/guide/set-up/workholding.md", vec!["workholding-vise-toolpath.png"]),
        (
            "site/content/guide/operations/operations-2d.md",
            vec![
                "op-2d-pocket-params.png",
                "op-2d-stepover-before.png",
                "op-2d-stepover-after.png",
                "op-2d-entry-before.png",
                "op-2d-entry-after.png",
                "op-2d-lead-before.png",
                "op-2d-lead-after.png",
            ],
        ),
        (
            "site/content/guide/operations/operations-3d.md",
            vec!["op-3d-pocket-roughing.png", "op-3d-finish-parallel.png"],
        ),
        ("site/content/guide/multi-axis/operations-multiaxis.md", vec!["op-multiaxis-swarf.png"]),
        ("site/content/guide/multi-axis/indexed.md", vec!["op-indexed-3plus2.png"]),
    ];

    for (rel, files) in pages {
        let md = std::fs::read_to_string(root.join(rel))
            .unwrap_or_else(|e| panic!("read {rel}: {e}"));
        for file in files {
            let url = format!("../../../images/guide/{file})");
            assert!(md.contains(&url), "{rel} must use relative Zola path {url}");
        }
    }

    let css = std::fs::read_to_string(root.join("site/static/style.css")).expect("style.css");
    assert!(css.contains(".guide-content img"), "style.css must style guide screenshots");
    assert!(css.contains(".guide-pair"), "style.css must lay out before/after pairs");
}
