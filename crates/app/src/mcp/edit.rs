//! The write side of the MCP tools (#128). Every change here is a real
//! command executed through the session's History - the same commands
//! the app's panels issue - so `undo` / `redo` walk exactly what the
//! agent did, and every construction-time refusal (bad numbers, a tool
//! the kind cannot cut with, a name taken) comes back as its text.
//!
//! Provenance: our own tool surface over this repo's command layer.

use super::tools::{opt_str, req_str};
use crate::commands::{AddTool, CommandError, CreateOperation, DeleteOperation, EditOperationLinks, EditOperationParams, HolderPick};
use crate::model;
use crate::operation::{self, KindInfo, OpParams, SrcRef};
use crate::project;
use crate::script::{inventory, Env};

use serde_json::{json, Map, Value};

fn cmd_err(e: CommandError) -> String {
    e.to_string()
}

/// The draft every new operation is seeded from - the app's shared seed
/// (`operation::seed_params`, #511), so an agent's new op and a clicked
/// one agree.
pub(super) fn seed(kind: usize) -> OpParams {
    operation::seed_params(kind)
}

/// A kind by its label ("Pocket", any case) or its file key ("pocket").
fn kind_index(name: &str) -> Result<usize, String> {
    let want = name.trim().to_ascii_lowercase();
    (0..KindInfo::ALL.len())
        .find(|&i| {
            KindInfo::ALL.get(i).is_some_and(|k| k.label.to_ascii_lowercase() == want)
                || super::job::kind_defaults(i).0 == want
        })
        .ok_or_else(|| format!("unknown operation kind '{name}' - see list_operation_types"))
}

/// An operation by its stable key or its display label.
pub(super) fn find_op(env: &Env, name: &str) -> Result<String, String> {
    inventory(env)
        .into_iter()
        .find(|(k, e, _)| k == name || model::operation_label(&env.world, k, e) == name)
        .map(|(k, _, _)| k)
        .ok_or_else(|| format!("no operation named '{name}'"))
}

/// Apply a params patch (JSON merge patch, RFC 7386, over the kind's
/// field object) and read the result back as domain params. A key the
/// kind does not have is refused by name - serde would drop it, and a
/// silently dropped edit reads as "done as asked".
fn patched(base: &OpParams, patch: &Value) -> Result<OpParams, String> {
    let Value::Object(patch) = patch else {
        return Err("\"params\" must be an object of fields".into());
    };
    let Value::Object(tagged) = project::params_to_json(base) else {
        return Err("the operation's parameters could not be read".into());
    };
    let Some((key, Value::Object(mut fields))) = tagged.into_iter().next() else {
        return Err("the operation's parameters could not be read".into());
    };
    let mut patch_owned = patch.clone();
    project::migrate_legacy_patch(&key, &mut patch_owned);
    let patch = &patch_owned;
    if patch.contains_key("pass_start") {
        let reason = match base {
            OpParams::Face { .. } => Some("Face has no pass order: its rows run edge to edge across the face, so there is no inside or outside to start from"),
            OpParams::Slot { .. } => Some("Slot has no pass order. Each depth has one continuous pass. A tool-width cut follows the centerline. A wider cut connects rows."),
            OpParams::TrochoidalSlot { .. } => Some(operation::TROCHOIDAL_SLOT_NO_PASS_ORDER),
            _ => None,
        };
        if let Some(reason) = reason {
            return Err(format!("bad params patch: pass_start - {reason}"));
        }
    }
    for (k, v) in patch {
        merge(fields.entry(k.clone()).or_insert(Value::Null), v);
        if v.is_null() {
            fields.remove(k);
        }
    }
    let mut tagged = Map::new();
    tagged.insert(key.clone(), Value::Object(fields));
    let tagged = Value::Object(tagged);
    let next = project::params_from_json(&tagged).map_err(|e| {
        let mut context = Vec::new();
        if e.contains("expected map with a single key") {
            merged_key_context(&Value::Object(fields_of(base)), &tagged[&key], "", &mut context);
        }
        let detail = if context.is_empty() { String::new() } else {
            format!("; merged object keys: {}; use null to remove unwanted keys before changing shape", context.join("; "))
        };
        format!("bad params patch: {e}{detail}")
    })?;
    let Value::Object(back) = project::params_to_json(&next) else {
        return Err("the patched parameters could not be read back".into());
    };
    let mut back = back.get(&key).and_then(Value::as_object).cloned().unwrap_or_default();
    // The project DTO omits a zero wire taper. Use the typed value when
    // checking the patch so an explicit return to vertical is accepted.
    if let OpParams::WireContour(wire) = &next {
        back.insert("taper_deg".into(), json!(wire.taper_deg));
    }
    // Opt-in fields (`blend_turns`, `minimum_cone_radius`) are omitted at
    // their false/zero default, but a literal patch of that default is
    // still a known field: it is kept when a true/positive probe would
    // have come back.
    for (k, v) in patch {
        let opt_in = (*v == Value::Bool(false) || v.as_f64() == Some(0.0))
            && opt_in_kept(&key, &fields_of(base), k, v.is_boolean());
        let corner_relief =
            k == "corner_relief" && corner_relief_kept(&key, &fields_of(base));
        if !back.contains_key(k) && (opt_in || corner_relief) {
            back.insert(k.clone(), v.clone());
        } else if k == "pass_start" && *v == json!("auto") && !back.contains_key(k) && pass_start_kept(&key, &fields_of(base)) {
            // #3316: `auto` is never written, so it never comes back.
            back.insert(k.clone(), v.clone());
        } else if k == "picks" && base.kind() != operation::OpType::Probe && let Some(req) = v.as_object() {
            // #3339: empty `picks` / empty role lists are skipped on save.
            let mut bp = back.get("picks").and_then(Value::as_object).cloned().unwrap_or_default();
            for role in ["start", "exit"] {
                if req.get(role) == Some(&json!([])) && !bp.contains_key(role) {
                    bp.insert(role.into(), json!([]));
                }
            }
            back.insert("picks".into(), Value::Object(bp));
        }
    }
    if let Some(path) = lost(patch, &back, "") {
        return Err(format!("bad params patch: unknown field {path} - a {key} operation has no field \"{path}\"; see list_operation_types"));
    }
    Ok(next)
}

/// Context for a failed enum parse: merge patch can retain a former tag.
/// These are merged keys, not a claim that every singleton object is an enum.
fn merged_key_context(base: &Value, merged: &Value, path: &str, out: &mut Vec<String>) {
    let (Some(base), Some(merged)) = (base.as_object(), merged.as_object()) else {
        return;
    };
    for (key, value) in merged {
        if let Some(old) = base.get(key) {
            let child = if path.is_empty() { key.clone() } else { format!("{path}.{key}") };
            merged_key_context(old, value, &child, out);
        }
    }
    if !path.is_empty() && base.len() == 1 && merged.len() > 1 && base.keys().all(|k| merged.contains_key(k)) {
        out.push(format!("{path}: {}", merged.keys().cloned().collect::<Vec<_>>().join(", ")));
    }
}

/// The kind's field object of `p`, without its tag.
fn fields_of(p: &OpParams) -> Map<String, Value> {
    match project::params_to_json(p) {
        Value::Object(tagged) => tagged.into_iter().next().and_then(|(_, v)| match v {
            Value::Object(m) => Some(m),
            _ => None,
        }),
        _ => None,
    }
    .unwrap_or_default()
}

fn probe_kept(key: &str, fields: &Map<String, Value>, k: &str, val: Value) -> bool {
    let mut probe = fields.clone();
    probe.insert(k.to_owned(), val);
    project::params_from_json(&json!({ key: probe }))
        .map(|p| fields_of(&p).contains_key(k))
        .unwrap_or(false)
}

/// True when `key` has an opt-in field `k`: a true (boolean) or positive
/// (number) probe survives the typed round-trip. A boolean flag must come
/// back `true`. A misspelled key never does.
fn opt_in_kept(key: &str, fields: &Map<String, Value>, k: &str, boolean: bool) -> bool {
    let mut probe = fields.clone();
    probe.insert(
        k.to_owned(),
        if boolean { Value::Bool(true) } else { Value::from(1.0) },
    );
    project::params_from_json(&json!({ key: probe }))
        .map(|p| {
            let back = fields_of(&p);
            if boolean {
                back.get(k) == Some(&Value::Bool(true))
            } else {
                back.contains_key(k)
            }
        })
        .unwrap_or(false)
}

fn corner_relief_kept(key: &str, fields: &Map<String, Value>) -> bool {
    probe_kept(key, fields, "corner_relief", json!({ "kind": "dogbone" }))
}

/// True when the kind `key` has the `pass_start` field (#3316): a value
/// off `auto` survives the typed round-trip.
fn pass_start_kept(key: &str, fields: &Map<String, Value>) -> bool {
    probe_kept(key, fields, "pass_start", json!("outside"))
}

/// The first key path of `patch` (dotted, e.g. `clearance_z.ofset`) the
/// typed round-trip did not keep, at any depth. A null value is a
/// removal (RFC 7386) and has nothing to keep; an object is checked key
/// by key against what came back; any other value is a leaf that must
/// simply be present.
pub(super) fn lost(patch: &Map<String, Value>, back: &Map<String, Value>, prefix: &str) -> Option<String> {
    for (k, v) in patch {
        if v.is_null() {
            continue;
        }
        let path = if prefix.is_empty() { k.clone() } else { format!("{prefix}.{k}") };
        match (v, back.get(k)) {
            (_, None) => return Some(path),
            (Value::Object(p), Some(Value::Object(b))) => {
                if let Some(deeper) = lost(p, b, &path) {
                    return Some(deeper);
                }
            }
            // An object patch whose field came back as a scalar or null
            // lost every key it carried.
            (Value::Object(p), Some(_)) => {
                if let Some(first) = p.iter().find(|(_, x)| !x.is_null()).map(|(n, _)| n) {
                    return Some(format!("{path}.{first}"));
                }
            }
            _ => {}
        }
    }
    None
}

pub(super) fn merge(target: &mut Value, patch: &Value) {
    match (target, patch) {
        (Value::Object(t), Value::Object(p)) => {
            for (k, v) in p {
                if v.is_null() {
                    t.remove(k);
                } else {
                    merge(t.entry(k.clone()).or_insert(Value::Null), v);
                }
            }
        }
        (t, p) => *t = p.clone(),
    }
}

pub(super) fn add_tool(env: &mut Env, a: &Map<String, Value>) -> Result<Value, String> {
    let catalog = opt_str(a, "catalog")?;
    let (default_name, tool) = match (catalog, a.get("definition")) {
        (Some(c), None) => {
            let t = crate::tools::catalog()
                .into_iter()
                .find(|(n, _)| *n == c)
                .map(|(_, t)| t)
                .ok_or_else(|| format!("no catalog tool named '{c}' - see list_tool_catalog"))?;
            (Some(c.to_owned()), t)
        }
        (None, Some(d)) => {
            // #2016: a definition's own holder reference (a tool library
            // file's shape) is not read here; dropping it would land the
            // tool with another holder, so it is refused by name.
            if d.get("holder").is_some_and(|h| !h.is_null()) {
                return Err("bad tool definition: its \"holder\" reference is not read by add_tool - name the library holder in add_tool's own \"holder\" (set_tool_holder sets the gauge length and projection)".into());
            }
            (None, project::tool_from_json(d).map(super::tool_edit::settled).map_err(|e| format!("bad tool definition: {e}"))?)
        }
        _ => return Err("give exactly one of \"catalog\" or \"definition\"".into()),
    };
    let name = match opt_str(a, "name")? {
        Some(n) => n.to_owned(),
        None => default_name.ok_or("\"name\" is required with a definition")?,
    };
    // #828: a stated holder wins ("none" adds it bare); unstated, the
    // tool starts with the active setup's machine's default holder.
    let (pick, from) = match opt_str(a, "holder")? {
        None => (HolderPick::MachineDefault, "machine default"),
        Some(h) if h.trim().eq_ignore_ascii_case("none") => (HolderPick::Bare, "stated"),
        Some(h) => (HolderPick::Named(h.to_owned()), "stated"),
    };
    let cmd = AddTool::with_holder(&env.world, &name, tool, pick).map_err(cmd_err)?;
    let number = cmd.number();
    let holder = cmd.holder().map(str::to_owned);
    let note = cmd.note();
    env.history.execute(&env.world, cmd);
    Ok(json!({
        "tool": name,
        "number": number,
        "posts_as": number,
        "holder": holder,
        "holder_from": holder.as_ref().map(|_| from),
        "note": note,
    }))
}

pub(super) fn add_operation(env: &mut Env, a: &Map<String, Value>) -> Result<Value, String> {
    let kind = kind_index(req_str(a, "kind")?)?;
    // A kind that mounts no library tool (the Wire Contour, #1108) takes
    // none: an absent or empty "tool" is no tool. A tool named anyway
    // goes to the command, which refuses it in its own words.
    let mounts = KindInfo::ALL.get(kind).is_none_or(|k| k.mounts_tool());
    let tool = if mounts { req_str(a, "tool")? } else { opt_str(a, "tool")?.unwrap_or("") };
    let label = KindInfo::ALL.get(kind).map_or("Operation", |k| k.label);
    let name = match opt_str(a, "name")? {
        Some(n) => n.to_owned(),
        None => (1..)
            .map(|n| format!("{label}{n}"))
            .find(|c| model::find_by_name(&env.world, c).is_none())
            .unwrap_or_else(|| label.to_owned()),
    };
    let src = match opt_str(a, "source")? {
        None => SrcRef::Stock,
        Some(s) if s.eq_ignore_ascii_case("stock") => SrcRef::Stock,
        Some(s) => SrcRef::Entity(s.to_owned()),
    };
    let mut params = seed(kind);
    // The seed helix follows the tool, as the app's quick-add does.
    if let Some(t) = model::find_by_name(&env.world, tool).and_then(|e| model::tool_of(&env.world, &e)) {
        operation::seed_helix_for_tool(&mut params, t.diameter);
        operation::seed_slot_for_tool(&mut params, t.diameter);
        // #890: the feed from the tool's row for the document material;
        // an explicit params patch below still wins.
        crate::cutting_rows::seed_feed_for_tool(&env.world, &mut params, &t);
    }
    let named_entry = a.get("params").and_then(Value::as_object).is_some_and(|p| p.contains_key("entry"));
    if let Some(patch) = a.get("params") {
        params = patched(&params, patch)?;
    }
    let cmd = CreateOperation::with_named_entry(&env.world, &name, tool, src, params, named_entry).map_err(cmd_err)?;
    env.history.execute(&env.world, cmd);
    let mut out = json!({ "operation": name, "kind": label, "status": "never" });
    clearance_warning(env, &name, &mut out);
    Ok(out)
}

pub(super) fn edit_operation(env: &mut Env, a: &Map<String, Value>) -> Result<Value, String> {
    let key = find_op(env, req_str(a, "name")?)?;
    let patch = a.get("params").ok_or("\"params\" is required")?;
    let e = model::find_by_name(&env.world, &key).ok_or_else(|| format!("no operation named '{key}'"))?;
    let op = model::operation_of(&env.world, &e).ok_or_else(|| format!("'{key}' is not an operation"))?;
    let named_entry = patch.as_object().is_some_and(|p| p.contains_key("entry"));
    let next = patched(&op.params, patch)?;
    let cmd = EditOperationParams::with_named_entry(&env.world, &key, next, named_entry).map_err(cmd_err)?;
    env.history.execute(&env.world, cmd);
    let params = model::operation_of(&env.world, &e).map(|o| project::params_to_json(&o.params));
    let mut out = json!({ "operation": key, "status": "stale", "params": params });
    clearance_warning(env, &key, &mut out);
    Ok(out)
}

/// #2121: a clearance plane set inside raw stock is said when the
/// operation is made or edited (`operation::clearance_in_stock_advice`),
/// as the answer's `warning` - the rapid law holds the post at Generate.
/// #3631: a 2D Pocket whose tool fits nowhere in its region is said too.
pub(super) fn clearance_warning(env: &Env, key: &str, out: &mut Value) {
    let op = model::find_by_name(&env.world, key).and_then(|e| model::operation_of(&env.world, &e));
    let clearance = op.and_then(|o| operation::clearance_in_stock_advice(&env.world, o.setup.as_deref(), &o.src, &o.params, Some(key)));
    let said: Vec<String> = clearance.into_iter().chain(operation::pocket_fit_advice(&env.world, key)).collect();
    if !said.is_empty() {
        out["warning"] = json!(said.join("; "));
    }
}

/// The app's Detect holes button over MCP (#312): the operation's
/// source read for holes now (`operation::detect_holes`, the button's
/// own rule) and stored as one undoable parameter edit. Answers with
/// how many axial and radial holes went in, and (#1556) a sentence per
/// hole left out because the op cannot make it.
pub(super) fn detect_holes(env: &mut Env, a: &Map<String, Value>) -> Result<Value, String> {
    let key = find_op(env, req_str(a, "name")?)?;
    let e = model::find_by_name(&env.world, &key).ok_or_else(|| format!("no operation named '{key}'"))?;
    let op = model::operation_of(&env.world, &e).ok_or_else(|| format!("'{key}' is not an operation"))?;
    let operation::DetectedHoles { params: next, skipped, undersize } =
        operation::detect_holes(&env.world, &key, &op.params, op.setup.as_deref(), &op.src)?;
    let (holes, radial) = match &next {
        OpParams::Drill { holes, cross, .. } => (holes.len(), cross.len()),
        OpParams::Bore { holes, .. } | OpParams::Thread { holes, .. } => (holes.len(), 0),
        _ => (0, 0),
    };
    let cmd = EditOperationParams::new(&env.world, &key, next).map_err(cmd_err)?;
    env.history.execute(&env.world, cmd);
    // #1556: each hole the op's kind could not take, named - the GUI's
    // status line reads the same sentences. #2496: each hole kept but
    // drilled undersize (wider than the drill), named the same way.
    Ok(json!({ "operation": key, "holes": holes, "radial_holes": radial, "skipped": skipped, "undersize": undersize, "status": "stale" }))
}

pub(super) fn delete_operation(env: &mut Env, a: &Map<String, Value>) -> Result<Value, String> {
    let key = find_op(env, req_str(a, "name")?)?;
    let cmd = DeleteOperation::new(&env.world, &key).map_err(cmd_err)?;
    env.history.execute(&env.world, cmd);
    Ok(json!({ "deleted": key }))
}

pub(super) fn set_tool(env: &mut Env, a: &Map<String, Value>) -> Result<Value, String> {
    let key = find_op(env, req_str(a, "operation")?)?;
    let tool = req_str(a, "tool")?;
    let cmd = EditOperationLinks::new(&env.world, &key, Some(tool), None).map_err(cmd_err)?;
    env.history.execute(&env.world, cmd);
    let mut out = json!({ "operation": key, "tool": tool });
    clearance_warning(env, &key, &mut out);
    Ok(out)
}

fn history_state(env: &Env) -> Value {
    json!({ "can_undo": env.history.can_undo(), "can_redo": env.history.can_redo() })
}

pub(super) fn undo(env: &mut Env, _a: &Map<String, Value>) -> Result<Value, String> {
    if !env.history.undo(&env.world) {
        return Err("nothing to undo".into());
    }
    let mut out = history_state(env);
    out["undone"] = json!(true);
    Ok(out)
}

pub(super) fn redo(env: &mut Env, _a: &Map<String, Value>) -> Result<Value, String> {
    if !env.history.redo(&env.world) {
        return Err("nothing to redo".into());
    }
    let mut out = history_state(env);
    out["redone"] = json!(true);
    Ok(out)
}
