//! Every accepted field of an operation kind for `list_operation_types`
//! (#3449): the defaults name only the fields that serialize; the serde
//! trace of `ParamsDto` names the rest - optional fields, enum values and
//! the keys of an object - by one rule for all kinds.
//!
//! Provenance: our own view over this repo's params DTOs.

mod trace;
mod written;

use crate::project::{self, ParamsDto};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use trace::{trace, Node};

/// Nested objects are spelled this deep in a field's `shape`.
const SHAPE_DEPTH: usize = 3;

/// A traced flattened type and the variant's own optional keys (name, type).
type Flattened = (BTreeMap<String, Node>, &'static [(&'static str, &'static str)]);

/// The kinds whose params flatten a struct in (`#[serde(flatten)]`). Serde
/// names no keys for such a struct, so the flattened type is traced by
/// itself, and the variant's own optional keys are named here.
fn flattened(key: &str) -> Option<Flattened> {
    match key {
        "thread" => Some((
            trace::<project::ThreadPassesDto>(key, None),
            &[
                ("retract_z", "any"),
                ("bottom_z", "any"),
                ("pitch", "number"),
                ("external", "boolean"),
                ("break_through", "number"),
                ("picks", "object"),
            ],
        )),
        "sinker_side_burn" => Some((trace::<project::SinkerDto>(key, None), &[])),
        _ => None,
    }
}

fn child(path: &str, name: &str) -> String {
    format!("{path}.{name}")
}

/// A short type text: `number`, `a|b|c`, `{key: type, key?: type}`, `[type]`.
fn shape(nodes: &BTreeMap<String, Node>, path: &str, depth: usize) -> String {
    let Some(n) = nodes.get(path) else {
        return "any".into();
    };
    if !n.values.is_empty() {
        return n.values.join("|");
    }
    match n.ty {
        "object" if !n.fields.is_empty() && depth < SHAPE_DEPTH => {
            let keys: Vec<String> = n
                .fields
                .iter()
                .map(|f| {
                    let p = child(path, f);
                    let opt = if nodes.get(&p).is_some_and(|c| c.optional) { "?" } else { "" };
                    format!("{f}{opt}: {}", shape(nodes, &p, depth + 1))
                })
                .collect();
            format!("{{{}}}", keys.join(", "))
        }
        "array" => format!("[{}]", shape(nodes, &format!("{path}[]"), depth + 1)),
        "" => "any".into(),
        t => t.into(),
    }
}

/// Completes a kind's field rows. `fields` holds the defaults (and the
/// stated extras); `defaults` is the serialized default object. Each row
/// gains its enum `values` or object `shape`; each traced field the rows
/// lack is appended with `"optional": true`. Returns false when the
/// kind's params cannot be traced (a flatten without an entry above).
pub(super) fn complete(key: &str, defaults: &Value, fields: &mut Vec<Value>) -> bool {
    let mut nodes = trace::<ParamsDto>("", Some(key));
    let mut names: Vec<&'static str> = nodes.get(key).map(|n| n.fields.clone()).unwrap_or_default();
    let mut stated: &[(&str, &str)] = &[];
    let opaque = nodes.get(key).is_none_or(|n| n.opaque || n.fields.is_empty());
    let mut known = !opaque;
    if opaque && let Some((inner, outer)) = flattened(key) {
        names.extend(inner.get(key).map(|n| n.fields.clone()).unwrap_or_default());
        nodes.extend(inner.into_iter().filter(|(p, _)| p != key));
        stated = outer;
        known = true;
    }
    // A key the type reads but never writes is a legacy key that the
    // load migrates (`skip_serializing`): it is not offered.
    let dto: Option<ParamsDto> = serde_json::from_value(json!({ key: defaults })).ok();
    if let Some(writes) = dto.as_ref().and_then(written::keys) {
        names.retain(|n| writes.contains(n));
    }
    let in_defaults = |n: &str| defaults.get(n).is_some();
    for row in fields.iter_mut() {
        let Some(name) = row["name"].as_str().map(str::to_owned) else {
            continue;
        };
        if !in_defaults(&name) {
            row["optional"] = json!(true);
        }
        decorate(&nodes, &child(key, &name), row);
    }
    let missing = |rows: &Vec<Value>, n: &str| !rows.iter().any(|r| r["name"] == n);
    for name in names {
        if missing(fields, name) {
            let path = child(key, name);
            let ty = match nodes.get(&path).map_or("", |n| n.ty) {
                "" | "null" => "any",
                t => t,
            };
            let mut row = json!({ "name": name, "type": ty, "default": Value::Null, "optional": true });
            decorate(&nodes, &path, &mut row);
            fields.push(row);
        }
    }
    for (name, ty) in stated {
        if missing(fields, name) {
            fields.push(json!({ "name": name, "type": ty, "default": Value::Null, "optional": true }));
        }
    }
    fields.sort_by(|a, b| a["name"].as_str().cmp(&b["name"].as_str()));
    known
}

fn decorate(nodes: &BTreeMap<String, Node>, path: &str, row: &mut Value) {
    let Some(n) = nodes.get(path) else {
        return;
    };
    if !n.values.is_empty() {
        row["values"] = json!(n.values);
    } else if n.ty == "array" || (n.ty == "object" && !n.fields.is_empty()) {
        row["shape"] = json!(shape(nodes, path, 0));
    }
}
