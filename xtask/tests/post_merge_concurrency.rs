// Provenance: issue #1922 and the repository's post-merge workflow contract.
use std::{fs, path::Path};

#[test]
fn post_merge_preserves_running_gate() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../.github/workflows/post-merge.yml");
    let workflow = fs::read_to_string(path).expect("read the post-merge workflow");
    let concurrency = workflow
        .split_once("\nconcurrency:\n")
        .expect("the workflow has top-level concurrency")
        .1
        .split("\njobs:\n")
        .next()
        .expect("read the concurrency block");
    assert!(concurrency.lines().any(|line| line == "  group: post-merge-${{ github.event_name == 'push' && 'main' || inputs.ref || 'main' }}"),
        "keep main pushes together and other refs in their own groups");
    assert!(
        concurrency
            .lines()
            .any(|line| line == "  cancel-in-progress: false"),
        "a later merge must not cancel the running post-merge gate"
    );
}
