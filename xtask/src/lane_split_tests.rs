//! Pins `gate batch --lane N/M` (#2908) on the batch plan of this repository: no step lost, no step
//! twice, the builds a lane needs, the same split on each call, and the refusals by name.
//! Provenance: issue #2908; our own gate plan.

use std::path::{Path, PathBuf};

use crate::lane_split::{self, Lane, MAX_LANES};
use crate::plan::{self, Plan, Step};
use crate::{lanes, parse_gate};

fn repo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).parent().expect("xtask has a parent").to_path_buf()
}

fn lane(plan: &Plan, index: usize, of: usize) -> Vec<Step> {
    lane_split::apply(plan.clone(), Lane { index, of }).expect("a lane of the batch plan has steps").steps
}

fn names(steps: &[Step]) -> Vec<String> {
    steps.iter().map(|s| s.name.clone()).collect()
}

fn tests_of(steps: &[Step]) -> Vec<Step> {
    steps.iter().filter(|s| !lanes::is_build(s)).cloned().collect()
}

#[test]
fn each_batch_step_runs_in_exactly_one_lane() {
    let batch = plan::batch(&repo());
    let mut want = names(&tests_of(&batch.steps));
    want.sort();
    assert!(want.len() > 100, "the batch plan of the repository is read: {} step(s)", want.len());
    for of in 1..=6 {
        let mut got = Vec::new();
        for index in 1..=of {
            got.extend(names(&tests_of(&lane(&batch, index, of))));
        }
        got.sort();
        // Equal sorted lists: nothing lost, and nothing in two lanes (a double would make `got` longer).
        assert_eq!(got, want, "the {of} lanes together run the batch plan, each step one time");
    }
}

#[test]
fn one_lane_of_one_is_the_whole_batch_plan() {
    let batch = plan::batch(&repo());
    assert_eq!(lane(&batch, 1, 1), batch.steps, "--lane 1/1 runs what `gate batch` runs today, in plan order");
}

#[test]
fn a_lane_builds_each_crate_that_it_tests_before_the_tests() {
    let batch = plan::batch(&repo());
    for index in 1..=3 {
        let steps = lane(&batch, index, 3);
        let first_test = steps.iter().position(|s| !lanes::is_build(s)).expect("a lane has a test step");
        assert!(steps[first_test..].iter().all(|s| !lanes::is_build(s)), "lane {index}: the builds come first");
        let built: Vec<&String> = steps[..first_test].iter().flat_map(|s| s.args.windows(2).filter(|w| w[0] == "-p").map(|w| &w[1])).collect();
        for s in steps[first_test..].iter().filter(|s| s.args[0] == "test") {
            let pkg = s.args.windows(2).find(|w| w[0] == "-p").map(|w| &w[1]).expect("a suite step names its crate");
            assert!(built.contains(&pkg), "lane {index}: '{}' has the build of {pkg}", s.name);
        }
    }
}

#[test]
fn the_lanes_share_the_queued_and_the_alone_steps_evenly() {
    let batch = plan::batch(&repo());
    for alone in [false, true] {
        let counts: Vec<usize> =
            (1..=3).map(|i| tests_of(&lane(&batch, i, 3)).iter().filter(|s| lanes::alone(s) == alone).count()).collect();
        let (min, max) = (counts.iter().min().copied().unwrap_or(0), counts.iter().max().copied().unwrap_or(0));
        assert!(min > 0 && max - min <= 1, "alone={alone}: the three lanes differ by one step at most: {counts:?}");
    }
}

#[test]
fn the_split_is_the_same_on_each_call_and_reads_no_timing_file() {
    let batch = plan::batch(&repo());
    let a = lane_split::assign(&batch.steps, 3);
    assert_eq!(a, lane_split::assign(&batch.steps, 3));
    // The order of the plan decides, not a history: the first queued step is in lane 1, the next in lane 2.
    let queued: Vec<usize> = batch
        .steps
        .iter()
        .enumerate()
        .filter(|(_, s)| !lanes::is_build(s) && !lanes::alone(s))
        .map(|(i, _)| i)
        .collect();
    assert_eq!((a[queued[0]], a[queued[1]], a[queued[2]], a[queued[3]]), (Some(1), Some(2), Some(3), Some(1)));
    assert!(batch.steps.iter().enumerate().all(|(i, s)| lanes::is_build(s) == a[i].is_none()), "only a build has no lane");
}

#[test]
fn a_bad_lane_is_refused_by_name() {
    let a = |s: &str| s.split_whitespace().map(String::from).collect::<Vec<_>>();
    assert_eq!(parse_gate(&a("batch --lane 2/3")).expect("batch --lane").lane, Some(Lane { index: 2, of: 3 }));
    assert_eq!(parse_gate(&a("batch")).expect("batch").lane, None);
    for bad in ["0/3", "4/3", "3", "a/b", "1/0", "-1/2", "1/17"] {
        let e = lane_split::parse(bad).expect_err(bad);
        assert!(e.contains("--lane needs N/M") && e.contains(&format!("'{bad}'")), "{e}");
    }
    assert!(lane_split::parse(&format!("{MAX_LANES}/{MAX_LANES}")).is_ok());
    let e = parse_gate(&a("release --lane 1/2")).expect_err("release has no lanes");
    assert!(e.contains("--lane splits the batch tier only, not 'release'"), "{e}");
    assert!(parse_gate(&a("batch --lane")).expect_err("no value").contains("--lane needs a value"));
}

#[test]
fn a_lane_with_no_step_is_an_error_not_a_green_run() {
    let one = Plan { notes: vec![], steps: vec![Step::new("clippy -D warnings", &["clippy", "-p", "camcraft-app"], "")] };
    assert_eq!(names(&lane(&one, 1, 2)), ["clippy -D warnings"]);
    let e = lane_split::apply(one, Lane { index: 2, of: 2 }).expect_err("lane 2 of a one-step plan is empty");
    assert!(e.contains("lane 2/2 has no step"), "{e}");
}
