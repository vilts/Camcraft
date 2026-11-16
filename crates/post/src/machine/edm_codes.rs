//! The EDM control's own words as machine data (#533, #290 D5): the
//! `.mch` file's `edm.codes` object, name -> the text the control reads
//! (`"thread": "M60"`, `"condition": "E{id}"`). A generic EDM post
//! writes the RS-274 motion words itself and takes every EDM-specific
//! code from here; which names it needs is the post's law, and a
//! missing one refuses the post by name. Nothing here is defaulted -
//! the same control word means different things across wire controls,
//! so an unstated code stays unstated.
//!
//! Provenance: our own design (#290 D5, the M-codes-as-data law the
//! coolant and live-tool words already follow); no vendor code table
//! was read or copied.

use std::collections::BTreeMap;

/// Longest code text a file may state: a block or two, never a program.
pub const CODE_MAX: usize = 64;

/// The names the generic sinker post (`sinker-iso.cps`, #542) reads,
/// with the placeholders each text may hold (the post fills them): the
/// unit word for the machine's unit (`metric` or `inch`), the electrode
/// pickup and its reference, the condition select, the burn to depth
/// and the program end. Orbit words are `edm.orbits[].word`
/// (`{radius}`). Only a list of names: every one is read through
/// [`parse_codes`]'s law like any other code.
pub const SINKER_CODES: [(&str, &str); 7] = [
    ("metric", ""),
    ("inch", ""),
    ("pickup", "{tool}"),
    ("reference", "{offset}"),
    ("condition", "{id}"),
    ("depth", "{z} {f}"),
    ("end", ""),
];

/// A code name: a short lowercase word (`thread`, `comp_off`).
fn name_ok(k: &str) -> bool {
    !k.is_empty() && k.len() <= 32 && k.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
}

/// A code text: printable ASCII, not blank, no comment brackets (a
/// code is a word the control executes, never commentary).
fn text_ok(t: &str) -> bool {
    !t.trim().is_empty() && t.len() <= CODE_MAX && t.bytes().all(|b| (0x20..0x7f).contains(&b) && b != b'(' && b != b')')
}

/// The same law [`parse_codes`] enforces at the front door, for a
/// project file's embedded machine.
#[must_use]
pub fn codes_sane(codes: &BTreeMap<String, String>) -> bool {
    codes.iter().all(|(k, v)| name_ok(k) && text_ok(v))
}

/// Read `edm.codes` from the file's `edm` section (empty when either is
/// absent). A garbled entry refuses the file by its name, never reads
/// as unstated.
///
/// # Errors
/// `codes` not an object, a name that is not a lowercase word, or a
/// value that is not a non-blank printable text of at most
/// [`CODE_MAX`] characters without `(` `)`.
pub(super) fn parse_codes(edm: Option<&serde_json::Value>) -> Result<BTreeMap<String, String>, String> {
    let mut out = BTreeMap::new();
    let codes = match edm.and_then(|e| e.get("codes")) {
        None | Some(serde_json::Value::Null) => return Ok(out),
        Some(serde_json::Value::Object(o)) => o,
        Some(x) => return Err(format!("machine file: edm.codes must be an object, not {x}")),
    };
    for (k, v) in codes {
        if !name_ok(k) {
            return Err(format!("machine file: edm.codes name {k:?} must be a lowercase word (a-z, 0-9, _)"));
        }
        match v {
            serde_json::Value::String(s) if text_ok(s) => {
                out.insert(k.clone(), s.clone());
            }
            _ => {
                return Err(format!(
                    "machine file: edm.codes.{k} must be the control's text (printable, at most {CODE_MAX} characters, no brackets), not {v}"
                ))
            }
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codes_parse_and_garbled_ones_refuse_by_name() {
        let v: serde_json::Value = serde_json::from_str(r#"{ "codes": { "thread": "M60", "condition": "E{id}" } }"#).expect("json");
        let c = parse_codes(Some(&v)).expect("parses");
        assert_eq!(c.get("thread").map(String::as_str), Some("M60"));
        assert!(codes_sane(&c));
        assert!(parse_codes(None).expect("none").is_empty());
        for bad in [r#"{ "codes": [] }"#, r#"{ "codes": { "Thread": "M60" } }"#, r#"{ "codes": { "cut": "" } }"#, r#"{ "codes": { "cut": "(x)" } }"#, r#"{ "codes": { "cut": 61 } }"#] {
            let v: serde_json::Value = serde_json::from_str(bad).expect("json");
            assert!(parse_codes(Some(&v)).expect_err(bad).contains("edm.codes"), "{bad}");
        }
    }

    /// #542: the sinker post's names are names this law accepts, each
    /// with the placeholders the post fills; a sinker code the law
    /// refuses (a bracket, a capital name) refuses by its name.
    #[test]
    fn the_sinker_names_pass_the_one_law_and_a_bad_sinker_code_refuses_by_name() {
        let mut codes = BTreeMap::new();
        for (name, holes) in SINKER_CODES {
            assert!(name_ok(name), "{name}");
            codes.insert(name.to_owned(), format!("X {holes}").trim().to_owned());
        }
        assert!(codes_sane(&codes));
        let v: serde_json::Value = serde_json::from_str(r#"{ "codes": { "depth": "G1 Z{z} (BURN)" } }"#).expect("json");
        assert!(parse_codes(Some(&v)).expect_err("bracket").contains("edm.codes.depth"));
        let v: serde_json::Value = serde_json::from_str(r#"{ "codes": { "Pickup": "M6 T{tool}" } }"#).expect("json");
        assert!(parse_codes(Some(&v)).expect_err("capital").contains("\"Pickup\""));
    }
}
