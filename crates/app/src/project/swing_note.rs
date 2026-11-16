//! The load note for a machine embedded before #804 (#813, #818): a
//! rotary that may turn the work about a line that is not vertical (an
//! A, a B, a leaning or unknown line) but carries no `turns_part` was saved
//! before the index-swing clause read it, so that clause cannot judge
//! it. Opening such a project names the machine and the rotary and asks
//! for the machine to be reloaded - never a silent "not judged".
//!
//! Provenance: our own persistence contract (the #804 field, the #342
//! load-note channel); no vendor material consulted.

use super::ProjectFile;
use crate::posting::index_swing::{RotaryLine, rotary_line};

/// One note per embedded machine (the document's, then each setup's own)
/// holding a rotary whose index could swing the work up (any line but a
/// vertical one: a level, leaning or unknown line) whose kind - turning
/// the work or the tool - the file never stated.
pub(super) fn notes(file: &ProjectFile) -> Vec<String> {
    let mut machines: Vec<(String, &crate::machine::Machine)> = Vec::new();
    if let Some(m) = &file.machine {
        machines.push(("the document".to_owned(), &m.machine));
    }
    for dto in &file.entities {
        if let Some(m) = dto.setup.as_ref().and_then(|s| s.machine.as_ref()).and_then(|m| m.machine.as_ref()) {
            machines.push((format!("setup '{}'", dto.name), m));
        }
    }
    machines
        .into_iter()
        .filter_map(|(owner, m)| {
            let unknown: Vec<&str> = m
                .axes
                .iter()
                .filter(|a| a.rotary && a.turns_part.is_none() && rotary_line(a) != RotaryLine::Vertical)
                .map(|a| a.name.as_str())
                .collect();
            (!unknown.is_empty()).then(|| {
                format!(
                    "{owner}'s machine {} was saved before rotaries stated whether they turn the work: rotary {} \
                     is not judged by the index-swing check - reload the machine so it is",
                    m.model,
                    unknown.join(", ")
                )
            })
        })
        .collect()
}
