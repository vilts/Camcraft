//! #828: the holder library handed to the post crate's machine reader.
//! `camcraft-post` does not depend on `camcraft-tools` (maintainer,
//! PR #880: a crate edge carrying a couple of items is passed in by the
//! caller instead), so the app passes the library lookup to the reader
//! (`parse_mch_with`), the pane's validator (`tooling_refusals`) and the
//! stored-machine re-judge (`Machine::tooling_refusal`). Our own glue.

use camcraft_post::machine::{HolderInfo, Machine};

/// A built-in library holder by name (case and spacing ignored), as the
/// mount rule reads it; the library's own refusal when there is none.
pub fn holder_lookup(name: &str) -> Result<HolderInfo, String> {
    camcraft_tools::holder_library::find(name)
        .map(|h| HolderInfo { name: h.name(), taper: h.taper.map(|t| t.name.to_owned()) })
        .map_err(|e| e.to_string())
}

/// Parse a `.mch` machine file, its default holder judged against the
/// holder library - the app's one reader (shadows the post crate's
/// word-only `parse_mch`).
pub fn parse_mch(text: &str) -> Result<Machine, String> {
    camcraft_post::machine::parse_mch_with(text, Some(&holder_lookup))
}

/// Every refusal the tooling keys of a `.mch`'s JSON earn, as
/// (key, rule), judged against the holder library.
#[must_use]
pub fn tooling_refusals(v: &serde_json::Value) -> Vec<(String, String)> {
    camcraft_post::machine::tooling_refusals(v, Some(&holder_lookup))
}

/// Why a stored machine's tooling keys cannot be meant, by key.
#[must_use]
pub fn tooling_refusal(m: &Machine) -> Option<String> {
    m.tooling_refusal(Some(&holder_lookup)).or_else(|| edm_refusal(m))
}

/// Why a stored EDM machine's `edm.codes` cannot be meant (#535): the
/// `condition` code is a template that selects the op's condition by
/// its id, so one without `{id}` would select nothing - the same law
/// the wire post refuses by (#533), asked at the machine door so an
/// edit never mounts it.
#[must_use]
pub fn edm_refusal(m: &Machine) -> Option<String> {
    for (k, t) in &m.edm_codes {
        if t.trim().is_empty() {
            return Some(format!("edm.codes.{k} is blank - write the control's text or remove the code"));
        }
        if t.contains('(') || t.contains(')') {
            return Some(format!("edm.codes.{k} {t:?} holds a bracket - a code is a word the control executes, never commentary"));
        }
    }
    if let Some(t) = m.edm_codes.get("condition")
        && !t.contains("{id}")
    {
        return Some(format!(
            "edm.codes.condition {t:?} states no {{id}} - the control would select no condition (write e.g. \"E{{id}}\")"
        ));
    }
    let conds = m.edm.as_ref().map_or(&[][..], |e| e.conditions.as_slice());
    for (i, c) in conds.iter().enumerate() {
        if c.id.trim().is_empty() {
            return Some(format!("EDM condition {} states no id - the control selects it by its id", i + 1));
        }
        if conds.iter().skip(i + 1).any(|d| d.id == c.id) {
            return Some(format!("EDM condition id {:?} is stated twice", c.id));
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn edm_refusal_names_the_code_or_condition_that_cannot_be_meant() {
        let mut m = Machine::default();
        assert_eq!(edm_refusal(&m), None, "nothing stated, nothing refused");
        m.edm_codes.insert("condition".into(), "FX-COND".into());
        assert!(edm_refusal(&m).is_some_and(|r| r.contains("edm.codes.condition") && r.contains("{id}")));
        m.edm_codes.insert("condition".into(), "FX-COND {id}".into());
        assert_eq!(edm_refusal(&m), None);
        m.edm_codes.insert("cut".into(), " ".into());
        assert!(edm_refusal(&m).is_some_and(|r| r.contains("edm.codes.cut")));
        m.edm_codes.remove("cut");
        let c = camcraft_post::machine::Condition { id: "C1".into(), ..Default::default() };
        m.edm = Some(camcraft_post::machine::Edm { conditions: vec![c.clone(), c], ..Default::default() });
        assert!(edm_refusal(&m).is_some_and(|r| r.contains("twice")));
    }
}
