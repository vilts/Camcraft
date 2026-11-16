//! The v59 -> v60 rung (#902): the document's unit becomes the unit
//! every program posts in. Before v60 the display unit was a lens only
//! and a post took its unit from the machine file (`units.length`), so
//! an older file opens on the unit its programs posted in - the posted
//! program is byte-identical, and only the display may change, which a
//! load note says. A file whose setups posted in different units cannot
//! keep both in one document setting: it keeps its display unit and a
//! note names every setup whose program unit changes.
//!
//! Provenance: our own persistence contract (the #342 load-note
//! channel, `swing_note`); no vendor material consulted.

use super::ProjectFile;

/// Whether a machine definition posted inch before #902 - the mirror's
/// own reading of `units` (`post::machine`), anything else mm.
fn inch(m: Option<&crate::machine::Machine>) -> bool {
    matches!(m.and_then(|m| m.units.as_deref()), Some("inch" | "inches"))
}

fn word(inch: bool) -> &'static str {
    if inch { "inch" } else { "mm" }
}

/// Settle an older file's unit; the load notes, if any.
pub(super) fn settle(file: &mut ProjectFile) -> Vec<String> {
    let doc = inch(file.machine.as_ref().map(|m| &m.machine));
    // What each setup posted in: its own machine, a deliberate "no
    // machine" (the default VMC, mm), or the document's.
    let setups: Vec<(String, bool)> = file
        .entities
        .iter()
        .filter_map(|e| e.setup.as_ref().map(|s| (e.name.clone(), s)))
        .map(|(name, s)| (name, s.machine.as_ref().map_or(doc, |m| inch(m.machine.as_ref()))))
        .collect();
    let shown_inch = file.units.as_deref() == Some("in");
    let all = |u: bool| setups.iter().all(|(_, s)| *s == u);
    let posted = if setups.is_empty() { Some(doc) } else if all(true) { Some(true) } else if all(false) { Some(false) } else { None };
    match posted {
        Some(u) if u == shown_inch => Vec::new(),
        Some(u) => {
            file.units = u.then(|| "in".to_owned());
            vec![format!(
                "the document displayed {} but its programs posted in {} (from the machine file); a document's unit is now \
                 its programs' unit too, so it opens in {} and posts exactly as before",
                word(shown_inch),
                word(u),
                word(u)
            )]
        }
        None => setups
            .iter()
            .filter(|(_, s)| *s != shown_inch)
            .map(|(name, s)| {
                format!(
                    "setup '{name}' posted in {} (from its machine file); every program is now written in the document's \
                     unit ({}) - set the document's units to {} to post it as before",
                    word(*s),
                    word(shown_inch),
                    word(*s)
                )
            })
            .collect(),
    }
}
