//! Provenance: issue #1794; PR review evidence belongs outside the product tree.

use std::path::Path;

#[test]
fn reviewed_screenshot_directories_are_absent() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    let present: Vec<_> = [
        ("docs", "pr-1711"),
        ("docs", "pr-1771"),
        ("docs/review", "1765"),
        ("docs/reviews", "1756"),
        ("docs/review", "1744"),
        ("docs/reviews", "1699"),
        ("docs/reviews", "1710"),
        ("docs/reviews", "1724"),
    ]
    .into_iter()
    .map(|(directory, number)| format!("{directory}/{number}"))
    .filter(|path| root.join(path).exists())
    .collect();

    assert!(
        present.is_empty(),
        "review screenshot directories remain in the product tree: {}",
        present.join(", ")
    );
}

#[test]
fn boundary_test_has_no_review_capture_hook() {
    let source = include_str!("../../crates/ui/src/egui_app/tests/edge_click_modifiers.rs");
    assert!(
        !source.contains("BOUNDARY_REVIEW_IMAGES"),
        "the boundary test must not recreate review directories from an environment variable"
    );
}

#[test]
fn manual_review_capture_uses_scratch_directory() {
    let source = include_str!("../../crates/ui/src/egui_app/tests/review_1710_evidence.rs");
    assert!(
        source.contains("std::env::temp_dir()") && !source.contains("CARGO_MANIFEST_DIR"),
        "manual review capture must use a scratch directory outside the product tree"
    );
}
