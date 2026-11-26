//! The machine editor's property grid (issue #383): a `.mch` as
//! collapsible groups of editable fields, each bound to one JSON key,
//! and the edit that writes a typed value back into the file's JSON.
//!
//! Provenance: our own layout, specced from the issue's group list and
//! the `.mch` grammar in `assets/machines/AGENTS.md`; group names, field
//! words and tooltips are ours.
//!
//! The grid edits the file's own JSON in place (a JSON pointer per
//! field), so every key the grid does not show - a vendor's extra
//! block, a hand-written note - is carried through untouched. An empty
//! field REMOVES its key: unstated is the file's way of saying "no
//! limit" (the unstated-limit law), never a zero.

use serde_json::{Map, Number, Value};

use super::validate::Refusal;
use super::Kind;

/// How a field's text is written back.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FieldKind {
    /// A number; text that does not read as one is written as text, so
    /// the validator names it rather than the editor eating it.
    Number,
    /// Free text (a model, a unit word, an M-code word).
    Text,
}

/// One editable field.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Field {
    /// The row's words, unique within a machine (`X travel min`).
    pub label: String,
    /// The key in the file's own words, for the tooltip
    /// (`kinematics ... X.travel.min`).
    pub key: String,
    /// Where the value lives: a JSON pointer into the file.
    pub pointer: String,
    pub kind: FieldKind,
    /// The validator keys whose refusals belong on this field.
    pub refusal_keys: Vec<String>,
    /// The value as the file states it; empty = unstated.
    pub value: String,
    /// #2374: the values a dropdown offers; empty = typed text. An empty
    /// pick leaves the key unstated, like an empty text.
    pub choices: Vec<String>,
}

/// One collapsible group of fields.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FieldGroup {
    pub title: String,
    pub fields: Vec<Field>,
}

impl Field {
    /// The refusals that belong on this field.
    #[must_use]
    pub fn refusals<'a>(&self, all: &'a [Refusal]) -> Vec<&'a Refusal> {
        all.iter().filter(|r| self.refusal_keys.contains(&r.key)).collect()
    }
}

/// The refusals no field carries (a missing axis, a malformed file):
/// the editor shows them above the grid.
#[must_use]
pub fn unplaced<'a>(groups: &[FieldGroup], all: &'a [Refusal]) -> Vec<&'a Refusal> {
    all.iter()
        .filter(|r| !groups.iter().flat_map(|g| &g.fields).any(|f| f.refusal_keys.contains(&r.key)))
        .collect()
}

fn text_of(v: Option<&Value>) -> String {
    match v {
        None | Some(Value::Null) => String::new(),
        Some(Value::String(s)) => s.clone(),
        Some(Value::Number(n)) => n.as_f64().map_or_else(|| n.to_string(), |x| format!("{x}")),
        Some(other) => other.to_string(),
    }
}

/// Escape one JSON-pointer token (RFC 6901).
fn tok(s: &str) -> String {
    s.replace('~', "~0").replace('/', "~1")
}

struct Grid<'a> {
    raw: &'a Value,
    group: FieldGroup,
}

impl<'a> Grid<'a> {
    fn new(raw: &'a Value, title: &str) -> Self {
        Self { raw, group: FieldGroup { title: title.to_owned(), fields: Vec::new() } }
    }

    fn add(&mut self, label: String, key: String, pointer: String, kind: FieldKind, refusal_keys: &[String]) {
        let value = text_of(self.raw.pointer(&pointer));
        self.group.fields.push(Field { label, key, pointer, kind, refusal_keys: refusal_keys.to_vec(), value, choices: Vec::new() });
    }
}

/// #2374: the built-in posts that serve the file's kind (every one while
/// the kind cannot be read), and the stated name when it is off that
/// list, so the editor shows it with its refusal rather than hiding it.
fn post_choices(raw: &Value, stated: &str) -> Vec<String> {
    let declared = raw.pointer("/general/kind").and_then(Value::as_str).and_then(Kind::from_word);
    let kind = declared.or_else(|| super::parse_mch(&raw.to_string()).ok().map(|m| super::kind(Some(&m))));
    let serves = |n: &&str| kind.is_none_or(|k| crate::posting::post_serves(n, k));
    let mut out: Vec<String> = crate::posting::BUILTINS.iter().map(|(n, _)| *n).filter(serves).map(str::to_owned).collect();
    if !stated.is_empty() && !out.iter().any(|n| n == stated) {
        out.push(stated.to_owned());
    }
    out
}

/// An axis part with its pointer: (letter, pointer, rotary).
fn axes_with_pointers(v: &Value) -> Vec<(String, String, bool)> {
    fn walk(v: &Value, at: &str, out: &mut Vec<(String, String, bool)>) {
        let Some(arr) = v.pointer(at).and_then(Value::as_array) else { return };
        for (i, p) in arr.iter().enumerate() {
            let here = format!("{at}/{i}");
            let ty = p.get("type").and_then(Value::as_str);
            let rotary = ty == Some("rotary");
            let name = p.get("name").and_then(Value::as_str).or_else(|| p.get("id").and_then(Value::as_str)).unwrap_or("");
            if (ty == Some("linear") || rotary) && !name.is_empty() {
                out.push((name.to_owned(), here.clone(), rotary));
            }
            walk(v, &format!("{here}/parts"), out);
        }
    }
    let mut out = Vec::new();
    walk(v, "/kinematics/default/parts", &mut out);
    out
}

/// Every `type: "head"` part's pointer, depth-first.
pub(crate) fn heads(v: &Value) -> Vec<String> {
    fn walk(v: &Value, at: &str, out: &mut Vec<String>) {
        let Some(arr) = v.pointer(at).and_then(Value::as_array) else { return };
        for (i, p) in arr.iter().enumerate() {
            let here = format!("{at}/{i}");
            if p.get("type").and_then(Value::as_str) == Some("head") {
                out.push(here.clone());
            }
            walk(v, &format!("{here}/parts"), out);
        }
    }
    let mut out = Vec::new();
    walk(v, "/kinematics/default/parts", &mut out);
    out
}

/// The grid's groups for a machine file's JSON: General, Axes and
/// travels, Rotaries and limits, Spindle range, Feeds, Tool changer and
/// stations, Coolant codes, Live-tool codes, Units.
#[must_use]
pub fn groups(raw: &Value) -> Vec<FieldGroup> {
    use FieldKind::{Number as N, Text as T};
    let s = |x: &str| x.to_owned();
    let mut out = Vec::new();

    let mut g = Grid::new(raw, "General");
    g.add(s("Model"), s("general.model"), s("/general/model"), T, &[s("general.model")]);
    g.add(s("Vendor"), s("general.vendor"), s("/general/vendor"), T, &[]);
    g.add(s("Kind"), s("general.kind (mill, lathe, mill-turn or plasma)"), s("/general/kind"), T, &[s("general.kind")]);
    // #2374: the post the machine names, picked from the built-in posts.
    g.add(s("Post"), s("controller.post (a built-in post)"), s("/controller/post"), T, &[s("controller.post")]);
    if let Some(f) = g.group.fields.last_mut() {
        f.choices = post_choices(raw, &f.value);
    }
    out.push(g.group);

    let axes = axes_with_pointers(raw);
    let mut lin = Grid::new(raw, "Axes and travels");
    let mut rot = Grid::new(raw, "Rotaries and limits");
    for (name, at, rotary) in &axes {
        let g = if *rotary { &mut rot } else { &mut lin };
        let rk = [format!("{name} travel")];
        for end in ["min", "max"] {
            g.add(format!("{name} travel {end}"), format!("{name}.travel.{end}"), format!("{at}/travel/{end}"), N, &rk);
        }
        if *rotary {
            g.add(format!("{name} resolution"), format!("{name}.resolution"), format!("{at}/resolution"), N, &[format!("{name} resolution")]);
        }
    }
    out.push(lin.group);
    out.push(rot.group);

    let heads = heads(raw);
    let head = heads
        .iter()
        .find(|h| raw.pointer(&format!("{h}/spindle")).is_some_and(|s| !s.is_null()))
        .or(heads.first())
        .cloned();
    let mut g = Grid::new(raw, "Spindle range");
    if let Some(h) = &head {
        for (label, k) in [("Spindle min speed", "min_speed"), ("Spindle max speed", "max_speed"), ("Spindle power", "power")] {
            g.add(s(label), format!("spindle.{k}"), format!("{h}/spindle/{k}"), N, &[format!("spindle {k}"), s("spindle")]);
        }
    }
    out.push(g.group);

    let mut g = Grid::new(raw, "Feeds");
    for (name, _, _) in &axes {
        let c = format!("/controller/synced_configuration/parts/{}", tok(name));
        for (word, k) in [("feed", "max_normal_speed"), ("rapid", "max_rapid_speed")] {
            g.add(
                format!("{name} {word}"),
                format!("controller.synced_configuration.parts.{name}.{k}"),
                format!("{c}/{k}"),
                N,
                &[format!("{name} {word} ({k})")],
            );
        }
    }
    out.push(g.group);

    let mut g = Grid::new(raw, "Tool changer and stations");
    g.add(s("Tools"), s("tooling.number_of_tools"), s("/tooling/number_of_tools"), N, &[s("tooling.number_of_tools")]);
    // #828: the mount and the holder a new tool starts with.
    g.add(s("Spindle mount"), s("tooling.spindle_mount"), s("/tooling/spindle_mount"), T, &[s("tooling.spindle_mount")]);
    g.add(s("Default holder"), s("tooling.default_holder"), s("/tooling/default_holder"), T, &[s("tooling.default_holder")]);
    if let Some(h) = &head {
        for (label, k) in [("Max tool length", "max_tool_length"), ("Max tool diameter", "max_tool_diameter")] {
            g.add(s(label), format!("tool_station.{k}"), format!("{h}/tool_station/{k}"), N, &[]);
        }
    }
    out.push(g.group);

    let mut g = Grid::new(raw, "Coolant codes");
    let mut lists: Vec<String> = heads.iter().map(|h| format!("{h}/tool_station/coolants")).collect();
    lists.push(s("/tooling/coolants"));
    for list in lists {
        let Some(arr) = raw.pointer(&list).and_then(Value::as_array) else { continue };
        for (i, e) in arr.iter().enumerate() {
            let Some(name) = e.get("name").and_then(Value::as_str) else { continue };
            let code = if e.get("mcode").is_some() && e.get("mCode").is_none() { "mcode" } else { "mCode" };
            g.add(
                format!("Coolant {name} code"),
                format!("coolants[{i}].{code} ({name})"),
                format!("{list}/{i}/{code}"),
                T,
                &[format!("coolant {name}")],
            );
        }
    }
    out.push(g.group);

    let mut g = Grid::new(raw, "Live-tool codes");
    if let Some(h) = &head {
        for (label, k) in [
            ("Live tool C on", "c_axis_on"),
            ("Live tool C off", "c_axis_off"),
            ("Live tool spindle on", "spindle_on"),
            ("Live tool spindle off", "spindle_off"),
        ] {
            g.add(s(label), format!("tool_station.live_tool.{k}"), format!("{h}/tool_station/live_tool/{k}"), T, &[s("live_tool"), format!("live_tool.{k}")]);
        }
    }
    out.push(g.group);

    let mut g = Grid::new(raw, "Units");
    g.add(s("Length unit"), s("kinematics.default.units.length (mm or inch)"), s("/kinematics/default/units/length"), T, &[s("units.length")]);
    g.add(s("Angle unit"), s("kinematics.default.units.angle (degrees)"), s("/kinematics/default/units/angle"), T, &[s("units.angle")]);
    out.push(g.group);
    out
}

/// The JSON a field's typed text becomes; `None` = remove the key.
fn typed(kind: FieldKind, text: &str) -> Option<Value> {
    let t = text.trim();
    if t.is_empty() {
        return None;
    }
    if kind == FieldKind::Number {
        if let Ok(i) = t.parse::<i64>() {
            return Some(Value::Number(i.into()));
        }
        if let Some(n) = t.parse::<f64>().ok().and_then(Number::from_f64) {
            return Some(Value::Number(n));
        }
    }
    Some(Value::String(t.to_owned()))
}

/// Write a field's typed text into the file's JSON at its pointer:
/// missing objects on the way are created, an empty text removes the
/// key, everything else in the file is left as it was. Refused (by
/// name, never a panic) when the pointer runs through something that
/// is not an object, or an array index that is not there.
pub fn set(raw: &mut Value, pointer: &str, kind: FieldKind, text: &str) -> Result<(), String> {
    let new = typed(kind, text);
    let toks: Vec<String> = pointer
        .split('/')
        .skip(1)
        .map(|t| t.replace("~1", "/").replace("~0", "~"))
        .collect();
    let Some((last, path)) = toks.split_last() else {
        return Err(format!("{pointer}: not a field pointer"));
    };
    let removing = new.is_none();
    let mut at = &mut *raw;
    for t in path {
        if removing && !at.is_array() && at.get(t.as_str()).is_none() {
            return Ok(()); // removing a key whose parent is absent: nothing to do
        }
        if at.is_null() {
            *at = Value::Object(Map::new());
        }
        at = match at {
            Value::Object(m) => m.entry(t.clone()).or_insert_with(|| Value::Object(Map::new())),
            Value::Array(a) => {
                let i: usize = t.parse().map_err(|_| format!("{pointer}: {t} is not a list index"))?;
                a.get_mut(i).ok_or_else(|| format!("{pointer}: entry {t} is not in the list"))?
            }
            _ => return Err(format!("{pointer}: {t} sits inside a value that is not an object")),
        };
    }
    if at.is_null() && !removing {
        *at = Value::Object(Map::new());
    }
    match (at, new) {
        (Value::Object(m), Some(v)) => {
            m.insert(last.clone(), v);
        }
        (Value::Object(m), None) => {
            m.remove(last);
        }
        (Value::Null, None) => {}
        _ => return Err(format!("{pointer}: {last} sits inside a value that is not an object")),
    }
    if removing {
        prune(raw, path);
    }
    Ok(())
}

/// After a removal, drop the objects the removal emptied, innermost
/// first (`live_tool: {}` would read as a live tool stating none of its
/// words). List entries are never dropped - an index is a position.
fn prune(raw: &mut Value, path: &[String]) {
    for k in (1..=path.len()).rev() {
        let (parent, key) = path[..k].split_at(k - 1);
        let ptr: String = parent.iter().map(|t| format!("/{}", tok(t))).collect();
        let Some(Value::Object(m)) = raw.pointer_mut(&ptr) else { return };
        let Some(key) = key.first() else { return };
        if m.get(key).is_some_and(|v| v.as_object().is_some_and(Map::is_empty)) {
            m.remove(key);
        } else {
            return;
        }
    }
}
