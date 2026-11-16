//! Provenance: the job budget law of issue #2057 (one budget shared by the lanes of a wave).

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

#[test]
fn wave_jobs_sum_to_at_most_the_budget_and_lanes_to_at_most_half() {
    for budget in 1..=16 {
        let steps = suites(11);
        let waves = crate::lanes::waves(&steps, lane_limit(budget));
        let mut seen: Vec<_> = waves.iter().flatten().copied().collect();
        seen.sort_unstable();
        assert_eq!(seen, (0..steps.len()).collect::<Vec<_>>(), "budget {budget}: a step was lost");
        for wave in &waves {
            assert!(wave.len() <= (budget / 2).max(1), "budget {budget}: wave {wave:?} has too many lanes");
            let jobs = per_lane(budget, wave.len());
            let sum: usize = wave.iter().filter_map(|&i| job_value(&scale(&steps[i].args, jobs))).sum();
            let thr: usize = wave.iter().filter_map(|&i| threads(&scale(&steps[i].args, jobs))).sum();
            assert!(sum <= budget.max(1), "budget {budget}: wave {wave:?} has -j sum {sum}");
            assert!(thr <= budget.max(1), "budget {budget}: wave {wave:?} has {thr} test threads");
        }
    }
}

#[test]
fn budget_six_gives_three_lanes_of_two_jobs() {
    assert_eq!(DEFAULT, 6);
    assert_eq!(crate::plan::J, DEFAULT.to_string(), "the plan writes the default budget");
    assert_eq!(lane_limit(6), 3);
    assert_eq!(per_lane(6, 3), 2);
    let waves = crate::lanes::waves(&suites(6), lane_limit(6));
    assert!(waves.iter().any(|w| w.len() == 3), "{waves:?}");
}

#[test]
fn a_step_alone_keeps_the_full_budget() {
    for budget in [1, 4, 6, 12] {
        let steps = suites(0);
        let waves = crate::lanes::waves(&steps, lane_limit(budget));
        assert!(waves.iter().all(|w| w.len() == 1));
        let jobs = per_lane(budget, 1);
        assert_eq!(jobs, budget);
        for &i in waves.iter().flatten() {
            let args = scale(&steps[i].args, jobs);
            assert_eq!(job_value(&args), Some(budget), "{}", steps[i].name);
        }
        assert_eq!(threads(&scale(&steps[1].args, jobs)), Some(budget));
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
