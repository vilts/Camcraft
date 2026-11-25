// Provenance: issues #1922 and #3184 and the repository's post-merge workflow
// contract; GitHub's public Actions documentation (concurrency groups).
use std::{fs, path::Path};

fn workflow() -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../.github/workflows/post-merge.yml");
    fs::read_to_string(path).expect("read the post-merge workflow")
}

/// The lines of one job: indented past the job key, up to the next job.
fn job(workflow: &str, name: &str) -> Vec<String> {
    let key = format!("\n  {name}:\n");
    let body = workflow.split_once(&key).unwrap_or_else(|| panic!("the workflow has a {name} job")).1;
    body.lines()
        .take_while(|line| line.is_empty() || line.starts_with("    "))
        .map(str::to_owned)
        .collect()
}

#[test]
fn post_merge_runs_each_commit_on_its_own() {
    let workflow = workflow();
    let concurrency = workflow
        .split_once("\nconcurrency:\n")
        .expect("the workflow has top-level concurrency")
        .1
        .split("\njobs:\n")
        .next()
        .expect("read the concurrency block");
    // One group per commit (#3184): a second push never waits behind the
    // running gate, and a third push never cancels the waiting one. A red run
    // then names exactly its own commit.
    assert!(
        concurrency.lines().any(|line| line == "  group: post-merge-${{ inputs.ref || github.sha }}"),
        "each pushed commit gets its own concurrency group: {concurrency}"
    );
    assert!(
        concurrency.lines().any(|line| line == "  cancel-in-progress: false"),
        "a later merge must not cancel a running post-merge gate: {concurrency}"
    );
}

#[test]
fn post_merge_reports_one_at_a_time_with_full_history() {
    let report = job(&workflow(), "report");
    // Runs finish out of order now: the reports go one at a time, and the
    // report script needs the history to see if its commit is older (#3184).
    let at = report
        .iter()
        .position(|line| line == "    concurrency:")
        .expect("the report job has its own concurrency");
    assert_eq!(report.get(at + 1).map(String::as_str), Some("      group: post-merge-report"), "{report:#?}");
    assert_eq!(report.get(at + 2).map(String::as_str), Some("      cancel-in-progress: false"), "{report:#?}");
    assert!(
        report.iter().any(|line| line.trim() == "fetch-depth: 0"),
        "the report job checks out the full history: {report:#?}"
    );
    assert!(
        !report.iter().any(|line| line.trim() == "fetch-depth: 1"),
        "the report job does not check out one commit only: {report:#?}"
    );
}
