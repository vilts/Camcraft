//! Machine-file validation (issue #287): the named refusals a `.mch`
//! must clear before the Machines pane calls it good - and, in a later
//! slice, before the editor may save it.
//!
//! Provenance: our own rules, specced from the `.mch` grammar in
//! `assets/machines/AGENTS.md` and general machine-tool practice (a
//! stroke runs from a lower to a higher bound, a feed or a spindle speed
//! is a positive rate, a coolant word is an RS-274 `M` number); no
//! vendor validator or file was consulted.
//!
//! The reader (`parse_mch`) is deliberately forgiving: an unreadable
//! travel is DROPPED, a zero rate reads "not stated". That is right for
//! a checker - and wrong for a file a person is about to trust, where a
//! stroke typed backwards should be named, not silently ignored. So the
//! validator reads the raw JSON, and every refusal names the key it is
//! about and the rule it breaks. Absent stays legal (the unstated-limit
//! law): only what a file STATES is judged, plus the few keys every
//! machine of its kind must carry.

use std::fmt;

use serde_json::Value;

use super::{kind, parse_mch, Kind};

/// One named refusal: the key it is about and the rule it breaks.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Refusal {
    /// The key, in words a reader can find in the file (`X travel`,
    /// `general.kind`, `coolant Flood`).
    pub key: String,
    /// The rule, with the offending value where there is one.
    pub rule: String,
}

impl Refusal {
    fn new(key: impl Into<String>, rule: impl Into<String>) -> Self {
        Self { key: key.into(), rule: rule.into() }
    }
}

impl fmt::Display for Refusal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.key, self.rule)
    }
}

/// An axis part of the kinematics tree, as the file states it: its
/// letter, its JSON, and whether it is a rotary. Depth-first, the walk
/// `parse_mch` makes - the pane's property sheet reads the same list.
pub(crate) fn axis_parts(v: &Value) -> Vec<(String, &Value, bool)> {
    fn walk<'a>(parts: Option<&'a Value>, out: &mut Vec<(String, &'a Value, bool)>) {
        let Some(arr) = parts.and_then(Value::as_array) else { return };
        for p in arr {
            let ty = p.get("type").and_then(Value::as_str);
            let rotary = ty == Some("rotary");
            let name = p
                .get("name")
                .and_then(Value::as_str)
                .or_else(|| p.get("id").and_then(Value::as_str))
                .unwrap_or("");
            if (ty == Some("linear") || rotary) && !name.is_empty() {
                out.push((name.to_owned(), p, rotary));
            }
            walk(p.get("parts"), out);
        }
    }
    let mut out = Vec::new();
    walk(v.pointer("/kinematics/default/parts"), &mut out);
    out
}

/// Every `type: "head"` part, depth-first.
pub(crate) fn head_parts(v: &Value) -> Vec<&Value> {
    fn walk<'a>(parts: Option<&'a Value>, out: &mut Vec<&'a Value>) {
        let Some(arr) = parts.and_then(Value::as_array) else { return };
        for p in arr {
            if p.get("type").and_then(Value::as_str) == Some("head") {
                out.push(p);
            }
            walk(p.get("parts"), out);
        }
    }
    let mut out = Vec::new();
    walk(v.pointer("/kinematics/default/parts"), &mut out);
    out
}

/// Validate a machine file's text. Empty = the file is good. Never
/// panics: malformed JSON is itself a refusal by name.
#[must_use]
pub fn validate_text(text: &str) -> Vec<Refusal> {
    let v: Value = match serde_json::from_str(text) {
        Ok(v) => v,
        Err(e) => return vec![Refusal::new("file", format!("not readable JSON ({e})"))],
    };
    if !v.is_object() {
        return vec![Refusal::new("file", "must be one machine object { ... }")];
    }
    let mut out = Vec::new();
    general(&v, &mut out);
    units(&v, &mut out);
    let axes = axis_parts(&v);
    axis_rules(&axes, &mut out);
    for head in head_parts(&v) {
        spindle(head.get("spindle"), &mut out);
        station(head.get("tool_station"), &mut out);
    }
    coolants(v.pointer("/tooling/coolants"), &mut out);
    controller(&v, &mut out);
    if let Some(n) = v.pointer("/tooling/number_of_tools")
        && !n.is_null()
        && n.as_u64().is_none_or(|n| n == 0)
    {
        out.push(Refusal::new("tooling.number_of_tools", format!("{n} must be a whole number above 0")));
    }
    // #828: the spindle mount and default holder, each refusal by name.
    out.extend(super::tooling_refusals(&v).into_iter().map(|(key, rule)| Refusal::new(key, rule)));
    required_by_kind(&v, text, &axes, &mut out);
    // Anything the reader still refuses that no rule above named (it
    // should be none - the reader's refusals are covered by name).
    if out.is_empty()
        && let Err(e) = parse_mch(text)
    {
        out.push(Refusal::new("file", e.trim_start_matches("machine file: ").to_owned()));
    }
    out
}

fn general(v: &Value, out: &mut Vec<Refusal>) {
    let g = v.get("general");
    let model = g.and_then(|g| g.get("model")).and_then(Value::as_str);
    if model.is_none_or(|m| m.trim().is_empty()) {
        out.push(Refusal::new("general.model", "required - the machine's name"));
    }
    match g.and_then(|g| g.get("kind")) {
        None | Some(Value::Null) => {}
        Some(Value::String(w)) if w.is_empty() || Kind::from_word(w).is_some() => {}
        Some(other) => out.push(Refusal::new("general.kind", format!("{other} is not mill, lathe, mill-turn or plasma"))),
    }
}

fn units(v: &Value, out: &mut Vec<Refusal>) {
    let u = v.pointer("/kinematics/default/units");
    match u.and_then(|u| u.get("length")) {
        None | Some(Value::Null) => {}
        Some(Value::String(s)) if matches!(s.as_str(), "mm" | "inch" | "inches") => {}
        Some(other) => out.push(Refusal::new("units.length", format!("{other} is not mm or inch"))),
    }
    match u.and_then(|u| u.get("angle")) {
        None | Some(Value::Null) => {}
        Some(Value::String(s)) if matches!(s.as_str(), "degrees" | "degree" | "deg") => {}
        Some(other) => out.push(Refusal::new("units.angle", format!("{other} is not degrees"))),
    }
}

fn axis_rules(axes: &[(String, &Value, bool)], out: &mut Vec<Refusal>) {
    let mut seen: Vec<String> = Vec::new();
    for (name, part, rotary) in axes {
        let up = name.to_ascii_uppercase();
        if seen.contains(&up) {
            out.push(Refusal::new(name.clone(), "is stated twice in the axis tree"));
        }
        seen.push(up);
        if let Some(t) = part.get("travel").filter(|t| !t.is_null()) {
            travel(&format!("{name} travel"), t, out);
        }
        if let Some(r) = part.get("resolution") {
            positive(&format!("{name} resolution"), r, out);
        }
        if let Some(a) = part.get("axis") {
            let d: Option<Vec<f64>> = a
                .as_array()
                .filter(|v| v.len() == 3)
                .and_then(|v| v.iter().map(Value::as_f64).collect());
            match d {
                Some(d) if d.iter().all(|x| x.is_finite()) => {
                    if *rotary && d.iter().all(|x| *x == 0.0) {
                        out.push(Refusal::new(format!("{name} axis"), "states the zero direction"));
                    }
                }
                _ => out.push(Refusal::new(format!("{name} axis"), format!("{a} must be three finite numbers"))),
            }
        }
    }
}

fn travel(key: &str, t: &Value, out: &mut Vec<Refusal>) {
    let lo = t.get("min").and_then(Value::as_f64).filter(|x| x.is_finite());
    let hi = t.get("max").and_then(Value::as_f64).filter(|x| x.is_finite());
    match (lo, hi) {
        (Some(lo), Some(hi)) if lo < hi => {}
        (Some(lo), Some(hi)) => out.push(Refusal::new(key, format!("min {lo} must be below max {hi}"))),
        _ => out.push(Refusal::new(key, "min and max must both be numbers")),
    }
}

/// A stated rate must be a number above zero. A null reads unstated
/// (legal); a zero is refused here even though the reader would read it
/// as unstated - a file someone validates should say what it means.
fn positive(key: &str, x: &Value, out: &mut Vec<Refusal>) {
    if x.is_null() {
        return;
    }
    if !x.as_f64().is_some_and(|x| x.is_finite() && x > 0.0) {
        out.push(Refusal::new(key, format!("{x} must be a number above 0 (leave the key out when unstated)")));
    }
}

fn spindle(sp: Option<&Value>, out: &mut Vec<Refusal>) {
    let Some(sp) = sp.filter(|s| !s.is_null()) else { return };
    for k in ["min_speed", "max_speed", "power"] {
        if let Some(x) = sp.get(k) {
            positive(&format!("spindle {k}"), x, out);
        }
    }
    if let Some(x) = sp.get("rigid_tapping").filter(|x| !x.is_null() && !x.is_boolean()) {
        out.push(Refusal::new("spindle.rigid_tapping", format!("{x} must be true or false")));
    }
    let lo = sp.get("min_speed").and_then(Value::as_f64);
    let hi = sp.get("max_speed").and_then(Value::as_f64);
    if let (Some(lo), Some(hi)) = (lo, hi)
        && lo > hi
    {
        out.push(Refusal::new("spindle", format!("min_speed {lo} must not exceed max_speed {hi}")));
    }
    if let Some(ranges) = sp.get("ranges").filter(|r| !r.is_null()) {
        let Some(arr) = ranges.as_array() else {
            out.push(Refusal::new("spindle ranges", "must be a list of { min_speed, max_speed }"));
            return;
        };
        for (i, r) in arr.iter().enumerate() {
            let key = format!("spindle range {}", i + 1);
            let lo = r.get("min_speed").and_then(Value::as_f64).filter(|x| x.is_finite() && *x > 0.0);
            let hi = r.get("max_speed").and_then(Value::as_f64).filter(|x| x.is_finite() && *x > 0.0);
            match (lo, hi) {
                (Some(lo), Some(hi)) if lo < hi => {}
                (Some(lo), Some(hi)) => out.push(Refusal::new(key, format!("min_speed {lo} must be below max_speed {hi}"))),
                _ => out.push(Refusal::new(key, "min_speed and max_speed must both be numbers above 0")),
            }
        }
    }
}

fn station(ts: Option<&Value>, out: &mut Vec<Refusal>) {
    let Some(ts) = ts.filter(|t| !t.is_null()) else { return };
    coolants(ts.get("coolants"), out);
    if let Some(lt) = ts.get("live_tool").filter(|l| !l.is_null()) {
        let words = ["c_axis_on", "c_axis_off", "spindle_on", "spindle_off"];
        let stated = words
            .iter()
            .filter(|k| lt.get(**k).and_then(Value::as_str).is_some_and(|w| !w.trim().is_empty()))
            .count();
        if stated != words.len() {
            out.push(Refusal::new(
                "live_tool",
                format!("states {stated} of the 4 words (c_axis_on, c_axis_off, spindle_on, spindle_off) - all four or none"),
            ));
        }
        // Each stated word is written as a block of its own: it must be
        // a well-formed M word (an unstated one is the rule above's).
        for k in words {
            if let Some(w) = lt.get(k).and_then(Value::as_str).filter(|w| !w.trim().is_empty())
                && let Err(why) = m_word(w.trim())
            {
                out.push(Refusal::new(format!("live_tool.{k}"), format!("code \"{w}\" is not an M word like M8 ({why})")));
            }
        }
    }
}

/// The highest M number a machine file may state. RS-274/NGC (NIST
/// IR 6556) and ISO 6983-1 give M a two-digit standard range; controls
/// extend it (LinuxCNC user codes M100-M199, three-digit builder codes),
/// so four digits is a sanity bound, not a standard's ceiling - see
/// `docs/decisions/383-machines-edit.md`.
pub const M_WORD_MAX: u32 = 9999;

/// Is `s` an M word: an uppercase `M` and a whole number
/// 0..=[`M_WORD_MAX`], leading zeros allowed (`M8`, `M08`, `M155`)?
/// `Err` says what is wrong with it. One word only - a post writes the
/// statement as it stands, so `M8 M7` or `M8.5` would reach the program.
fn m_word(s: &str) -> Result<(), &'static str> {
    let rest = match s.strip_prefix('M') {
        Some(r) => r,
        None if s.starts_with('m') => return Err("write it with an uppercase M"),
        None => return Err("it must start with M"),
    };
    if rest.is_empty() {
        return Err("no number after M");
    }
    if !rest.bytes().all(|b| b.is_ascii_digit()) {
        return Err("only digits may follow M");
    }
    let digits = rest.trim_start_matches('0');
    match digits.parse::<u32>() {
        _ if digits.is_empty() => Ok(()),
        Ok(n) if digits.len() <= 4 && n <= M_WORD_MAX => Ok(()),
        _ => Err("above M9999"),
    }
}

fn coolants(list: Option<&Value>, out: &mut Vec<Refusal>) {
    let Some(list) = list.filter(|l| !l.is_null()) else { return };
    let Some(arr) = list.as_array() else {
        out.push(Refusal::new("coolants", "must be a list"));
        return;
    };
    for (i, e) in arr.iter().enumerate() {
        if let Some(name) = e.as_str() {
            if name.trim().is_empty() {
                out.push(Refusal::new(format!("coolant {}", i + 1), "has an empty name"));
            }
            continue;
        }
        let name = e.get("name").and_then(Value::as_str).unwrap_or("");
        if name.trim().is_empty() {
            out.push(Refusal::new(format!("coolant {}", i + 1), "needs a name"));
            continue;
        }
        let key = format!("coolant {name}");
        match e.get("mCode").or_else(|| e.get("mcode")) {
            None | Some(Value::Null) => {}
            Some(Value::Number(n)) if n.as_u64().is_some_and(|n| n <= u64::from(M_WORD_MAX)) => {}
            Some(Value::String(s)) => {
                if let Err(why) = m_word(s.trim()) {
                    out.push(Refusal::new(key, format!("code \"{s}\" is not an M word like M8 ({why})")));
                }
            }
            Some(other) => out.push(Refusal::new(key, format!("code {other} is not an M word like M8"))),
        }
    }
}

fn controller(v: &Value, out: &mut Vec<Refusal>) {
    let Some(parts) = v.pointer("/controller/synced_configuration/parts").filter(|p| !p.is_null()) else {
        return;
    };
    let Some(map) = parts.as_object() else {
        out.push(Refusal::new("controller parts", "must be keyed by axis letter"));
        return;
    };
    for (letter, c) in map {
        for (k, word) in [("max_normal_speed", "feed"), ("max_rapid_speed", "rapid")] {
            if let Some(x) = c.get(k) {
                positive(&format!("{letter} {word} ({k})"), x, out);
            }
        }
    }
}

/// The keys every machine of its kind must carry: the kinematics tree,
/// and the linear axes the kind moves - X/Y/Z for a mill, X/Z for a
/// lathe, X/Y/Z and a rotary for a mill-turn, X/Y/Z and the plasma
/// rules (no spindle, coolant or rotary) for a plasma table. The kind is the declared
/// word, else the reader's axis-inventory inference.
fn required_by_kind(v: &Value, text: &str, axes: &[(String, &Value, bool)], out: &mut Vec<Refusal>) {
    if !v.pointer("/kinematics/default/parts").is_some_and(Value::is_array) {
        out.push(Refusal::new("kinematics.default.parts", "required - the machine's axis tree"));
        return;
    }
    let declared = v
        .pointer("/general/kind")
        .and_then(Value::as_str)
        .and_then(Kind::from_word);
    let Some(k) = declared.or_else(|| parse_mch(text).ok().map(|m| kind(Some(&m)))) else {
        return;
    };
    let has_linear = |l: &str| axes.iter().any(|(n, _, r)| !r && n.eq_ignore_ascii_case(l));
    let need: &[&str] = match k {
        Kind::Lathe => &["X", "Z"],
        // #669: a plasma table's Z is the torch lifter - required, as
        // the axis the control raises and probes with.
        Kind::Mill | Kind::MillTurn | Kind::Plasma => &["X", "Y", "Z"],
        // #530: a wire cuts in X/Y (U/V add taper); a sinker burns in Z.
        Kind::WireEdm => &["X", "Y"],
        Kind::SinkerEdm => &["Z"],
    };
    for l in need {
        if !has_linear(l) {
            out.push(Refusal::new(format!("{l} axis"), format!("required for a {} machine", k.word())));
        }
    }
    // #669: the plasma rules, the same list the reader refuses on.
    if k == Kind::Plasma {
        out.extend(super::plasma_refusals(v).into_iter().map(|(key, rule)| Refusal::new(key, rule)));
    }
    // #530: an EDM's head states no spindle, live tool or coolant.
    if matches!(k, Kind::WireEdm | Kind::SinkerEdm) {
        out.extend(super::edm_refusals(v).into_iter().map(|(key, rule)| Refusal::new(key, rule)));
    }
    if k == Kind::MillTurn && !axes.iter().any(|(_, _, r)| *r) {
        out.push(Refusal::new("rotary axis", "required for a mill-turn machine (the turning spindle, C)"));
    }
}
