//! Step selection for `cargo xtask gate`: `--only <step>` and `--failed <gate-logs/run>` (#1702).
//! Provenance: our own build law (AGENTS.md "Three gate tiers"); reads only our own gate logs.

use std::path::Path;

use crate::exec::log_name;
use crate::plan::Plan;

/// True when `name` is `word`, or starts with `word` followed by a non-word character
/// ("kernel" selects "kernel suites"; "app" does not select "application"). Case-insensitive.
pub fn name_matches(name: &str, word: &str) -> bool {
    let (n, w) = (name.to_lowercase(), word.trim().to_lowercase());
    if w.is_empty() {
        return false;
    }
    match n.strip_prefix(&w) {
        Some("") => true,
        Some(rest) => rest.chars().next().is_some_and(|c| !(c.is_alphanumeric() || c == '_')),
        None => false,
    }
}

/// True when the log text of one step shows a failure: a failed libtest result, or a cargo error line.
pub fn log_failed(text: &str) -> bool {
    text.lines().any(|l| l.starts_with("test result: FAILED") || l.starts_with("error:"))
}

/// The step stems (log name without the `NN-` prefix and `.log`) of the failed steps of one run, from
/// `(file name, log text)` pairs. A step without a log was never run and is not listed.
pub fn failed_stems(logs: &[(String, String)]) -> Vec<String> {
    let mut out = Vec::new();
    for (file, text) in logs {
        let Some(stem) = file.strip_suffix(".log") else { continue };
        let Some((num, rest)) = stem.split_once('-') else { continue };
        if num.is_empty() || !num.chars().all(|c| c.is_ascii_digit()) {
            continue;
        }
        if log_failed(text) {
            out.push(rest.to_string());
        }
    }
    out
}

/// The failed step stems of the run in `dir` (a `gate-logs/<tier>-<stamp>` directory).
pub fn failed_in_dir(dir: &Path) -> Result<Vec<String>, String> {
    let rd = std::fs::read_dir(dir).map_err(|e| format!("--failed: cannot read {}: {e}", dir.display()))?;
    let mut logs = Vec::new();
    for e in rd.flatten() {
        let file = e.file_name().to_string_lossy().into_owned();
        if file.ends_with(".log") {
            logs.push((file, std::fs::read_to_string(e.path()).unwrap_or_default()));
        }
    }
    logs.sort();
    let stems = failed_stems(&logs);
    if stems.is_empty() {
        return Err(format!("--failed: no failed step in {}", dir.display()));
    }
    Ok(stems)
}

/// Keeps the steps named by `only` (a step stays when any word matches) and, when `failed` is given,
/// only those whose log stem is in it. An empty selection is an error that lists the step names.
pub fn apply(mut plan: Plan, only: &[String], failed: Option<&[String]>) -> Result<Plan, String> {
    if only.is_empty() && failed.is_none() {
        return Ok(plan);
    }
    let all: Vec<String> = plan.steps.iter().map(|s| s.name.clone()).collect();
    plan.steps.retain(|s| {
        let by_only = only.is_empty() || only.iter().any(|w| name_matches(&s.name, w));
        let stem = log_name(0, &s.name);
        let stem = stem.split_once('-').map(|x| x.1).unwrap_or("").trim_end_matches(".log");
        let by_failed = failed.is_none_or(|f| f.iter().any(|x| x == stem));
        by_only && by_failed
    });
    if plan.steps.is_empty() {
        return Err(format!(
            "no step of this plan matches the selection (only: {only:?}, failed: {failed:?}); steps: {}",
            all.join("; ")
        ));
    }
    let names: Vec<&str> = plan.steps.iter().map(|s| s.name.as_str()).collect();
    plan.notes.push(format!("selected {} of {} step(s): {}", names.len(), all.len(), names.join("; ")));
    Ok(plan)
}
