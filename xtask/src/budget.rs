//! One job budget for a whole gate run (#2057): the lanes of the release queue share it, so `-j` summed
//! over the running lanes never exceeds it. Provenance: our own build law (AGENTS.md "Three gate tiers"), cargo's `-j`
//! and libtest's `--test-threads` flags as documented in the Cargo Book and the rustc test docs.

/// The environment variable that overrides the budget (a whole number of 1 or more).
pub const ENV: &str = "CAMCRAFT_GATE_JOBS";

/// Resolve the budget from a CPU count, or use two jobs if CPU detection failed.
/// Only the automatic default has a floor of two and a cap of 64; explicit overrides win.
pub fn from_cpu_value(v: Option<&str>, cpus: Option<usize>) -> (usize, Option<String>) {
    let default = cpus.unwrap_or(2).clamp(2, 64);
    let source = match cpus {
        Some(n) => format!("available CPUs: {n}, floor 2, cap 64"),
        None => "available CPUs unavailable, fallback 2".to_string(),
    };
    match v.map(str::trim) {
        None | Some("") => (default, Some(format!("gate budget source: {source}"))),
        Some(s) => match s.parse::<usize>() {
            Ok(n) if n >= 1 => (n, Some(format!("gate budget source: {ENV}={n}"))),
            _ => (default, Some(format!("note: {ENV}={s} is not a whole number of 1 or more; using {default} job(s) ({source})"))),
        },
    }
}

/// The budget from the environment value and the available CPU count.
pub fn from_value(v: Option<&str>) -> (usize, Option<String>) {
    from_cpu_value(v, std::thread::available_parallelism().ok().map(|n| n.get()))
}

/// The budget of this process.
pub fn from_env() -> (usize, Option<String>) {
    from_value(std::env::var(ENV).ok().as_deref())
}

/// The most lanes of the release queue: half the budget, so each lane has two jobs or more; at least one.
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
    format!("gate budget: {budget} job(s) ({ENV} overrides); at most {limit} queue lane(s); alone steps take all {budget}; {}", split.join(", "))
}
