//! MCP tools for "Do not touch" protected faces and edges (#2055).
//! Provenance: our own MCP surface over `SetOperationDoNotTouch` and `operation::avoid`.

use super::{edit, pick, tools::req_str};
use crate::commands::{CommandError, SetOperationDoNotTouch};
use crate::model;
use crate::operation::DoNotTouch;
use crate::script::Env;
use serde_json::{json, Map, Value};

fn cmd_err(e: CommandError) -> String {
    e.to_string()
}

pub(super) fn view(w: &flecs_ecs::prelude::World, dnt: &DoNotTouch) -> Value {
    json!({
        "clearance": dnt.clearance,
        "picks": pick::picks_view(w, &dnt.picks),
    })
}

pub(super) fn set_operation_do_not_touch(env: &mut Env, a: &Map<String, Value>) -> Result<Value, String> {
    for k in a.keys() {
        if !matches!(k.as_str(), "operation" | "clearance" | "picks") {
            return Err(format!("set_operation_do_not_touch: unknown field '{k}'"));
        }
    }
    let key = edit::find_op(env, req_str(a, "operation")?)?;
    let clearance = match a.get("clearance") {
        None => 0.0,
        Some(v) => v
            .as_f64()
            .ok_or_else(|| "set_operation_do_not_touch: \"clearance\" must be a number (mm)".to_owned())?,
    };
    let picks = match a.get("picks") {
        None => Vec::new(),
        Some(v) => pick::picks_of(env, Some(v), "picks")?,
    };
    let cmd = SetOperationDoNotTouch::new(&env.world, &key, DoNotTouch { clearance, picks })
        .map_err(cmd_err)?;
    env.history.execute(&env.world, cmd);
    let e = model::find_by_name(&env.world, &key).ok_or_else(|| format!("no operation named '{key}'"))?;
    let dnt = model::operation_of(&env.world, &e)
        .map(|o| o.area.do_not_touch)
        .unwrap_or_default();
    Ok(json!({
        "operation": key,
        "status": "stale",
        "clearance": dnt.clearance,
        "picks": pick::picks_view(&env.world, &dnt.picks),
    }))
}

pub(super) fn get_operation_do_not_touch(env: &mut Env, a: &Map<String, Value>) -> Result<Value, String> {
    for k in a.keys() {
        if k != "operation" {
            return Err(format!("get_operation_do_not_touch: unknown field '{k}'"));
        }
    }
    let key = edit::find_op(env, req_str(a, "operation")?)?;
    let e = model::find_by_name(&env.world, &key).ok_or_else(|| format!("no operation named '{key}'"))?;
    let op = model::operation_of(&env.world, &e).ok_or_else(|| format!("'{key}' is not an operation"))?;
    Ok(json!({
        "operation": key,
        "clearance": op.area.do_not_touch.clearance,
        "picks": pick::picks_view(&env.world, &op.area.do_not_touch.picks),
    }))
}

pub(super) fn s_set_do_not_touch() -> Value {
    json!({
        "type": "object",
        "properties": {
            "operation": { "type": "string", "description": "Operation name (key or label)." },
            "clearance": {
                "type": "number",
                "minimum": 0.0,
                "description": "Clearance standoff (mm) around every protected face/edge; stock_to_leave adds on top (default 0.0)."
            },
            "picks": pick::pick_list_schema("Protected faces or edge chains (vertices are refused).")
        },
        "required": ["operation"],
        "additionalProperties": false
    })
}
