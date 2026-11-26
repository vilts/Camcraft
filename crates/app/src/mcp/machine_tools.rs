//! The MCP `edit_machine` tool (#1162): a JSON merge patch (RFC 7396) over
//! the mounted machine's definition - the `edm` section above all - run
//! as the same undoable mount the Machine window's Apply pushes
//! (`LoadMachine` / `SetSetupMachine`). One undo step; every refusal is
//! named by key, an unknown key is refused (never ignored), and a
//! refused patch changes nothing.
//!
//! Provenance: our own tool surface over this repo's machine commands;
//! the merge-patch rule is RFC 7396.

use super::setup_tools::{cmd_err, obj};
use super::tools::{opt_str, Tool};
use crate::commands::{LoadMachine, SetSetupMachine};
use crate::machine::{self, Machine};
use crate::script::Env;
use crate::setup;

use serde_json::{json, Map, Value};

pub(super) const TOOLS: &[Tool] = &[Tool {
    name: "edit_machine",
    description: "Edit the mounted machine's definition with a JSON merge patch (RFC 7396: an object merges, null removes a key, any other value replaces) - e.g. {\"edm\": {\"max_orbit_radius\": 0.5, \"conditions\": [...]}}. Applies as one undoable mount, like the Machine window's Apply. Keys are those get_job's machine.definition shows; unknown keys and every insane value are refused by name, changing nothing. With setup, edits that setup's own machine (it must have one); without, the document's.",
    schema: s_edit_machine,
    run: edit_machine,
}];

/// Merge `patch` into `base` (RFC 7396).
fn merge(base: &mut Value, patch: &Value) {
    let Value::Object(p) = patch else {
        *base = patch.clone();
        return;
    };
    if !base.is_object() {
        *base = Value::Object(Map::new());
    }
    if let Value::Object(b) = base {
        for (k, v) in p {
            if v.is_null() {
                b.remove(k);
            } else {
                merge(b.entry(k.clone()).or_insert(Value::Null), v);
            }
        }
    }
}

/// The first patch key (as a path) the machine did not keep: an unknown
/// key deserializes into nothing, so it is missing from the re-read value.
/// Empty arrays and nulls are legitimately skipped by the serializer.
fn lost_key(patch: &Value, kept: &Value, path: &str) -> Option<String> {
    match (patch, kept) {
        (Value::Object(p), Value::Object(k)) => p.iter().find_map(|(key, v)| {
            let here = if path.is_empty() { key.clone() } else { format!("{path}.{key}") };
            match k.get(key) {
                Some(kv) => lost_key(v, kv, &here),
                None if v.is_null() || v.as_array().is_some_and(Vec::is_empty) => None,
                None => Some(here),
            }
        }),
        (Value::Array(p), Value::Array(k)) => p.iter().zip(k).enumerate().find_map(|(i, (pv, kv))| lost_key(pv, kv, &format!("{path}[{i}]"))),
        (Value::Object(_), _) => Some(path.to_owned()),
        _ => None,
    }
}

/// The MCP definition uses the same `edm.codes` path as machine files.
pub(super) fn definition(machine: &Machine) -> Result<Value, String> {
    let mut value = serde_json::to_value(machine).map_err(|e| format!("machine unreadable: {e}"))?;
    if let Some(codes) = value.as_object_mut().and_then(|m| m.remove("edm_codes")) {
        if !value["edm"].is_object() {
            value["edm"] = json!({});
        }
        value["edm"]["codes"] = codes;
    }
    Ok(value)
}

/// Convert only the MCP view; embedded project machines keep their schema.
fn from_definition(mut value: Value) -> Result<Machine, String> {
    let codes = value.get_mut("edm").and_then(Value::as_object_mut).and_then(|e| e.remove("codes"));
    if let Some(codes) = codes {
        let entries = codes.as_object().ok_or("patch refused: edm.codes must be an object")?;
        for (name, text) in entries {
            let Some(text) = text.as_str() else {
                return Err(format!("patch refused: edm.codes.{name} must be control text"));
            };
            let entry = std::iter::once((name.clone(), text.to_owned())).collect();
            if !crate::machine::edm_codes::codes_sane(&entry) {
                return Err(format!("patch refused: edm.codes.{name}: use a lowercase code name and nonblank printable ASCII text of at most 64 characters without parentheses"));
            }
        }
        value["edm_codes"] = codes;
    }
    serde_json::from_value(value).map_err(|e| format!("patch refused: {e}"))
}

fn edit_machine(env: &mut Env, a: &Map<String, Value>) -> Result<Value, String> {
    let patch = a.get("patch").filter(|p| p.is_object()).ok_or("\"patch\" must be a JSON object")?;
    if patch.get("edm_codes").is_some() {
        return Err("patch refused: unknown key 'edm_codes'; use edm.codes (see get_job machine.definition)".into());
    }
    let w = &env.world;
    let target = match opt_str(a, "setup")? {
        Some(s) if !setup::exists(w, s) => return Err(format!("no setup named '{s}'")),
        Some(s) if crate::setup_mount::own_machine(w, Some(s)).is_none() => {
            return Err(format!("setup '{s}' follows the document's machine; edit that (omit setup) or give it its own first"));
        }
        s => s,
    };
    let (path, mounted) = machine::of_setup(w, target);
    let Some(mounted) = mounted else { return Err("no machine is mounted - load_machine first".into()) };
    let mut value = definition(&mounted)?;
    merge(&mut value, patch);
    let next = from_definition(value)?;
    let kept = definition(&next)?;
    if let Some(k) = lost_key(patch, &kept, "") {
        return Err(format!("patch refused: unknown key '{k}' (see get_job machine.definition for the keys)"));
    }
    if let Some(r) = next.edm.as_ref().and_then(|e| e.refusal()) {
        return Err(format!("patch refused: {r}"));
    }
    let k = machine::kind(Some(&next));
    // A kind edit cannot establish a work spindle; load its machine definition instead.
    let turning = |kind| matches!(kind, machine::Kind::Lathe | machine::Kind::MillTurn);
    if turning(k) && !turning(machine::kind(Some(&mounted))) {
        return Err(format!("patch refused: kind: machine '{}' has no established turning capability; load a turning machine file instead", mounted.model));
    }
    if next.edm.as_ref().is_some_and(|e| !e.fits(k)) {
        return Err(format!("patch refused: edm: states a key this machine kind ({k:?}) does not take (a wire states no sinker key, a sinker no wire key, other kinds no edm section)"));
    }
    if next == mounted {
        return Err("patch changes nothing".into());
    }
    match target {
        None => {
            let cmd = LoadMachine::new(w, path, Some(next)).map_err(cmd_err)?;
            env.history.execute(&env.world, cmd);
        }
        Some(s) => {
            let cmd = SetSetupMachine::new(w, s, Some((path, Some(next)))).map_err(cmd_err)?;
            env.history.execute(&env.world, cmd);
        }
    }
    let (path, m) = machine::of_setup(&env.world, target);
    Ok(json!({ "setup": target, "path": path, "machine": m.as_ref().map(definition).transpose()? }))
}

fn s_edit_machine() -> Value {
    obj(
        json!({
            "patch": { "type": "object", "description": "JSON merge patch over the machine definition, e.g. {\"edm\": {\"max_orbit_radius\": 0.5}}." },
            "setup": { "type": "string", "description": "A declared setup with its own machine; omitted edits the document's machine." }
        }),
        &["patch"],
    )
}
