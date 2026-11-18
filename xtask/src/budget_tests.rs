//! Provenance: the job budget law of issue #2057 (one budget shared by the lanes of the release queue).

use crate::budget::{DEFAULT, from_value, lane_limit, per_lane, scale};
use crate::plan::Step;

fn job_value(args: &[String]) -> Option<usize> {
    let i = args.iter().position(|a| a == "-j")?;
    args.get(i + 1)?.parse().ok()
}

fn threads(args: &[String]) -> Option<usize> {
    args.iter().find_map(|a| a.strip_prefix("--test-threads=")?.parse().ok())
}

fn suites(n: usize) -> Vec<Step> {
    let mut steps = vec![Step::new("debug build", &["test", "-j", "6", "--no-run"], "")];
    for i in 0..n {
        let profile: &[&str] = if i % 2 == 0 { &[] } else { &["--release"] };
        let mut args = vec!["test"];
        args.extend_from_slice(profile);
        args.extend(["-j", "6", "-p", "camcraft-motion", "--", "--test-threads=6"]);
        steps.push(Step::new(format!("suite {i}"), &args, ""));
    }
    steps.push(Step::new("app", &["test", "-j", "6", "-p", "camcraft-app", "--", "--test-threads=6"], ""));
    steps
}

/// The lanes and per-lane jobs `queue::run_schedule` uses for a queue of `n` steps.
fn queue_split(budget: usize, n: usize) -> (usize, usize) {
    let lanes = lane_limit(budget).min(n).max(1);
    (lanes, per_lane(budget, lanes))
}

#[test]
fn queue_jobs_sum_to_at_most_the_budget_and_lanes_to_at_most_half() {
    for budget in 1..=16 {
        let steps = suites(11);
        let s = crate::lanes::schedule(&steps, &std::collections::HashMap::new());
        let mut seen: Vec<_> = s.builds.iter().chain(&s.queue).chain(&s.alone).copied().collect();
        seen.sort_unstable();
        assert_eq!(seen, (0..steps.len()).collect::<Vec<_>>(), "budget {budget}: a step was lost");
        let (lanes, jobs) = queue_split(budget, s.queue.len());
        assert!(lanes <= (budget / 2).max(1), "budget {budget}: {lanes} lanes");
        for &i in &s.queue {
            let args = scale(&steps[i].args, jobs);
            let (j, t) = (job_value(&args).unwrap_or(0), threads(&args).unwrap_or(0));
            assert!(j * lanes <= budget.max(1), "budget {budget}: -j sum {}", j * lanes);
            assert!(t * lanes <= budget.max(1), "budget {budget}: {} test threads", t * lanes);
        }
    }
}

#[test]
fn budget_six_gives_three_lanes_of_two_jobs() {
    assert_eq!(DEFAULT, 6);
    assert_eq!(crate::plan::J, DEFAULT.to_string(), "the plan writes the default budget");
    assert_eq!(lane_limit(6), 3);
    assert_eq!(per_lane(6, 3), 2);
    assert_eq!(queue_split(6, 6), (3, 2));
    assert_eq!(queue_split(16, 100), (8, 2));
    assert_eq!(queue_split(16, 2), (2, 8), "a short queue gives each lane more jobs");
}

#[test]
fn a_step_alone_keeps_the_full_budget() {
    for budget in [1, 4, 6, 12, 16] {
        let steps = suites(0);
        let s = crate::lanes::schedule(&steps, &std::collections::HashMap::new());
        assert!(s.queue.is_empty(), "{s:?}");
        for &i in s.builds.iter().chain(&s.alone) {
            let args = scale(&steps[i].args, budget);
            assert_eq!(job_value(&args), Some(budget), "{}", steps[i].name);
        }
        // The whole-crate app step runs with --test-threads equal to the budget, not the plan's 6.
        assert_eq!(threads(&scale(&steps[1].args, budget)), Some(budget));
    }
}

#[test]
fn scale_keeps_deliberate_values_low_and_child_args_alone() {
    let s = |v: &[&str]| v.iter().map(|a| a.to_string()).collect::<Vec<_>>();
    assert_eq!(scale(&s(&["test", "--", "--test-threads=1"]), 4), s(&["test", "--", "--test-threads=1"]));
    assert_eq!(scale(&s(&["test", "-j", "8"]), 2), s(&["test", "-j", "2"]));
    assert_eq!(scale(&s(&["run", "--", "-j", "6"]), 2), s(&["run", "--", "-j", "6"]));
    assert_eq!(scale(&s(&["run", "--quiet"]), 2), s(&["run", "--quiet"]));
}

#[test]
fn the_override_must_be_a_whole_number_of_one_or_more() {
    assert_eq!(from_value(None), (DEFAULT, None));
    assert_eq!(from_value(Some("12")).0, 12);
    assert_eq!(from_value(Some(" 2 ")).0, 2);
    for bad in ["0", "-3", "six"] {
        let (b, note) = from_value(Some(bad));
        assert_eq!(b, DEFAULT);
        assert!(note.is_some_and(|n| n.contains(bad)), "{bad}");
    }
}
