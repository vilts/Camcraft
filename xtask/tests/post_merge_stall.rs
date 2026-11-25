// Provenance: issue #3303 - a runner stall (a STALL row of the xtask watchdog, or the
// workflow's step timeout) with no FAIL row is not a red main: the report script opens
// and comments on no issue. FAIL rows stay red. GitHub's public Actions documentation
// (`timeout-minutes` of a step, job outputs).
use std::{fs, path::Path};

fn workflow() -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../.github/workflows/post-merge.yml");
    fs::read_to_string(path).expect("read the post-merge workflow")
}

#[test]
fn the_batch_step_has_its_own_timeout_below_the_job_cap() {
    let wf = workflow();
    let step = wf.split_once("      - name: Batch gate tier\n").expect("the batch step").1;
    let step: String = step.lines().take_while(|l| !l.starts_with("      - ")).collect::<Vec<_>>().join("\n");
    assert!(step.contains("        id: gate\n"), "the summary step reads the outcome by id:\n{step}");
    assert!(step.contains("        timeout-minutes: 75\n"), "the step timeout:\n{step}");
    assert!(wf.contains("    timeout-minutes: 90\n"), "the job cap stays");
    assert!(wf.contains("GATE_OUTCOME: ${{ steps.gate.outcome }}"), "the summary step reads the outcome");
    assert!(wf.contains("RUNNER: ${{ needs.gate.outputs.runner }}"), "the report names the runner");
}

// The script runs on Linux only, so these runs are unix only.
#[cfg(unix)]
mod script {
    use std::{fs, os::unix::fs::PermissionsExt, path::PathBuf, process::Command};

    const FAKE_GH: &str = "#!/usr/bin/env bash\nprintf '%s\\n' \"$*\" >> \"$FAKE_DIR/log\"\nexit 0\n";

    /// Runs the report script (STATUS=failure) on a gate output; returns (stdout, gh calls).
    fn report(name: &str, output: &str) -> (String, String) {
        let dir: PathBuf = std::env::temp_dir().join(format!("camcraft-post-merge-stall-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("dir");
        let gh = dir.join("gh");
        fs::write(&gh, FAKE_GH).expect("fake gh");
        fs::set_permissions(&gh, fs::Permissions::from_mode(0o755)).expect("chmod");
        fs::write(dir.join("log"), "").expect("log");
        fs::write(dir.join("out.txt"), output).expect("output");
        let script = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../.github/scripts/post-merge-report.sh");
        let out = Command::new("bash")
            .arg(script)
            .current_dir(&dir)
            .env("GH", &gh)
            .env("FAKE_DIR", &dir)
            .env("REPO", "o/r")
            .env("SHA", "0123456789abcdef")
            .env("STATUS", "failure")
            .env("RUN_URL", "https://run")
            .env("RUNNER", "len-pm")
            .env("OUTPUT", dir.join("out.txt"))
            .output()
            .expect("run the script");
        assert!(out.status.success(), "exit 0: {}", String::from_utf8_lossy(&out.stderr));
        let log = fs::read_to_string(dir.join("log")).expect("gh log");
        let _ = fs::remove_dir_all(&dir);
        (String::from_utf8_lossy(&out.stdout).into_owned(), log)
    }

    const STALL: &str = "== gate summary (batch) - logs in gate-logs/x\n  ok       40s  debug build: post\n  STALL   901s  post suites: lib  stall: no output for 15min; process tree killed (alive: post (12))\n";

    #[test]
    fn a_stall_row_is_a_runner_stall_and_opens_no_issue() {
        let (out, log) = report("row", STALL);
        assert!(out.contains("runner stall: post suites: lib on len-pm"), "{out}");
        assert!(!log.contains("issue create") && !log.contains("issue comment"), "no issue write: {log}");
    }

    #[test]
    fn a_step_timeout_is_a_runner_stall_and_opens_no_issue() {
        let text = "[01] debug build: post\n[07] ui suites: lib\ngate step TIMEOUT after 45 min; last started step: [07] ui suites: lib\n";
        let (out, log) = report("timeout", text);
        assert!(out.contains("runner stall: [07] ui suites: lib on len-pm"), "{out}");
        assert!(!log.contains("issue create") && !log.contains("issue comment"), "no issue write: {log}");
    }

    #[test]
    fn a_fail_row_stays_a_red_main_even_with_a_stall() {
        let text = format!("{STALL}  FAIL     3s  post suites: red  failed: tests/red.rs [tests: red_fails]\n");
        let (out, log) = report("fail", &text);
        assert!(!out.contains("runner stall"), "{out}");
        assert!(log.contains("issue create"), "a FAIL row opens the red issue: {log}");
    }

    #[test]
    fn a_step_that_ended_red_before_a_timeout_stays_a_red_main() {
        // Step 5 failed, a later step hung, and the step timeout stopped the gate before its summary.
        let text = "[05] post suites: red
     FAIL in 3s failed: tests/red.rs [tests: red_fails]
[30] ui suites: lib
gate step TIMEOUT after 45 min; last started step: [30] ui suites: lib
";
        let (out, log) = report("progress", text);
        assert!(!out.contains("runner stall"), "{out}");
        assert!(log.contains("issue create") && log.contains("FAIL in 3s failed: tests/red.rs"), "the issue lists the FAIL line: {log}");
    }

    #[test]
    fn a_large_summary_with_fail_and_stall_rows_stays_a_red_main() {
        // Over the 64 KiB of a pipe: a grep -q in a pipe under pipefail would read this as no FAIL row.
        let mut text = String::from("== gate summary (batch) - logs in gate-logs/x
  FAIL     3s  post suites: red  failed: tests/red.rs
");
        (0..3000).for_each(|i| text.push_str(&format!("  ok       1s  step {i} with a long name to fill the pipe buffer
")));
        text.push_str("  STALL   901s  post suites: lib  stall
");
        let (out, log) = report("large", &text);
        assert!(!out.contains("runner stall"), "{out}");
        assert!(log.contains("issue create"), "a FAIL row opens the red issue");
    }

    #[test]
    fn a_red_run_with_no_summary_and_no_timeout_stays_red() {
        let (_, log) = report("nosummary", "error: could not compile `xtask`\n");
        assert!(log.contains("issue create"), "an xtask that did not build is a red main: {log}");
    }
}
