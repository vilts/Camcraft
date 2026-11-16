//! One job budget for a whole gate run (#2057): the lanes of a wave share it, so `-j` summed over a
//! wave never exceeds it. Provenance: our own build law (AGENTS.md "Three gate tiers"), cargo's `-j`
//! and libtest's `--test-threads` flags as documented in the Cargo Book and the rustc test docs.

/// The budget when `CAMCRAFT_GATE_JOBS` is not set. The plan writes this value as `plan::J`.
pub const DEFAULT: usize = 6;
/// The environment variable that overrides the budget (a whole number of 1 or more).
pub const ENV: &str = "CAMCRAFT_GATE_JOBS";

/// The budget from the value of `ENV`; `None` or a bad value gives `DEFAULT` and a note.
pub fn from_value(v: Option<&str>) -> (usize, Option<String>) {
    match v.map(str::trim) {
        None | Some("") => (DEFAULT, None),
        Some(s) => match s.parse::<usize>() {
            Ok(n) if n >= 1 => (n, None),
            _ => (DEFAULT, Some(format!("note: {ENV}={s} is not a whole number of 1 or more; the budget stays {DEFAULT}"))),
        },
    }
}

/// The budget of this process.
pub fn from_env() -> (usize, Option<String>) {
    from_value(std::env::var(ENV).ok().as_deref())
}

/// The most lanes in one wave: half the budget, so each lane has two jobs or more; at least one.
pub fn lane_limit(budget: usize) -> usize {
    (budget / 2).max(1)
}

/// The jobs of each lane when `lanes` steps share `budget`; at least one. A step alone gets all of it.
pub fn per_lane(budget: usize, lanes: usize) -> usize {
    (budget / lanes.max(1)).max(1)
}

/// The args with `-j` and `--test-threads=` set to `jobs`. A value that the plan did not write as
/// `plan::J` is a deliberate choice: it only goes down to `jobs`, never up. Args without those flags
/// are unchanged; `-j` after `--` belongs to the child program and is left alone.
pub fn scale(args: &[String], jobs: usize) -> Vec<String> {
    let pick = |v: &str| match v.parse::<usize>() {
        Ok(n) if v != crate::plan::J => n.min(jobs).max(1),
        _ => jobs,
    };
    let mut out = Vec::with_capacity(args.len());
    let (mut child, mut after_j) = (false, false);
    for a in args {
        if after_j {
            out.push(pick(a).to_string());
            after_j = false;
        } else if !child && a == "-j" {
            out.push(a.clone());
            after_j = true;
        } else if let Some(v) = a.strip_prefix("--test-threads=") {
            out.push(format!("--test-threads={}", pick(v)));
        } else {
            child |= a == "--";
            out.push(a.clone());
        }
    }
    out
}

/// The line printed at the start of a gate: the budget, the lane limit and the jobs of each lane.
pub fn header(budget: usize, lanes: bool) -> String {
    if !lanes {
        return format!("gate budget: {budget} job(s); steps run one at a time with -j {budget}");
    }
    let limit = lane_limit(budget);
    let split: Vec<String> = (1..=limit).map(|n| format!("{n} lane(s) x -j {}", per_lane(budget, n))).collect();
    format!("gate budget: {budget} job(s) ({ENV} overrides); at most {limit} lane(s) per wave; {}", split.join(", "))
}
