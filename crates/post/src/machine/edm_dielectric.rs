//! The dielectric a wire EDM job runs in (#1499, part of #617): the
//! `.mch` file's `edm.tank` (the work submerged in the tank or not) and
//! `edm.flushing` (high or low pressure flushing). Each is an enumerated
//! word with no default: unstated means the operator sets it at the
//! control and the post writes nothing; a garbled word refuses the file
//! by name. Which control word a post writes for each is the post's own
//! (`sodick-wire.cps`: T94 / T89, T84 / T85).
//!
//! Provenance: our own design over #290's D5 (machine facts are data);
//! the two choices are the ones the 1998 Sodick "Wire EDM Machine
//! Operation" manual (Mark 21 / EX21) names in its T-code table
//! (submersed / non-submersed machining, high / low pressure flushing).
//! No vendor machine file or post was read.

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// `edm.tank`: is the work cut submerged in the filled tank?
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Tank {
    Submerged,
    NonSubmerged,
}

/// `edm.flushing`: the flushing pressure the cut runs with.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Flushing {
    High,
    Low,
}

/// The words `edm.tank` takes, as the file says them.
pub const TANK_WORDS: [&str; 2] = ["submerged", "non-submerged"];
/// The words `edm.flushing` takes, as the file says them.
pub const FLUSHING_WORDS: [&str; 2] = ["high", "low"];

impl Tank {
    /// The word, as the file, the post and refusals say it.
    #[must_use]
    pub fn word(self) -> &'static str {
        match self {
            Tank::Submerged => TANK_WORDS[0],
            Tank::NonSubmerged => TANK_WORDS[1],
        }
    }
}

impl Flushing {
    /// The word, as the file, the post and refusals say it.
    #[must_use]
    pub fn word(self) -> &'static str {
        match self {
            Flushing::High => FLUSHING_WORDS[0],
            Flushing::Low => FLUSHING_WORDS[1],
        }
    }
}

/// Read one enumerated key of the `edm` section (`None` when absent).
fn word<T: serde::de::DeserializeOwned>(v: &Value, key: &str, allowed: &[&str]) -> Result<Option<T>, String> {
    match v.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(x) => serde_json::from_value(x.clone())
            .map(Some)
            .map_err(|_| format!("machine file: edm.{key} must be one of {}, not {x}", allowed.join(", "))),
    }
}

/// Read `edm.tank` and `edm.flushing`.
///
/// # Errors
/// A value that is not one of the key's words, named with the key.
pub(super) fn parse_dielectric(v: &Value) -> Result<(Option<Tank>, Option<Flushing>), String> {
    Ok((word(v, "tank", &TANK_WORDS)?, word(v, "flushing", &FLUSHING_WORDS)?))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn tank_and_flushing_words_parse_and_unstated_reads_as_none() {
        let v = json!({ "tank": "non-submerged", "flushing": "high" });
        assert_eq!(parse_dielectric(&v), Ok((Some(Tank::NonSubmerged), Some(Flushing::High))));
        let v = json!({ "tank": "submerged", "flushing": "low" });
        assert_eq!(parse_dielectric(&v), Ok((Some(Tank::Submerged), Some(Flushing::Low))));
        assert_eq!(parse_dielectric(&json!({})), Ok((None, None)));
        assert_eq!(parse_dielectric(&json!({ "tank": null })), Ok((None, None)));
        for t in [Tank::Submerged, Tank::NonSubmerged] {
            assert_eq!(serde_json::to_value(t).ok(), Some(json!(t.word())));
        }
        for f in [Flushing::High, Flushing::Low] {
            assert_eq!(serde_json::to_value(f).ok(), Some(json!(f.word())));
        }
    }

    #[test]
    fn an_unknown_tank_or_flushing_word_refuses_the_file_by_name() {
        let err = parse_dielectric(&json!({ "tank": "flooded" })).err().unwrap_or_default();
        assert_eq!(err, "machine file: edm.tank must be one of submerged, non-submerged, not \"flooded\"");
        let err = parse_dielectric(&json!({ "flushing": "medium" })).err().unwrap_or_default();
        assert_eq!(err, "machine file: edm.flushing must be one of high, low, not \"medium\"");
        let err = parse_dielectric(&json!({ "flushing": true })).err().unwrap_or_default();
        assert!(err.contains("edm.flushing must be one of high, low, not true"), "{err}");
    }
}
