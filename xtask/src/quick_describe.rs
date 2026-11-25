//! The quick tier's plan text: what each pick runs and why.
//! Provenance: our own build law (AGENTS.md "Three gate tiers"), split from quick.rs (#3706).

use std::collections::BTreeSet;

use super::Picks;
use crate::plan::{ps_cmp, ps_sorted};

pub(super) fn by_fn(set: &BTreeSet<(String, String)>) -> Vec<(String, String)> {
    let mut v: Vec<_> = set.iter().cloned().collect();
    v.sort_by(|a, b| ps_cmp(&a.0, &b.0).then_with(|| ps_cmp(&a.1, &b.1)));
    v
}

pub(super) fn describe(p: &Picks, heavy: bool, xtask: bool, unmapped: &[String], notes: &mut Vec<String>) {
    notes.push("quick plan:".into());
    for d in ps_sorted(p.libs.iter().cloned()) {
        notes.push(format!("  {d} lib (unit tests)"));
    }
    for k in ps_sorted(p.whole.iter().cloned()) {
        let why = p.why.get(&k).map(String::as_str).unwrap_or("");
        notes.push(format!("  {k}  whole suite - {why}"));
    }
    super::subs::describe(p, notes);
    let ign_note = if heavy {
        "  [ignored -> release --ignored]"
    } else {
        "  [ignored: heavy lane, skipped - pass --heavy or use `gate test <name>`]"
    };
    let mut both: Vec<(String, String, bool)> = by_fn(&p.fns)
        .into_iter()
        .map(|(k, f)| (k, f, false))
        .collect();
    both.extend(by_fn(&p.ign).into_iter().map(|(k, f)| (k, f, true)));
    both.sort_by(|a, b| ps_cmp(&a.0, &b.0).then_with(|| ps_cmp(&a.1, &b.1)));
    for (k, f, ig) in both {
        if !p.whole.contains(&k) {
            notes.push(format!("  {k}::{f}{}", if ig { ign_note } else { "" }));
        }
    }
    if xtask {
        notes.push("  xtask (unit tests) - xtask/ changed".into());
    }
    for a in unmapped {
        notes.push(format!(
            "  {a}: NO SUITE covers this asset - nothing posts through it in crates/*/tests"
        ));
    }
    if unmapped.is_empty()
        && p.libs.is_empty()
        && p.whole.is_empty()
        && p.fns.is_empty()
        && p.ign.is_empty()
        && p.subs.is_empty()
        && !xtask
    {
        notes.push("  nothing in crates/ changed: no tests to run".into());
    }
}
