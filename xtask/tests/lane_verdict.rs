//! Provenance: PR #2942 review; exercise lane completeness through the public gate CLI.
#![cfg(unix)]

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::process::{Command, Output};
use std::sync::atomic::{AtomicUsize, Ordering};

static NEXT: AtomicUsize = AtomicUsize::new(0);

struct Fixture(PathBuf);

impl Fixture {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!(
            "camcraft-lane-verdict-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(root.join("crates/ui")).unwrap();
        fs::create_dir_all(root.join("xtask")).unwrap();
        fs::write(root.join(".gitignore"), "gate-logs/\n").unwrap();
        let cargo = root.join("cargo");
        // The gate runs one selected step. Only its cargo child is replaced; planning,
        // selection, record writing and verdict collection use the real xtask CLI.
        fs::write(
            &cargo,
            "#!/bin/sh\nprintf 'test result: ok. 1 passed; 0 failed; 0 ignored\\n'\n",
        )
        .unwrap();
        fs::set_permissions(&cargo, fs::Permissions::from_mode(0o755)).unwrap();
        for args in [
            vec!["init", "--quiet"],
            vec!["add", "."],
            vec![
                "-c",
                "user.name=Test",
                "-c",
                "user.email=test@example.com",
                "commit",
                "--quiet",
                "-m",
                "fixture",
            ],
        ] {
            let output = Command::new("git")
                .args(args)
                .current_dir(&root)
                .output()
                .unwrap();
            assert!(output.status.success(), "{output:?}");
        }
        Self(root)
    }

    fn run(&self, args: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_xtask"))
            .args(args)
            .env("CARGO_MANIFEST_DIR", self.0.join("xtask"))
            .env("CARGO", self.0.join("cargo"))
            .env("CAMCRAFT_NO_SCCACHE", "1")
            .env("CAMCRAFT_GATE_JOBS", "2")
            .output()
            .unwrap()
    }

    fn check_partial(&self, selection: &[&str]) {
        let mut args = vec!["gate", "batch", "--lane", "1/1"];
        args.extend_from_slice(selection);
        let gate = self.run(&args);
        assert!(gate.status.success(), "{gate:?}");
        let logs = self.0.join("gate-logs");
        let verdict = self.run(&["gate-verdict", logs.to_str().unwrap()]);
        assert_eq!(verdict.status.code(), Some(1), "{verdict:?}");
        let text = String::from_utf8_lossy(&verdict.stdout);
        assert!(text.contains("Batch gate verdict: RED"), "{text}");
        assert!(text.contains("the other steps did not run"), "{text}");
        let run = fs::read_dir(logs).unwrap().next().unwrap().unwrap().path();
        let record = fs::read_to_string(run.join("lane-result.tsv")).unwrap();
        let planned: usize = record
            .lines()
            .find_map(|l| l.strip_prefix("planned\t"))
            .unwrap()
            .parse()
            .unwrap();
        assert!(planned > 1, "{record}");
        assert_eq!(
            record.lines().filter(|l| l.starts_with("step\t")).count(),
            1,
            "{record}"
        );
        assert!(record.contains("result\tred\n"), "{record}");
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn only_selected_lane_steps_cannot_give_a_green_verdict() {
    Fixture::new().check_partial(&["--only", "cl suites: lib"]);
}

#[test]
fn failed_selected_lane_steps_cannot_give_a_green_verdict() {
    let fixture = Fixture::new();
    let failed = fixture.0.join("previous");
    fs::create_dir(&failed).unwrap();
    fs::write(
        failed.join("01-cl_suites_lib.log"),
        "test result: FAILED.\n",
    )
    .unwrap();
    fixture.check_partial(&["--failed", failed.to_str().unwrap()]);
}

#[test]
fn an_empty_lane_record_cannot_give_a_green_verdict() {
    let fixture = Fixture::new();
    let records = fixture.0.join("records");
    fs::create_dir(&records).unwrap();
    fs::write(
        records.join("lane-result.tsv"),
        "lane\t1/1\nrunner\ttest\nresult\tgreen\nwall\t0\nplanned\t0\n",
    )
    .unwrap();
    let verdict = fixture.run(&["gate-verdict", records.to_str().unwrap()]);
    assert_eq!(verdict.status.code(), Some(1), "{verdict:?}");
    let text = String::from_utf8_lossy(&verdict.stdout);
    assert!(text.contains("Batch gate verdict: RED"), "{text}");
    assert!(text.contains("empty lane is incomplete"), "{text}");
}
