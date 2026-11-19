//! The heavy tier's plan (#2568): from the files a diff touches to the release `--ignored` lanes it can
//! reach, also through the dependencies between our crates (a change in `motion` reaches the `app` lanes).
//! Provenance: our own build law (AGENTS.md "Heavy lanes at review"); reads only our own Cargo manifests,
//! by the dependency-table syntax of the public Cargo reference.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use crate::plan::{ALL_CRATES, HEAVY_CRATES, Plan, heavy_steps_where};
use crate::{assets, fixture_inputs, suites};

/// For each crate dir, the crate dirs that a change in it reaches: itself and every crate that depends on
/// it, directly or through others, by normal, dev or build dependency. An unreadable manifest is an
/// error: a missing edge would hide a heavy lane.
pub fn reach(root: &Path) -> Result<BTreeMap<String, BTreeSet<String>>, String> {
    let mut deps: BTreeMap<&str, BTreeSet<&str>> = BTreeMap::new();
    for &d in ALL_CRATES {
        let file = root.join("crates").join(d).join("Cargo.toml");
        let text = std::fs::read_to_string(&file).map_err(|e| format!("gate heavy: cannot read {}: {e}", file.display()))?;
        let mine = deps.entry(d).or_default();
        for line in text.lines() {
            // A dependency key: `camcraft-<dir> = ...` or `camcraft-<dir>.workspace = true`.
            let Some(rest) = line.trim_start().strip_prefix("camcraft-") else { continue };
            let name: String = rest.chars().take_while(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_')).collect();
            if let Some(&dep) = ALL_CRATES.iter().find(|c| **c == name && **c != d) {
                mine.insert(dep);
            }
        }
    }
    let mut out: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for &d in ALL_CRATES {
        // Every crate from which `d` is reachable along the dependency edges; `seen` ends a cycle.
        let mut seen: BTreeSet<&str> = BTreeSet::from([d]);
        let mut grew = true;
        while grew {
            grew = false;
            for (&c, of) in &deps {
                if !seen.contains(c) && of.iter().any(|x| seen.contains(x)) {
                    seen.insert(c);
                    grew = true;
                }
            }
        }
        out.insert(d.to_string(), seen.into_iter().map(String::from).collect());
    }
    Ok(out)
}

/// What the diff reaches: whole heavy crates, and single targets (`crate:label`), each with its reason.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Reached {
    pub crates: BTreeMap<String, String>,
    pub targets: BTreeMap<String, String>,
}

impl Reached {
    pub fn keeps(&self, dir: &str, label: &str) -> bool {
        self.crates.contains_key(dir) || self.targets.contains_key(&format!("{dir}:{label}"))
    }
}

fn is_heavy(dir: &str) -> bool {
    HEAVY_CRATES.contains(&dir)
}

/// Maps `changed` (`/`-separated, relative to the root) to the heavy crates and targets that it reaches.
/// - a workspace manifest, lock file, cargo config or toolchain file: every heavy crate;
/// - a file of one suite (`crates/<d>/tests/<suite>.rs`, `tests/<suite>/..`): that binary only;
/// - other files under `crates/<d>/tests/`: the heavy lanes of `d` (shared test code or input);
/// - any other file of crate `d` that is not Markdown: the heavy lanes of each crate that `d` reaches;
/// - a post script, machine file, catalog case or fixture: the heavy binaries that read it (as quick does).
pub fn reached(root: &Path, changed: &[String]) -> Result<Reached, String> {
    let reach = reach(root)?;
    let mut r = Reached::default();
    let (mut libs, mut whole, mut why) = (BTreeSet::new(), BTreeSet::new(), BTreeMap::new());
    for f in changed {
        let workspace = matches!(f.as_str(), "Cargo.toml" | "Cargo.lock") || f.starts_with(".cargo/") || f.starts_with("rust-toolchain");
        if workspace {
            for d in HEAVY_CRATES {
                r.crates.entry((*d).to_string()).or_insert_with(|| format!("workspace file {f}"));
            }
            continue;
        }
        let Some((d, rest)) = f.strip_prefix("crates/").and_then(|x| x.split_once('/')) else { continue };
        if !ALL_CRATES.contains(&d) {
            continue;
        }
        if let Some(test) = rest.strip_prefix("tests/") {
            if !is_heavy(d) {
                continue;
            }
            let flat = !test.contains('/') && test.ends_with(".rs");
            if flat && !root.join(f).is_file() {
                continue; // deleted flat suite: Cargo no longer has this target
            }
            let name = test.split('/').next().unwrap_or(test);
            let name = if flat { name.trim_end_matches(".rs") } else { name };
            if suites::entry(root, d, name).is_file() {
                r.targets.entry(format!("{d}:{name}")).or_insert_with(|| format!("suite changed ({f})"));
            } else {
                r.crates.entry(d.to_string()).or_insert_with(|| format!("shared test input {f}"));
            }
        } else if !rest.to_ascii_lowercase().ends_with(".md") {
            for c in reach.get(d).into_iter().flatten().filter(|c| is_heavy(c)) {
                let how = if c == d { format!("{f} changed") } else { format!("depends on {d} ({f} changed)") };
                r.crates.entry(c.clone()).or_insert(how);
            }
        }
    }
    for a in assets::changed_assets(changed) {
        if root.join(&a.path).is_file() {
            for (k, how) in assets::suites_for(root, &a)? {
                whole.insert(k.clone());
                why.entry(k).or_insert_with(|| format!("{} {how}", a.path));
            }
        }
    }
    crate::catalog::map_catalog(root, changed, &mut whole, &mut why);
    fixture_inputs::map(root, changed, &mut libs, &mut whole, &mut why)?;
    whole.extend(libs.iter().map(|d| format!("{d}:lib")));
    for k in whole {
        if k.split(':').next().is_some_and(is_heavy) {
            let how = why.get(&k).cloned().unwrap_or_default();
            r.targets.entry(k).or_insert(how);
        }
    }
    Ok(r)
}

/// The plan of `cargo xtask gate heavy`: one release build of the reached heavy crates, then one
/// `--ignored` step for each reached target. No reached lane gives a plan with a note and no step.
pub fn plan(root: &Path, changed: &[String]) -> Result<Plan, String> {
    let r = reached(root, changed)?;
    let mut plan = Plan { notes: Vec::new(), steps: heavy_steps_where(root, &|d, label| r.keeps(d, label)) };
    plan.notes.push("heavy plan (release --ignored lanes the diff reaches, #2568):".into());
    for (d, how) in &r.crates {
        plan.notes.push(format!("  {d} heavy: every target - {how}"));
    }
    for (k, how) in r.targets.iter().filter(|(k, _)| !r.crates.contains_key(k.split(':').next().unwrap_or(""))) {
        let (d, label) = k.split_once(':').unwrap_or((k, ""));
        let planned = plan.steps.iter().any(|s| s.name == format!("{d} heavy: {label}"));
        let none = if planned { "" } else { " [no #[ignore]d test: no step]" };
        plan.notes.push(format!("  {d} heavy: {label} - {how}{none}"));
    }
    if plan.steps.is_empty() {
        plan.notes.push("  the diff reaches no heavy lane: nothing to run".into());
    }
    Ok(plan)
}
