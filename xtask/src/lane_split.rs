//! `gate batch --lane N/M` (#2908): the batch plan cut into M lanes, one for each CI job, so that the
//! lanes run on different runners at the same time or queue on one runner with the same result.
//! Provenance: issue #2908 (the maintainer's request for lanes); a plain round-robin deal over our own
//! gate plan. The split reads only the plan, never a timing file, so each machine computes the same lanes.

use crate::lanes;
use crate::plan::{Plan, Step};

/// More lanes than this is a typing error, not a runner pool.
pub const MAX_LANES: usize = 16;

/// Lane `index` (1-based) of `of`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Lane {
    pub index: usize,
    pub of: usize,
}

impl Lane {
    pub fn label(&self) -> String {
        format!("{}/{}", self.index, self.of)
    }
}

/// `N/M` with `1 <= N <= M <= MAX_LANES`; anything else is refused by name.
pub fn parse(text: &str) -> Result<Lane, String> {
    let bad = || format!("--lane needs N/M with 1 <= N <= M <= {MAX_LANES} (for example 2/3), not '{text}'");
    let (n, m) = text.split_once('/').ok_or_else(bad)?;
    let index: usize = n.trim().parse().map_err(|_| bad())?;
    let of: usize = m.trim().parse().map_err(|_| bad())?;
    if index == 0 || index > of || of > MAX_LANES {
        return Err(bad());
    }
    Ok(Lane { index, of })
}

/// The `-p` packages of a step's cargo command (before `--`).
fn packages(s: &Step) -> Vec<&str> {
    let end = s.args.iter().position(|a| a == "--").unwrap_or(s.args.len());
    s.args[..end].windows(2).filter(|w| w[0] == "-p").map(|w| w[1].as_str()).collect()
}

fn is_clippy(s: &Step) -> bool {
    s.args.first().is_some_and(|a| a == "clippy")
}

/// The lane (1-based) of each step of `steps`, `None` for a build step. The steps that share the machine
/// (`lanes::alone` false) are dealt first, then the steps that run alone, each in plan order and one
/// after the other round the `of` lanes: each lane gets its share of the two kinds. Each step that is
/// not a build is in exactly one lane.
pub fn assign(steps: &[Step], of: usize) -> Vec<Option<usize>> {
    let mut out = vec![None; steps.len()];
    let of = of.max(1);
    let mut dealt = 0usize;
    for want_alone in [false, true] {
        for (i, s) in steps.iter().enumerate() {
            if !lanes::is_build(s) && lanes::alone(s) == want_alone {
                out[i] = Some(dealt % of + 1);
                dealt += 1;
            }
        }
    }
    out
}

/// The steps of `lane`: its share of the test steps and clippy (`assign`), and before them each build
/// step that names a package of one of its test steps. Clippy compiles for itself and keeps no build.
/// A lane with no step is an error: a job that runs nothing must not report green.
pub fn apply(mut plan: Plan, lane: Lane) -> Result<Plan, String> {
    let lane_of = assign(&plan.steps, lane.of);
    let total = lane_of.iter().flatten().count();
    let mine = |i: usize| lane_of[i] == Some(lane.index);
    let needed: Vec<String> = plan
        .steps
        .iter()
        .enumerate()
        .filter(|(i, s)| mine(*i) && !is_clippy(s))
        .flat_map(|(_, s)| packages(s))
        .map(String::from)
        .collect();
    let keep: Vec<bool> = plan
        .steps
        .iter()
        .enumerate()
        .map(|(i, s)| match lane_of[i] {
            Some(_) => mine(i),
            None => packages(s).iter().any(|p| needed.iter().any(|n| n == p)),
        })
        .collect();
    let mut flags = keep.iter();
    plan.steps.retain(|_| flags.next().copied().unwrap_or(false));
    let builds = plan.steps.iter().filter(|s| lanes::is_build(s)).count();
    let tests = plan.steps.len() - builds;
    if tests == 0 {
        return Err(format!(
            "lane {} has no step: the plan has {total} step(s) to deal, use fewer lanes",
            lane.label()
        ));
    }
    plan.notes.push(format!(
        "lane {}: {tests} of {total} step(s) of the plan, after {builds} build(s) (--lane, #2908)",
        lane.label()
    ));
    Ok(plan)
}
