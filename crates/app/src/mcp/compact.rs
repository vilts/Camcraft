//! #1944: compact read modes that keep a CAM job inside a model's context
//! window - the `get_job` summary, the `list_operation_types` kind and
//! section filter, and the short `list_features` rows.
//!
//! Provenance: our own reductions over this repo's MCP JSON views; no
//! vendor material consulted.

use crate::operation::KindInfo;

use serde_json::{json, Map, Value};
use std::collections::HashMap;

/// Rows per feature kind of a compact `list_features` without a `limit`:
/// a chat-sized page. The full listing keeps its own default.
pub(super) const COMPACT_LIMIT: usize = 25;

/// The section names `list_operation_types` writes, in kind order, once each.
fn sections() -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for k in KindInfo::ALL {
        let s = format!("{:?}", k.section);
        if !out.contains(&s) {
            out.push(s);
        }
    }
    out
}

/// An optional boolean argument; absent or `null` is `false`.
pub(super) fn flag(a: &Map<String, Value>, key: &str) -> Result<bool, String> {
    match a.get(key) {
        None | Some(Value::Null) => Ok(false),
        Some(Value::Bool(b)) => Ok(*b),
        Some(other) => Err(format!("\"{key}\" must be true or false, got {other}")),
    }
}

pub(super) fn s_get_job() -> Value {
    json!({ "type": "object", "additionalProperties": false, "required": [], "properties": {
        "summary": { "type": "boolean", "description": "Short form: omit the machine definition and the operation fields that still hold their defaults (default false)." }
    }})
}

pub(super) fn s_list_operation_types() -> Value {
    json!({ "type": "object", "additionalProperties": false, "required": [], "properties": {
        "kind": { "type": "string", "description": "Only this kind, by key or label (e.g. \"face\")." },
        "section": { "type": "string", "description": format!("Only the kinds of this section: {}.", sections().join(", ")) }
    }})
}

/// The default field object of every kind, by its file key.
fn defaults_by_key() -> HashMap<String, Map<String, Value>> {
    (0..KindInfo::ALL.len())
        .filter_map(|i| match super::job::kind_defaults(i) {
            (key, Value::Object(m)) if !key.is_empty() => Some((key, m)),
            _ => None,
        })
        .collect()
}

/// Keep only the fields that differ from the kind's defaults.
fn changed_fields(params: &Value, defaults: &HashMap<String, Map<String, Value>>) -> Value {
    let Some(outer) = params.as_object() else {
        return params.clone();
    };
    let mut out = Map::new();
    for (key, fields) in outer {
        let (Some(fields), Some(base)) = (fields.as_object(), defaults.get(key)) else {
            out.insert(key.clone(), fields.clone());
            continue;
        };
        let kept: Map<String, Value> =
            fields.iter().filter(|(n, v)| base.get(*n) != Some(*v)).map(|(n, v)| (n.clone(), v.clone())).collect();
        out.insert(key.clone(), Value::Object(kept));
    }
    Value::Object(out)
}

/// Drop the `null` and empty-list fields of one row (not inside `params`,
/// where a `null` is a stated value).
fn drop_empty(row: &mut Value) {
    if let Some(m) = row.as_object_mut() {
        m.retain(|k, v| k == "params" || !(v.is_null() || v.as_array().is_some_and(Vec::is_empty)));
    }
}

/// Turn a full `get_job` reply into its summary form, in place.
pub(super) fn summarize_job(job: &mut Value) {
    if let Some(machine) = job.get_mut("machine").and_then(Value::as_object_mut)
        && let Some(def) = machine.remove("definition")
    {
        if let Some(name) = def.get("name").filter(|n| !n.is_null()) {
            machine.insert("name".into(), name.clone());
        }
        machine.insert("definition_omitted".into(), json!(!def.is_null()));
    }
    let defaults = defaults_by_key();
    if let Some(ops) = job.get_mut("operations").and_then(Value::as_array_mut) {
        for op in ops {
            if let Some(params) = op.get_mut("params") {
                *params = changed_fields(params, &defaults);
            }
            drop_empty(op);
        }
    }
    for list in ["setups", "tools", "models"] {
        if let Some(rows) = job.get_mut(list).and_then(Value::as_array_mut) {
            for row in rows {
                drop_empty(row);
                if let Some(wcs) = row.get_mut("wcs") {
                    drop_empty(wcs);
                }
            }
        }
    }
    job["summary"] = json!(true);
    job["note"] = json!("Summary: operation params list only the fields changed from the kind's defaults (see list_operation_types). Call get_job without summary for the whole document.");
}

/// Keep the kinds that `kind` and `section` name; refuse a name that matches none.
pub(super) fn filter_kinds(kinds: Vec<Value>, a: &Map<String, Value>) -> Result<Vec<Value>, String> {
    let kind = super::tools::opt_str(a, "kind")?;
    let section = super::tools::opt_str(a, "section")?;
    if let Some(s) = section {
        let known = sections();
        if !known.iter().any(|n| n.eq_ignore_ascii_case(s)) {
            return Err(format!("unknown section \"{s}\" - sections: {}", known.join(", ")));
        }
    }
    let same = |v: &Value, want: &str| v.as_str().is_some_and(|t| t.eq_ignore_ascii_case(want));
    let kept: Vec<Value> = kinds
        .into_iter()
        .filter(|k| kind.is_none_or(|want| same(&k["key"], want) || same(&k["label"], want)))
        .filter(|k| section.is_none_or(|want| same(&k["section"], want)))
        .collect();
    if let (Some(want), true) = (kind, kept.is_empty()) {
        return Err(format!("no operation kind \"{want}\"{} - call list_operation_types without \"kind\" for the list", section.map(|s| format!(" in section {s}")).unwrap_or_default()));
    }
    Ok(kept)
}

/// The short form of one `list_features` row: its id, its name and the
/// facts a pick is chosen by; counts in place of nested lists.
fn compact_row(kind: &str, row: &Value) -> Value {
    let keep: &[&str] = match kind {
        "faces" => &["id", "name", "facing", "area", "patch"],
        "chains" => &["id", "name", "closed", "edges", "length"],
        "patches" => &["id", "name"],
        _ => &["id", "name", "at"],
    };
    let mut out = Map::new();
    for k in keep {
        if let Some(v) = row.get(*k) {
            out.insert((*k).to_owned(), v.clone());
        }
    }
    // #2186: a face whose outside cannot be told still says so.
    if let Some(why) = row.get("outside_unknown").filter(|v| !v.is_null()) {
        out.insert("outside_unknown".into(), why.clone());
    }
    for list in ["loops", "faces"] {
        if let Some(items) = row.get(list).and_then(Value::as_array) {
            out.insert(format!("{list}_count"), json!(items.len()));
        }
    }
    Value::Object(out)
}

pub(super) fn compact_rows(kind: &str, rows: Vec<Value>) -> Vec<Value> {
    rows.iter().map(|r| compact_row(kind, r)).collect()
}
