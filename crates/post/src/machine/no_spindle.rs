//! The "no spindle" head rules one law serves two kinds with (#669
//! plasma, #530 EDM): a machine whose head holds a torch, a wire or an
//! electrode states no spindle, no live tool and no coolant list. Each
//! kind brings its own sentence; the walk and the keys are shared, so
//! the reader and the Machines pane's validator can never disagree.
//!
//! Provenance: our own rules (see `plasma.rs` and `edm.rs` for the
//! public sources each kind was specced from).

use serde_json::Value;

/// One kind's sentence for each refused key.
pub(super) struct NoSpindle {
    pub spindle: &'static str,
    pub live_tool: &'static str,
    pub coolant: &'static str,
}

/// The spindle / live-tool / coolant refusals over a file's raw JSON,
/// as `(key, rule)` pairs in file order, plus the rotary axis names the
/// walk passed (a kind that refuses rotaries names them itself).
pub(super) fn head_refusals(v: &Value, r: &NoSpindle) -> (Vec<(String, String)>, Vec<String>) {
    let mut out = Vec::new();
    let mut heads = Vec::new();
    let mut rotaries = Vec::new();
    walk(v.pointer("/kinematics/default/parts"), &mut heads, &mut rotaries);
    for head in heads {
        if let Some(sp) = head.get("spindle").filter(|s| !s.is_null()) {
            match sp.as_object().filter(|o| !o.is_empty()) {
                Some(o) => out.extend(o.keys().map(|k| (format!("spindle.{k}"), r.spindle.to_owned()))),
                None => out.push(("spindle".to_owned(), r.spindle.to_owned())),
            }
        }
        let ts = head.get("tool_station");
        if ts.and_then(|t| t.get("live_tool")).is_some_and(|l| !l.is_null()) {
            out.push(("live_tool".to_owned(), r.live_tool.to_owned()));
        }
        if states_list(ts.and_then(|t| t.get("coolants"))) {
            out.push(("coolants".to_owned(), r.coolant.to_owned()));
        }
    }
    if states_list(v.pointer("/tooling/coolants")) {
        out.push(("tooling.coolants".to_owned(), r.coolant.to_owned()));
    }
    (out, rotaries)
}

/// A coolant list that states something (an empty list states nothing).
fn states_list(v: Option<&Value>) -> bool {
    v.is_some_and(|l| !l.is_null() && l.as_array().is_none_or(|a| !a.is_empty()))
}

/// The head parts and the rotary axis names, depth-first.
fn walk<'a>(parts: Option<&'a Value>, heads: &mut Vec<&'a Value>, rotaries: &mut Vec<String>) {
    let Some(arr) = parts.and_then(Value::as_array) else { return };
    for p in arr {
        match p.get("type").and_then(Value::as_str) {
            Some("head") => heads.push(p),
            Some("rotary") => {
                let name = p
                    .get("name")
                    .or_else(|| p.get("id"))
                    .and_then(Value::as_str)
                    .unwrap_or("rotary axis");
                rotaries.push(name.to_owned());
            }
            _ => {}
        }
        walk(p.get("parts"), heads, rotaries);
    }
}
