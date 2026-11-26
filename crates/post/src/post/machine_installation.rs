//! The machine and post views of installation facts, from Camcraft's .mch design (#3181).

use crate::machine::{self, Machine, installation};
use serde_json::{Map, Value, json};
use std::borrow::Cow;

/// File-form input through the public engine receives the same typed facts, limits and axis
/// inventory as the app and standalone CLI, which normally supply the static machine mirror.
pub(super) fn checked_input(m: &Value) -> Result<Cow<'_, Value>, String> {
    let has_facts = ["/controller/identity", "/tooling/tool_change"]
        .iter()
        .any(|key| m.pointer(key).is_some_and(|v| !v.is_null()));
    if has_facts {
        if m.pointer("/kinematics/default/parts").is_some() {
            return Ok(Cow::Owned(super::machine::mirror(&machine::parse_mch(
                &m.to_string(),
            )?)));
        }
        installation::identity(m)?;
        installation::tool_change(m)?;
    }
    Ok(Cow::Borrowed(m))
}

pub(super) fn mirror(m: &Machine, out: &mut Map<String, Value>) {
    if let Some(id) = &m.controller_identity {
        out.entry("controller").or_insert_with(|| json!({}))["identity"] = json!(id);
        out.insert("kind".into(), json!(machine::kind(Some(m)).word()));
        // The legacy static motion tree carries XYZ only. Retain additional named linear
        // axes so a post requiring exactly three can detect them.
        let extra: Vec<_> = m
            .axes
            .iter()
            .filter(|a| !a.rotary)
            .map(|a| a.name.as_str())
            .filter(|n| !matches!(*n, "X" | "Y" | "Z"))
            .collect();
        if !extra.is_empty() {
            out.insert("additionalLinearAxes".into(), json!(extra));
        }
    }
    if let Some(mode) = m.tool_change {
        out.entry("tooling").or_insert_with(|| json!({}))["tool_change"] = json!(mode);
    }
}

pub(super) fn script_props(m: &Value, props: &mut Value) {
    if let Some(id) = m.pointer("/controller/identity").filter(|v| !v.is_null()) {
        props["controllerIdentity"] = id.clone();
        let mut axes: Vec<_> = m
            .pointer("/kinematics/static/axes")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .map(|a| a.get("letter").and_then(Value::as_str))
            .collect();
        if let Some(extra) = m.get("additionalLinearAxes").and_then(Value::as_array) {
            // A malformed extra entry cannot supply a missing XYZ axis.
            axes.extend(
                extra
                    .iter()
                    .map(|v| v.as_str().filter(|s| !matches!(*s, "X" | "Y" | "Z"))),
            );
        }
        props["linearAxes"] = json!(axes);
    }
    if let Some(mode) = m.pointer("/tooling/tool_change").and_then(Value::as_str) {
        props["toolChangeMode"] = json!(mode);
    }
}
