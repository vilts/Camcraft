//! End-of-program unit switching (`postOptions.EndUnits`, #596 / #2189):
//! a post script declares `endUnits = "<inch_word> <mm_word>"` (e.g. `"G20 G21"`
//! or SINUMERIK `"G700 G710"`) when it writes the unit word before program end,
//! or `endUnits = "note"` when its dialect cannot switch units at program end
//! (and the script notes the ask once instead). A script that declares no
//! `endUnits` is refused by name when `EndUnits` is set, rather than silently
//! ignoring the option, and the UI offers the picker only when `words()` returns
//! `Some`.
//!
//! Provenance: RS-274 / ISO 6983 (`G20` / `G21`), SINUMERIK 840D sl
//! Fundamentals (`G700` / `G710`), and this repo's manifest-flag pattern
//! (`units.rs`, `explain.rs`); no vendor template consulted.

use super::js::{is_true, Script};
use super::PostError;
use serde_json::Value;

/// What a script's `endUnits` manifest declares.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EndUnitsSpec {
    /// Writes `(inch, mm)` unit words before program end when `EndUnits` differs.
    Words { inch: String, mm: String },
    /// Cannot switch units at program end; writes a one-time note when asked.
    Note,
}

/// Whether the options ask for an end unit. Absent (or null) leaves the
/// program's unit modal; any other value must be `"inch"` or `"mm"`.
pub fn asks_end_units(options: &Value) -> Result<Option<&str>, PostError> {
    match options.get("EndUnits") {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(s)) if s == "inch" || s == "mm" => Ok(Some(s.as_str())),
        Some(other) => Err(PostError::Refused(format!(
            "the end unit {other} is not one a post writes - say \"inch\" or \"mm\""
        ))),
    }
}

/// Parse the script's `endUnits` manifest declaration.
pub fn parse_manifest(s: &mut Script) -> Result<Option<EndUnitsSpec>, PostError> {
    let bad_type = is_true(
        &s.eval_src("typeof endUnits !== \"undefined\" && typeof endUnits !== \"string\"")?,
    );
    if bad_type {
        let who = super::units::post_name(s)?;
        return Err(PostError::Refused(format!(
            "{who} declares endUnits as a non-string - expected \"note\" or \"<inch_word> <mm_word>\" (e.g. \"G20 G21\")"
        )));
    }
    let Some(raw) = s.string_global("endUnits")? else {
        return Ok(None);
    };
    let trimmed = raw.trim();
    if trimmed == "note" {
        return Ok(Some(EndUnitsSpec::Note));
    }
    let parts: Vec<&str> = trimmed.split_whitespace().collect();
    if let [inch, mm] = parts.as_slice() {
        return Ok(Some(EndUnitsSpec::Words {
            inch: (*inch).to_owned(),
            mm: (*mm).to_owned(),
        }));
    }
    let who = super::units::post_name(s)?;
    Err(PostError::Refused(format!(
        "{who} declares endUnits = \"{trimmed}\" - expected \"note\" or \"<inch_word> <mm_word>\" (e.g. \"G20 G21\")"
    )))
}

/// The `(inch_word, mm_word)` declared by a `.cps` script for UI offer gating,
/// or `None` when the post declares `"note"` or no `endUnits`.
pub fn words(cps: &str) -> Result<Option<(String, String)>, PostError> {
    let mut s = Script::load(cps)?;
    Ok(match parse_manifest(&mut s)? {
        Some(EndUnitsSpec::Words { inch, mm }) => Some((inch, mm)),
        Some(EndUnitsSpec::Note) | None => None,
    })
}

/// Refusal when `EndUnits` is requested on a script that declares no `endUnits`,
/// or `None` when the script may run.
pub fn refusal(s: &mut Script, options: &Value) -> Result<Option<String>, PostError> {
    let Some(want) = asks_end_units(options)? else {
        return Ok(None);
    };
    if parse_manifest(s)?.is_some() {
        return Ok(None);
    }
    let who = super::units::post_name(s)?;
    Ok(Some(format!(
        "End units is set to \"{want}\", but {who} does not declare endUnits (it does not handle EndUnits) - set End units to Leave modal, or pick a post that supports it"
    )))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parses_end_units_option_and_manifest() {
        assert_eq!(asks_end_units(&Value::Null).unwrap_or(Some("err")), None);
        assert_eq!(asks_end_units(&json!({})).unwrap_or(Some("err")), None);
        assert_eq!(asks_end_units(&json!({ "EndUnits": "inch" })).unwrap_or(None), Some("inch"));
        assert_eq!(asks_end_units(&json!({ "EndUnits": "mm" })).unwrap_or(None), Some("mm"));
        assert!(asks_end_units(&json!({ "EndUnits": "furlong" })).is_err());

        assert_eq!(words("function onOpen() {}").expect("none"), None);
        assert_eq!(words("endUnits = \"note\";").expect("note"), None);
        assert_eq!(
            words("endUnits = \"G20 G21\";").expect("rs274"),
            Some(("G20".to_owned(), "G21".to_owned()))
        );
        assert_eq!(
            words("endUnits = \"G700 G710\";").expect("sinumerik"),
            Some(("G700".to_owned(), "G710".to_owned()))
        );
        assert!(words("endUnits = \"G20\";").is_err());
        assert!(words("endUnits = true;").is_err());
    }

    #[test]
    fn refuses_undeclared_script_only_when_end_units_asked() {
        let mut s = Script::load("description = \"Plain Mill (3-axis)\";").expect("loads");
        assert_eq!(refusal(&mut s, &json!({})).expect("absent"), None);
        let why = refusal(&mut s, &json!({ "EndUnits": "inch" }))
            .expect("reads")
            .expect("refused");
        assert!(why.contains("the Plain Mill post") && why.contains("endUnits"), "{why}");
    }
}
