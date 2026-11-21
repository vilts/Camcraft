// Provenance: issue #1611 and the repository's post-merge workflow contract;
// GitHub's public Actions documentation (status check functions in `if`).
use std::{fs, path::Path};

/// The `if:` line of the `report` job.
fn report_if() -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../.github/workflows/post-merge.yml");
    let workflow = fs::read_to_string(path).expect("read the post-merge workflow");
    let report = workflow.split_once("\n  report:\n").expect("the workflow has a report job").1;
    // The job's own lines: indented past the job key, up to the next job.
    report
        .lines()
        .take_while(|line| line.is_empty() || line.starts_with("    "))
        .find_map(|line| line.trim_start().strip_prefix("if: "))
        .expect("the report job has an if")
        .to_owned()
}

#[test]
fn a_failed_gate_still_runs_the_report_job() {
    let cond = report_if();
    // Without a status function GitHub adds an implicit success(): a failed
    // `needs: gate` then skips the job and a red main opens no issue (#1611).
    assert!(cond.starts_with("${{ always() && "), "the report job's if starts with always(): {cond}");
    // always() alone would also report a cancelled or a cut-skipped run.
    assert!(
        cond.contains("(needs.gate.result == 'success' || needs.gate.result == 'failure')"),
        "only a green or a red gate reports: {cond}"
    );
    assert!(cond.contains("needs.gate.outputs.skipped != 'true'"), "a run that yields to a cut reports nothing: {cond}");
}
