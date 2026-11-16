//! The machine property sheet (issue #287): a `.mch` restated as
//! grouped, read-only rows for the Machines pane.
//!
//! Provenance: our own layout, specced from the `.mch` grammar in
//! `assets/machines/AGENTS.md`; group names and row words are ours.
//!
//! Numbers are shown AS THE FILE STATES THEM, in the file's own length
//! unit - the reader scales an inch file to mm for the checker, but a
//! person reading their machine wants the numbers they typed. An
//! unstated value says so; it is never shown as a zero.

use serde_json::Value;

use super::validate::{axis_parts, head_parts};
use super::{Kind, Machine};

/// One collapsible group of the sheet: a title and its (label, value)
/// rows.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PropGroup {
    pub title: String,
    pub rows: Vec<(String, String)>,
}

impl PropGroup {
    fn new(title: &str) -> Self {
        Self { title: title.to_owned(), rows: Vec::new() }
    }

    fn row(&mut self, label: impl Into<String>, value: impl Into<String>) {
        self.rows.push((label.into(), value.into()));
    }
}

const UNSTATED: &str = "not stated";

/// A stated value as text: a number plainly, anything else as its JSON
/// (so a malformed value is visible as itself), absence as "not stated".
fn val(v: Option<&Value>, unit: &str) -> String {
    match v {
        None | Some(Value::Null) => UNSTATED.to_owned(),
        Some(Value::Number(n)) => match n.as_f64() {
            Some(x) if unit.is_empty() => format!("{x}"),
            Some(x) => format!("{x} {unit}"),
            None => n.to_string(),
        },
        Some(Value::String(s)) => s.clone(),
        Some(other) => other.to_string(),
    }
}

/// The rigid-tapping row (#786): what the stated capability does to a
/// program, not a bare JSON boolean. Anything else shows as itself (the
/// validator names it).
fn tapping(v: Option<&Value>) -> String {
    match v {
        Some(Value::Bool(true)) => "yes".to_owned(),
        Some(Value::Bool(false)) => "no - tap cycles are refused".to_owned(),
        None | Some(Value::Null) => format!("{UNSTATED} - not limited"),
        Some(other) => other.to_string(),
    }
}

/// The sheet's groups: General, Axes, Spindle, Tool station, Coolants,
/// Posts. `raw` is the file's JSON, `m` the reader's view of it.
pub(crate) fn groups(raw: &Value, m: &Machine, k: Option<Kind>, posts: &[&str]) -> Vec<PropGroup> {
    let len = if m.units.as_deref() == Some("inch") { "in" } else { "mm" };
    let mut out = Vec::new();

    let mut g = PropGroup::new("General");
    g.row("Model", val(raw.pointer("/general/model"), ""));
    g.row("Vendor", val(raw.pointer("/general/vendor"), ""));
    let declared = raw.pointer("/general/kind").and_then(Value::as_str).filter(|w| !w.is_empty());
    g.row(
        "Kind",
        match (declared, k) {
            (Some(w), _) => w.to_owned(),
            (None, Some(k)) => format!("{} (inferred from the axes)", k.word()),
            (None, None) => UNSTATED.to_owned(),
        },
    );
    g.row("Length unit", val(raw.pointer("/kinematics/default/units/length"), ""));
    g.row("Angle unit", val(raw.pointer("/kinematics/default/units/angle"), ""));
    g.row("Spindles", m.spindles.map_or_else(|| UNSTATED.to_owned(), |n| n.to_string()));
    out.push(g);

    let mut g = PropGroup::new("Axes");
    let ctrl = raw.pointer("/controller/synced_configuration/parts");
    for (name, part, rotary) in axis_parts(raw) {
        let (dist, rate) = if rotary { ("deg", "deg/min") } else { (len, if len == "in" { "in/min" } else { "mm/min" }) };
        let t = part.get("travel");
        let travel = match (t.and_then(|t| t.get("min")), t.and_then(|t| t.get("max"))) {
            (None, None) => UNSTATED.to_owned(),
            (lo, hi) => format!("{} .. {}", val(lo, ""), val(hi, dist)),
        };
        let c = ctrl.and_then(|c| c.get(name.as_str()));
        let mut s = format!(
            "{}, travel {travel}, feed {}, rapid {}",
            if rotary { "rotary" } else { "linear" },
            val(c.and_then(|c| c.get("max_normal_speed")), rate),
            val(c.and_then(|c| c.get("max_rapid_speed")), rate),
        );
        if rotary {
            s.push_str(&format!(", about {}", val(part.get("axis"), "")));
        }
        g.row(name, s);
    }
    if g.rows.is_empty() {
        g.row("Axes", "none stated");
    }
    out.push(g);

    let heads = head_parts(raw);
    let head = heads.iter().find(|h| h.get("spindle").is_some_and(|s| !s.is_null())).or(heads.first());
    let sp = head.and_then(|h| h.get("spindle"));
    let mut g = PropGroup::new("Spindle");
    g.row("Min speed", val(sp.and_then(|s| s.get("min_speed")), "rpm"));
    g.row("Max speed", val(sp.and_then(|s| s.get("max_speed")), "rpm"));
    g.row("Power", val(sp.and_then(|s| s.get("power")), "kW"));
    if let Some(r) = sp.and_then(|s| s.get("ranges")).and_then(Value::as_array) {
        for (i, r) in r.iter().enumerate() {
            g.row(
                format!("Range {}", i + 1),
                format!("{} .. {}", val(r.get("min_speed"), ""), val(r.get("max_speed"), "rpm")),
            );
        }
    }
    g.row("Rigid tapping", tapping(sp.and_then(|s| s.get("rigid_tapping"))));
    g.row("Body diameter", val(sp.and_then(|s| s.get("body_diameter")), len));
    g.row("Body length", val(sp.and_then(|s| s.get("body_length")), len));
    out.push(g);

    let ts = head.and_then(|h| h.get("tool_station"));
    let mut g = PropGroup::new("Tool station");
    g.row("Max tool length", val(ts.and_then(|t| t.get("max_tool_length")), len));
    g.row("Max tool diameter", val(ts.and_then(|t| t.get("max_tool_diameter")), len));
    g.row("Tools", val(raw.pointer("/tooling/number_of_tools"), ""));
    g.row(
        "Live tool",
        match &m.live_tool {
            Some(l) => format!(
                "C on {}, C off {}, spindle on {}, spindle off {}",
                l.c_axis_on, l.c_axis_off, l.spindle_on, l.spindle_off
            ),
            None => UNSTATED.to_owned(),
        },
    );
    out.push(g);

    let mut g = PropGroup::new("Coolants");
    for c in &m.coolants {
        g.row(c.name.clone(), if c.m_code.is_empty() { "no code".to_owned() } else { c.m_code.clone() });
    }
    if g.rows.is_empty() {
        g.row("Coolants", "none stated");
    }
    out.push(g);

    let mut g = PropGroup::new("Posts");
    g.row("Built-in posts", if posts.is_empty() { "none".to_owned() } else { posts.join(", ") });
    out.push(g);
    out
}
