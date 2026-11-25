//! The options of `cargo xtask gate`: the tier, the test filter and the step selection flags.
//! Provenance: our own gate law (AGENTS.md "Three gate tiers", "Heavy lanes at review"); moved out of
//! main.rs when `--covers` (#3460) was added, so that main.rs stays under the 400-line law.

use crate::heavy_covers::{self, Cover};
use crate::{lane_split, USAGE};

/// Parsed `gate` options.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct GateArgs {
    pub tier: String,
    pub filter: Option<String>,
    pub base: String,
    pub krate: Option<String>,
    pub binary: Option<String>,
    pub exact: bool,
    pub heavy: bool,
    pub dry: bool,
    /// `gate release --no-snapshots` (#1413): the release plan without the two ui pixel-snapshot lanes.
    pub no_snapshots: bool,
    /// `--only <step>` (repeatable, #1702): keep only the steps whose name starts with a given word.
    pub only: Vec<String>,
    /// `--failed <gate-logs/run>` (#1702): keep only the steps that failed in that earlier run.
    pub failed: Option<String>,
    /// `gate batch --lane N/M` (#2908): keep only the steps of that lane (`lane_split`).
    pub lane: Option<lane_split::Lane>,
    /// `gate heavy --covers <PR>:<sha>` (repeatable, #3460): the PR heads that a wave's trial tree covers.
    pub covers: Vec<Cover>,
}

pub fn parse_gate(args: &[String]) -> Result<GateArgs, String> {
    let mut g = GateArgs {
        base: "main".into(),
        ..GateArgs::default()
    };
    let mut it = args.iter();
    let value = |flag: &str, it: &mut std::slice::Iter<String>| {
        it.next()
            .cloned()
            .ok_or_else(|| format!("{flag} needs a value"))
    };
    while let Some(a) = it.next() {
        match a.as_str() {
            "--base" => g.base = value(a, &mut it)?,
            "--crate" => g.krate = Some(value(a, &mut it)?),
            "--binary" => g.binary = Some(value(a, &mut it)?),
            "--exact" => g.exact = true,
            "--heavy" => g.heavy = true,
            "--dry-run" | "--plan" => g.dry = true,
            "--no-snapshots" => g.no_snapshots = true,
            "--only" => g.only.push(value(a, &mut it)?),
            "--failed" => g.failed = Some(value(a, &mut it)?),
            "--lane" => g.lane = Some(lane_split::parse(&value(a, &mut it)?)?),
            "--covers" => {
                let c = heavy_covers::parse(&value(a, &mut it)?).map_err(|e| format!("{e}\n{USAGE}"))?;
                if g.covers.iter().any(|o| o.pr == c.pr) {
                    return Err(format!("--covers names #{} two times\n{USAGE}", c.pr));
                }
                g.covers.push(c);
            }
            s if s.starts_with('-') => return Err(format!("unknown gate option '{s}'\n{USAGE}")),
            s if g.tier.is_empty() => g.tier = s.to_string(),
            s if g.tier == "test" && g.filter.is_none() => g.filter = Some(s.to_string()),
            s => return Err(format!("unexpected argument '{s}'\n{USAGE}")),
        }
    }
    if g.tier.is_empty() {
        g.tier = "quick".into();
    }
    if g.no_snapshots && g.tier != "release" {
        return Err(format!("--no-snapshots applies to the release tier only, not '{}'", g.tier));
    }
    if g.lane.is_some() && g.tier != "batch" {
        return Err(format!("--lane splits the batch tier only, not '{}'", g.tier));
    }
    if !g.covers.is_empty() && g.tier != "heavy" {
        return Err(format!("--covers applies to the heavy tier only, not '{}'\n{USAGE}", g.tier));
    }
    if !g.covers.is_empty() && (!g.only.is_empty() || g.failed.is_some()) {
        return Err(format!("--covers records a full heavy run: it does not take --only or --failed\n{USAGE}"));
    }
    if g.tier == "test" && (!g.only.is_empty() || g.failed.is_some()) {
        return Err("--only and --failed select steps of quick, heavy, batch, light or release, not of 'gate test'".into());
    }
    match g.tier.as_str() {
        "quick" | "heavy" | "batch" | "light" | "release" => Ok(g),
        "test" => match &g.filter {
            Some(f)
                if f.split("::").all(|p| {
                    !p.is_empty() && p.chars().all(|c| c.is_alphanumeric() || c == '_')
                }) =>
            {
                Ok(g)
            }
            Some(f) => Err(format!(
                "test filter must be a fn name or module::fn path (letters, digits, _): '{f}'"
            )),
            None => Err(format!("gate test needs a filter\n{USAGE}")),
        },
        t => Err(format!(
            "unknown tier '{t}' (quick | heavy | batch | light | release | test)\n{USAGE}"
        )),
    }
}
