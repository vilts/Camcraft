//! Quick tier: a change in one submodule of a directory suite runs only that submodule (#3706).
//! Provenance: libtest's name filter (a test runs when its path contains the filter) and our gate law
//! (AGENTS.md "Test the change, not everything"; when in doubt, the whole suite).
//!
//! A file `tests/<suite>/<sub>.rs` that changes outside its test fns runs `--test <suite> -- <sub>::`
//! when the file holds no item that other modules can use (`pub`, `pub(..)`, a trait impl, a macro).
//! A change of `main.rs`, of a shared helper module, of a cfg-gated module, or of more than one
//! submodule of a suite runs the whole suite.

use std::collections::{BTreeMap, BTreeSet};

use super::Picks;
use crate::plan::{J, Step, T, ps_sorted};
use crate::scan::{self, re};
use crate::suites::Source;

/// The libtest module filter for a change outside the test fns of `source`, or `None` for the whole
/// suite. `before` is the text at the merge base (`None`: unknown, so the whole suite).
pub(super) fn filter(source: Option<&Source>, before: Option<&str>) -> Result<Option<String>, String> {
    let Some(s) = source else { return Ok(None) };
    let Some(before) = before else { return Ok(None) };
    if !s.precise || s.module.is_empty() {
        return Ok(None);
    }
    // `pub`, `pub(crate)`, `pub(super)`, `pub(in ..)`, also after attributes on the same line; a trait
    // impl (`impl Trait for Type`), which other modules see; a macro (#3711).
    let shared = re(concat!(
        r"(?m)^\s*(?:#\s*\[[^\]]*\]\s*)*pub\b",
        r"|^\s*(?:#\s*\[[^\]]*\]\s*)*(?:unsafe\s+)?impl\b[^{;]*\bfor\b",
        r"|macro_rules!|macro_export|macro_use",
    ))?;
    let now = scan::read(&s.path);
    if now.is_empty() || shared.is_match(&now) || shared.is_match(before) {
        return Ok(None);
    }
    Ok(Some(s.module.clone()))
}

/// After all whole-suite picks: a suite with one changed submodule keeps its filter (and drops the
/// exact fns that the filter runs); a suite picked whole, or with more than one changed submodule,
/// runs whole.
pub(super) fn settle(p: &mut Picks) {
    let mut by_key: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for (k, m) in std::mem::take(&mut p.subs) {
        by_key.entry(k).or_default().insert(m);
    }
    for (k, mods) in by_key {
        if p.whole.contains(&k) {
            continue;
        }
        let mut mods = mods.into_iter();
        let (Some(m), None) = (mods.next(), mods.next()) else {
            p.whole.insert(k.clone());
            p.why.insert(k, "more than one submodule changed".into());
            continue;
        };
        let prefix = format!("{m}::");
        p.fns.retain(|(fk, f)| !(fk == &k && f.starts_with(&prefix)));
        p.subs.insert((k, m));
    }
}

/// The plan lines of the submodule filters.
pub(super) fn describe(p: &Picks, notes: &mut Vec<String>) {
    for (k, m) in &p.subs {
        notes.push(format!("  {k}  {m}:: - submodule changed outside its test fns"));
    }
}

/// One step for each submodule filter of crate dir `d` (package `pk`).
pub(super) fn steps(p: &Picks, d: &str, pk: &str) -> Vec<Step> {
    let keys = ps_sorted(p.subs.iter().map(|(k, m)| format!("{k}\t{m}")));
    let mut out = Vec::new();
    for km in keys {
        let Some((k, m)) = km.split_once('\t') else { continue };
        let Some((kd, b)) = k.split_once(':') else { continue };
        if kd != d {
            continue;
        }
        let filter = format!("{m}::");
        let args = ["test", "-j", J, "-p", pk, "--no-fail-fast", "--test", b, "--", T, filter.as_str()];
        out.push(Step::new(format!("{d} {b} ({filter})"), &args, d));
    }
    out
}
