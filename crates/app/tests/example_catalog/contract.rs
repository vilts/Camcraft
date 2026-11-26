// Provenance: test-runner contracts for CamCraft's original example catalog (#2857).
use std::process::Command;

#[test]
fn every_case_has_two_named_test_results() {
    let output = Command::new(std::env::current_exe().expect("test executable"))
        .args(["--list", "--format", "terse"])
        .output()
        .expect("list catalog tests");
    assert!(output.status.success());
    let listing = String::from_utf8_lossy(&output.stdout);
    let cases = super::discover_cases(&super::repo("examples/catalog"));
    assert!(
        cases
            .iter()
            .any(|case| case.family == "2d" && case.name == "first-part"),
        "seed case 2d/first-part must be present under examples/catalog/"
    );
    for case in cases {
        let module = format!("catalog_{}_{}", case.family, case.name).replace('-', "_");
        for action in ["regenerates", "rebuilds"] {
            let name = format!("{module}::{action}: test");
            assert!(
                listing.lines().any(|line| line == name),
                "missing case test {name}\n{listing}"
            );
        }
    }
}

#[test]
fn adaptive_pocket_full_case_checks_run_in_the_heavy_lane() {
    let output = Command::new(std::env::current_exe().expect("test executable"))
        .args(["--ignored", "--list", "--format", "terse"])
        .output()
        .expect("list heavy catalog tests");
    assert!(output.status.success());
    let listing = String::from_utf8_lossy(&output.stdout);
    for action in ["regenerates", "rebuilds"] {
        let name = format!("catalog_2d_adaptive_pocket::{action}: test");
        assert!(
            listing.lines().any(|line| line == name),
            "missing heavy case test {name}\n{listing}"
        );
    }
}

#[test]
fn two_broken_cases_report_both_names_and_times() {
    let scratch = std::env::temp_dir().join(format!("catalog-broken-cases-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&scratch);
    std::fs::create_dir_all(&scratch).expect("empty catalog root");
    for action in ["regenerates", "rebuilds"] {
        let names = [
            format!("catalog_2d_first_part::{action}"),
            format!("catalog_2d_contour_tabs::{action}"),
        ];
        // One test thread (#3043): the two cases run in order, so the panic
        // message of one case cannot fall into the timing line of the other.
        let output = Command::new(std::env::current_exe().expect("test executable"))
            .args([
                names[0].as_str(),
                names[1].as_str(),
                "--nocapture",
                "--test-threads=1",
            ])
            .env("CAMCRAFT_TEST_CATALOG_ROOT", &scratch)
            .output()
            .expect("run two broken cases");
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            !output.status.success(),
            "two broken cases must fail: {stdout}"
        );
        for name in names {
            assert!(
                stdout.contains(&format!("{name} ... FAILED")),
                "missing named failure {name}: {stdout}"
            );
        }
        for case in ["2d/first-part", "2d/contour-tabs"] {
            assert!(
                stderr.contains(&format!("catalog {case} {action} elapsed=")),
                "missing time for {case}: {stderr}"
            );
        }
    }
    let _ = std::fs::remove_dir_all(&scratch);
}
