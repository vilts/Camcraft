//! Explain G-code (#1386, design on #1358): an opt-in post option that
//! lets a post append a short plain-English comment to a block, saying
//! what its G / M / T / S / F words do. The app states it as
//! `postOptions.Explain = true` (absent = off). A post that writes the
//! comments declares its comment form in its manifest -
//! `explains = "paren"` (`( )`, RS-274 / ISO 6983) or `"semi"` (`;` to
//! the end of the block) - and may state `commentMax`, the longest
//! comment its control keeps, delimiters included (60 when unstated).
//! Explain on a post that declares no form is refused by name before any
//! hook runs - never a silent program without the comments it was asked
//! for.
//!
//! The comment comes from ONE PRELUDE helper, `explain(words)`, so the
//! wording is written once and the guards cannot be forgotten by a post:
//! the text is cut down to printable ASCII without any comment
//! delimiter, a comment that would open with a keyword some controls
//! execute (LinuxCNC's MSG, DEBUG, PRINT, LOG..., PY...) is led by
//! "- " in every dialect, and the comment is cut to `commentMax` - only
//! the comment, never an executable word, which the post writes before
//! it. With the option off (or before the engine arms it) the helper
//! returns "", so a post that calls it posts byte-identical programs.
//!
//! Provenance: the word meanings are written fresh for this repo from
//! RS-274 / ISO 6983 and the LinuxCNC G-code reference (G and M code
//! lists, "Comments", "Messages"); no vendor post, manual text or other
//! product's output was consulted for the wording.

use super::js::Script;
use super::PostError;
use serde_json::Value;

/// The default longest comment, delimiters included, when the manifest
/// states no `commentMax`.
pub const DEFAULT_COMMENT_MAX: u32 = 60;

/// The smallest `commentMax` a manifest may state: room for the
/// delimiters and a few words, below which no comment says anything.
pub const MIN_COMMENT_MAX: u32 = 8;

/// The PRELUDE part that explains a block. Appended after the core
/// PRELUDE by `Script::load`, so every script sees `explain`.
pub const EXPLAIN_JS: &str = r#"
// Explain G-code (#1386): armed by the engine only when the option is
// on and the manifest states a comment form; null means "say nothing".
var _explainForm = null;
// What each word does, written for this repo (see explain.rs).
var _EXPLAIN_WORDS = {
  G0: "RAPID MOVE", G1: "FEED MOVE", G2: "ARC CLOCKWISE", G3: "ARC COUNTERCLOCKWISE",
  G4: "PAUSE", G7: "LATHE DIAMETER MODE", G8: "LATHE RADIUS MODE",
  G10: "WRITE OFFSET TABLE", G17: "XY PLANE", G18: "XZ PLANE", G19: "YZ PLANE",
  G20: "INCH UNITS", G21: "MM UNITS", G28: "GO TO REFERENCE POINT",
  G30: "GO TO SECOND REFERENCE POINT", G33: "SPINDLE SYNCED MOVE",
  G40: "RADIUS COMP CANCEL", G41: "RADIUS COMP LEFT OF PATH", G42: "RADIUS COMP RIGHT OF PATH",
  G43: "TOOL LENGTH OFFSET ON", G49: "TOOL LENGTH OFFSET CANCEL",
  G53: "MACHINE COORDINATES FOR THIS BLOCK",
  G54: "WORK OFFSET 1", G55: "WORK OFFSET 2", G56: "WORK OFFSET 3",
  G57: "WORK OFFSET 4", G58: "WORK OFFSET 5", G59: "WORK OFFSET 6",
  G61: "STOP EXACTLY AT EACH POINT", G64: "BLEND BETWEEN MOVES",
  G73: "CHIP BREAKING PECK CYCLE", G76: "THREADING CYCLE", G80: "CANCEL DRILL CYCLE",
  G81: "DRILL CYCLE", G82: "DRILL CYCLE, DWELL AT DEPTH", G83: "FULL RETRACT PECK CYCLE",
  G84: "TAPPING CYCLE", G85: "BORE CYCLE, FEED OUT", G86: "BORE CYCLE, SPINDLE STOP, RAPID OUT",
  G89: "BORE CYCLE, DWELL, FEED OUT",
  G90: "ABSOLUTE POSITIONS", G91: "INCREMENTAL POSITIONS",
  "G90.1": "ARC CENTERS ABSOLUTE", "G91.1": "ARC CENTERS FROM START POINT",
  G93: "INVERSE TIME FEED", G94: "FEED PER MINUTE", G95: "FEED PER REVOLUTION",
  G96: "CONSTANT SURFACE SPEED", G97: "SPINDLE RPM MODE",
  G98: "CYCLE RETURNS TO START HEIGHT", G99: "CYCLE RETURNS TO R HEIGHT",
  M0: "PROGRAM STOP", M1: "OPTIONAL STOP", M2: "PROGRAM END",
  M3: "SPINDLE ON CLOCKWISE", M4: "SPINDLE ON COUNTERCLOCKWISE", M5: "SPINDLE STOP",
  M6: "TOOL CHANGE", M7: "MIST COOLANT ON", M8: "FLOOD COOLANT ON", M9: "COOLANT OFF",
  M30: "PROGRAM END, BACK TO START"
};
// Words a control may execute when a comment opens with them (LinuxCNC
// "Comments" / "Messages" / "Logging"; #594's list), any case.
var _EXPLAIN_ACTIVE = /^\s*(MSG|DEBUG|PRINT|LOG|LOGOPEN|LOGAPPEND|LOGCLOSE|PROBEOPEN|PROBECLOSE|ABORT|AXIS|PY|PYRUN)\b/i;
// A post's own meanings (#1399). The table above is the mill reading;
// a control that reads a word differently says so in its manifest:
//   explainWords = { G99: "FEED PER REVOLUTION", G7: "" , G50: { said: "SPINDLE SPEED LIMIT", takes: "S" } }
// a string is the word's meaning on that control ("" leaves it
// unexplained); { said, takes } is a word whose meaning needs another
// word of the block - explained only beside that word, whose value it
// folds in (that word is then not explained on its own), silent without
// it. explainTool = "station-offset" reads a T word as a two-digit
// offset number after the tool number (T0101: tool 1, offset 1), the
// lathe T word; any other T word on such a post is left unexplained.
function _explainOwn(key) {
  if (typeof explainWords !== "object" || explainWords === null) return undefined;
  return Object.prototype.hasOwnProperty.call(explainWords, key) ? explainWords[key] : undefined;
}
function _explainStationOffset() {
  return typeof explainTool !== "undefined" && explainTool === "station-offset";
}
// The reason a post's own meanings cannot be used, or "" when they can
// (read by the engine before any hook runs, so a malformed table is
// refused by name, never half-applied).
function _explainCheck() {
  if (typeof explainTool !== "undefined" && explainTool !== "station-offset")
    return "explainTool = " + JSON.stringify(explainTool) + " - the only T word reading a post may state is \"station-offset\"";
  if (typeof explainWords === "undefined") return "";
  if (typeof explainWords !== "object" || explainWords === null || Array.isArray(explainWords))
    return "explainWords is not a table of words and their meanings";
  for (var k in explainWords) {
    if (!Object.prototype.hasOwnProperty.call(explainWords, k)) continue;
    var w = /^([GM])([0-9]+(?:\.[0-9]+)?)$/.exec(k);
    if (!w || (w[1] + String(Number(w[2]))) !== k)
      return "explainWords names \"" + k + "\" - only G and M words without leading zeros (G99, M8) are explained";
    var v = explainWords[k];
    if (typeof v === "string") continue;
    if (v !== null && typeof v === "object" && typeof v.said === "string" && v.said !== ""
        && typeof v.takes === "string" && /^[A-Z]$/.test(v.takes) && v.takes !== "G" && v.takes !== "M") continue;
    return "explainWords gives " + k + " a meaning that is neither a text nor { said, takes } with one value letter";
  }
  return "";
}
// One word's meaning, or "" when the table has none (never a guess).
// `vals` holds the block's value words by letter, for { said, takes }.
function _explainWord(w, vals) {
  var m = /^([A-Za-z])([-+]?(?:[0-9]+\.?[0-9]*|\.[0-9]+))$/.exec(String(w));
  if (!m) return "";
  var letter = m[1].toUpperCase(), n = Number(m[2]);
  if (!isFinite(n)) return "";
  var num = String(n);
  if (letter === "G" || letter === "M") {
    var own = _explainOwn(letter + num);
    if (typeof own === "string") return own;
    if (own !== undefined && own !== null) {
      var v = vals ? vals[own.takes] : undefined;
      return v === undefined ? "" : own.said + " " + v;
    }
    var said = _EXPLAIN_WORDS[letter + num];
    return said === undefined ? "" : said;
  }
  if (letter === "T") {
    if (!_explainStationOffset()) return "TOOL " + num;
    var d = /^([0-9]{1,2})([0-9]{2})$/.exec(m[2]);
    if (!d) return "";
    var off = Number(d[2]);
    return "TOOL " + Number(d[1]) + (off === 0 ? ", NO OFFSET" : ", OFFSET " + off);
  }
  if (letter === "S") return "SPEED " + num;
  if (letter === "F") return "FEED RATE " + num;
  return "";
}
// The trailing comment for a block's words (an array, or the words as
// arguments; a string may hold several words), in the post's declared
// form - "" when the option is off, the block carries no G / M / T / S
// word, or nothing in it is known. The post writes it AFTER its words.
function explain() {
  if (_explainForm === null) return "";
  var list = (arguments.length === 1 && Array.isArray(arguments[0])) ? arguments[0] : arguments;
  var tokens = [];
  for (var i = 0; i < list.length; i++) {
    var a = list[i];
    if (a === undefined || a === null) continue;
    var parts = String(a).split(/\s+/);
    for (var j = 0; j < parts.length; j++) if (parts[j] !== "") tokens.push(parts[j]);
  }
  // The words before any comment, and the block's value words by letter.
  var words = [], vals = {};
  for (var k = 0; k < tokens.length; k++) {
    var c = tokens[k].charAt(0);
    if (c === "(" || c === ";") break;  // the rest is already a comment
    words.push(tokens[k]);
    var vm = /^([A-Za-z])([-+]?(?:[0-9]+\.?[0-9]*|\.[0-9]+))$/.exec(tokens[k]);
    var L = vm ? vm[1].toUpperCase() : "";
    if (vm && L !== "G" && L !== "M" && vals[L] === undefined && isFinite(Number(vm[2]))) vals[L] = String(Number(vm[2]));
  }
  // A { said, takes } word present with its value word folds that word in.
  var folded = {};
  for (var f = 0; f < words.length; f++) {
    var gm = /^([GMgm])([0-9]+(?:\.[0-9]*)?)$/.exec(words[f]);
    if (!gm) continue;
    var own = _explainOwn(gm[1].toUpperCase() + String(Number(gm[2])));
    if (own !== undefined && own !== null && typeof own === "object" && vals[own.takes] !== undefined) folded[own.takes] = true;
  }
  var anchored = false, said = [];
  for (var q = 0; q < words.length; q++) {
    var t = words[q];
    if (/^[GMTSgmts]/.test(t)) anchored = true;
    if (folded[t.charAt(0).toUpperCase()]) continue;
    var s = _explainWord(t, vals);
    if (s !== "") said.push(s);
  }
  if (!anchored || said.length === 0) return "";
  return _explainComment(said.join(", "));
}
// The engine's guards on any explanation text: printable ASCII with no
// comment delimiter, the active-keyword lead, the length cap.
function _explainComment(text) {
  var t = String(text).replace(/[^\x20-\x7E]/g, " ").replace(/[()%;]/g, " ").replace(/\s+/g, " ").trim();
  if (t === "") return "";
  if (_EXPLAIN_ACTIVE.test(t)) t = "- " + t;
  var open = _explainForm.semi ? "; " : "(", close = _explainForm.semi ? "" : ")";
  var room = _explainForm.max - open.length - close.length;
  if (room < 1) return "";
  if (t.length > room) {
    // Whole meanings where one fits: cut back to the last ", " unless
    // the cap falls right on one; a single long meaning is cut short.
    var cut = t.substring(0, room);
    if (t.charAt(room) !== ",") { var b = cut.lastIndexOf(", "); if (b > 0) cut = cut.substring(0, b); }
    t = cut.replace(/[\s,]+$/, "");
  }
  if (t === "") return "";
  return open + t + close;
}
"#;

/// Whether the options ask for explained blocks. Absent (or no options
/// object at all) is off - every program posted before #1386; anything
/// but a boolean is refused by name, never guessed.
pub fn asks_explain(options: &Value) -> Result<bool, PostError> {
    match options.get("Explain") {
        None | Some(Value::Null) => Ok(false),
        Some(Value::Bool(b)) => Ok(*b),
        Some(other) => Err(PostError::Refused(format!(
            "the Explain G-code option {other} is not a yes / no - say true or false"
        ))),
    }
}

/// The manifest's comment form, as the engine arms it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Form {
    /// `;` to the end of the block, else `( )`.
    pub semi: bool,
    /// The longest comment, delimiters included.
    pub max: u32,
}

/// The form a script declares, the refusal naming why it cannot explain
/// (`Err(Refused)`), or `None` when the option is off. Read before any
/// hook runs.
pub fn form(s: &mut Script, options: &Value) -> Result<Option<Form>, PostError> {
    if !asks_explain(options)? {
        return Ok(None);
    }
    let who = super::units::post_name(s)?;
    let semi = match s.string_global("explains")?.as_deref() {
        Some("paren") => false,
        Some("semi") => true,
        Some(other) => {
            return Err(PostError::Refused(format!(
                "Explain G-code is on, but {who} declares explains = \"{other}\" - a post explains with \"paren\" or \"semi\" comments"
            )))
        }
        None => {
            return Err(PostError::Refused(format!(
                "Explain G-code is on, but {who} does not declare explains = \"paren\" or \"semi\" (it writes no explanatory comments) - turn Explain G-code off, or pick a post that explains its blocks"
            )))
        }
    };
    let max = match s.number_global("commentMax")? {
        None => DEFAULT_COMMENT_MAX,
        Some(n) if n.is_finite() && n.fract() == 0.0 && n >= f64::from(MIN_COMMENT_MAX) && n <= 255.0 => n as u32,
        Some(n) => {
            return Err(PostError::Refused(format!(
                "Explain G-code is on, but {who} states commentMax = {n} - it must be a whole number from {MIN_COMMENT_MAX} to 255"
            )))
        }
    };
    // The post's own meanings (#1399): a malformed table is refused by
    // name before any hook, never half-applied.
    if let Some(why) = s.explain_refusal()? {
        return Err(PostError::Refused(format!("Explain G-code is on, but {who} states {why}")));
    }
    Ok(Some(Form { semi, max }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn the_option_reads_a_boolean_or_refuses() {
        assert!(!asks_explain(&Value::Null).unwrap_or(true));
        assert!(!asks_explain(&json!({})).unwrap_or(true));
        assert!(!asks_explain(&json!({ "Explain": false })).unwrap_or(true));
        assert!(asks_explain(&json!({ "Explain": true })).unwrap_or(false));
        assert!(asks_explain(&json!({ "Explain": "yes" })).is_err());
    }

    #[test]
    fn the_manifest_states_the_form_or_is_refused_by_name() {
        let on = json!({ "Explain": true });
        let mut none = Script::load("description = \"Plain Mill (3-axis)\";").expect("loads");
        match form(&mut none, &on) {
            Err(PostError::Refused(why)) => assert!(why.contains("the Plain Mill post does not declare explains"), "{why}"),
            other => panic!("expected a refusal, got {other:?}"),
        }
        let mut odd = Script::load("explains = \"brace\";").expect("loads");
        assert!(matches!(form(&mut odd, &on), Err(PostError::Refused(_))));
        let mut tiny = Script::load("explains = \"paren\"; commentMax = 3;").expect("loads");
        assert!(matches!(form(&mut tiny, &on), Err(PostError::Refused(_))));
        let mut paren = Script::load("explains = \"paren\";").expect("loads");
        assert_eq!(form(&mut paren, &on).expect("reads"), Some(Form { semi: false, max: DEFAULT_COMMENT_MAX }));
        let mut semi = Script::load("explains = \"semi\"; commentMax = 32;").expect("loads");
        assert_eq!(form(&mut semi, &on).expect("reads"), Some(Form { semi: true, max: 32 }));
        // Off never reads the manifest.
        assert_eq!(form(&mut none, &json!({})).expect("reads"), None);
    }
}
