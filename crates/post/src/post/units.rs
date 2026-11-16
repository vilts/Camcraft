//! The program's unit (#902): the document says mm or inch, the post
//! writes it. The stream always speaks mm and mm/min; the app states
//! the document's unit as `postOptions.Units` (`"mm"` / `"inch"`,
//! absent = mm), and a script that converts its lengths and feeds and
//! writes its dialect's unit word declares which units it can write
//! in its manifest: `programUnits = "mm inch"`. An inch document on a
//! script that does not list `inch` is refused by name before any
//! hook runs - never mm numbers under an inch program, never a silent
//! mm program for an inch document.
//!
//! Provenance: RS-274 / ISO 6983 (G20 inch / G21 mm), the international
//! inch (exactly 25.4 mm); the manifest-flag refusal pattern is this
//! engine's own (`coolantModes`, `afterModes`). No vendor material.

use super::js::Script;
use super::PostError;
use serde_json::Value;

/// Whether the options ask for an inch program. Absent (or no options
/// object at all) is mm - what every stream posted before #902; any
/// other word is a refusal by name, never a guess.
pub fn asks_inch(options: &Value) -> Result<bool, PostError> {
    match options.get("Units") {
        None | Some(Value::Null) => Ok(false),
        Some(Value::String(s)) if s == "mm" => Ok(false),
        Some(Value::String(s)) if s == "inch" => Ok(true),
        Some(other) => Err(PostError::Refused(format!(
            "the program unit {other} is not one a post writes - say \"mm\" or \"inch\""
        ))),
    }
}

/// The refusal for an inch program on a script that does not write
/// one, or `None` when the script may run.
pub fn refusal(s: &mut Script, options: &Value) -> Result<Option<String>, PostError> {
    if !asks_inch(options)? {
        return Ok(None);
    }
    let stated = s.string_global("programUnits")?;
    let writes_inch = stated
        .as_deref()
        .is_some_and(|u| u.split_whitespace().any(|w| w == "inch"));
    if writes_inch {
        return Ok(None);
    }
    let who = post_name(s)?;
    let what = match stated.as_deref() {
        Some(u) => format!("writes {} programs only (programUnits = \"{u}\")", u.trim()),
        None => "does not declare programUnits = \"mm inch\" (it does not write inch programs)".to_owned(),
    };
    Ok(Some(format!(
        "the document's unit is inch, but {who} {what} - set the document's units to mm, or pick a post that writes inch"
    )))
}

/// The post by the name its manifest's description gives (the text
/// before its parenthesised summary), so a refusal says whose it is:
/// "the Tormach PathPilot post", or "this post" when it states none.
pub fn post_name(s: &mut Script) -> Result<String, PostError> {
    Ok(s
        .string_global("description")?
        .and_then(|d| d.split(" (").next().map(|n| n.trim().to_owned()))
        .filter(|n| !n.is_empty())
        .map_or_else(|| "this post".to_owned(), |n| format!("the {n} post")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn the_unit_word_reads_mm_inch_or_refuses() {
        assert!(!asks_inch(&Value::Null).unwrap_or(true));
        assert!(!asks_inch(&json!({})).unwrap_or(true));
        assert!(!asks_inch(&json!({ "Units": "mm" })).unwrap_or(true));
        assert!(asks_inch(&json!({ "Units": "inch" })).unwrap_or(false));
        assert!(asks_inch(&json!({ "Units": "in" })).is_err());
        assert!(asks_inch(&json!({ "Units": 25.4 })).is_err());
    }

    #[test]
    fn an_inch_ask_needs_the_manifest_word() {
        let inch = json!({ "Units": "inch" });
        let mut none = Script::load("function onOpen() {}").expect("loads");
        let why = refusal(&mut none, &inch).expect("reads").expect("refused");
        assert!(why.contains("does not declare programUnits"), "{why}");
        let mut mm = Script::load("programUnits = \"mm\"; description = \"Makera Carvera (3-axis mill)\";").expect("loads");
        let why = refusal(&mut mm, &inch).expect("reads").expect("refused");
        assert!(why.contains("the Makera Carvera post writes mm programs only"), "{why}");
        let mut both = Script::load("programUnits = \"mm inch\";").expect("loads");
        assert_eq!(refusal(&mut both, &inch).expect("reads"), None);
        // An mm (or unstated) document never asks the manifest.
        assert_eq!(refusal(&mut none, &json!({ "Units": "mm" })).expect("reads"), None);
        assert_eq!(refusal(&mut none, &Value::Null).expect("reads"), None);
    }
}
