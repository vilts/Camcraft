//! The MCP tools for an operation's display name and its place in its
//! setup's program (#45): `rename_operation` and `reorder_operation`,
//! the browser's Rename and drag-and-drop over MCP. Both run the same
//! undoable commands the app issues (`RenameOperation`,
//! `ReorderOperation`), so one `undo` reverts each, every refusal the
//! command makes at construction (a taken name, another setup) comes
//! back as its text, and the order they save is THE program order
//! (`setup::operation_order`) that posting, simulation and rest read.
//!
//! Provenance: our own tool surface over this repo's command layer.

use super::edit::find_op;
use super::tools::{opt_str, req_str, Tool};
use crate::commands::{RenameOperation, ReorderOperation};
use crate::model;
use crate::program_entry;
use crate::script::Env;

use serde_json::{json, Map, Value};

pub(super) const TOOLS: &[Tool] = &[
    Tool { name: "rename_operation", description: "Change an operation's display name. Its stable key (what scripts, saved files and other tools address it by) and its place in the program do not change; naming it back to its key clears the display name. Names are unique and cannot take another operation's key. One undo step.", schema: s_rename, run: rename_operation },
    Tool { name: "reorder_operation", description: "Move an operation before or after another operation of the SAME setup, as the browser's drag and drop does. The order is saved with the project and is the order the program posts, simulates and computes rest material in; moving between setups is refused. Answers with that setup's operations in their new order. One undo step.", schema: s_reorder, run: reorder_operation },
];

fn rename_operation(env: &mut Env, a: &Map<String, Value>) -> Result<Value, String> {
    let key = find_op(env, req_str(a, "name")?)?;
    let label = req_str(a, "new_name")?;
    let cmd = RenameOperation::new(&env.world, &key, label).map_err(|e| e.to_string())?;
    env.history.execute(&env.world, cmd);
    let e = model::find_by_name(&env.world, &key).ok_or_else(|| format!("no operation named '{key}'"))?;
    Ok(json!({ "operation": key, "name": model::operation_label(&env.world, &key, &e) }))
}

fn reorder_operation(env: &mut Env, a: &Map<String, Value>) -> Result<Value, String> {
    let key = find_op(env, req_str(a, "name")?)?;
    let target = find_op(env, req_str(a, "target")?)?;
    let after = match opt_str(a, "place")? {
        Some("after") => true,
        Some("before") | None => false,
        Some(other) => return Err(format!("\"place\" must be \"before\" or \"after\", got \"{other}\"")),
    };
    let cmd = ReorderOperation::new(&env.world, &key, &target, after).map_err(|e| e.to_string())?;
    env.history.execute(&env.world, cmd);
    let w = &env.world;
    let e = model::find_by_name(w, &key).ok_or_else(|| format!("no operation named '{key}'"))?;
    let setup = program_entry::effective_setup(w, &e);
    // The setup's operations in program order (Pause / Home entries are
    // not operations and are left out of the answer).
    let order: Vec<Value> = crate::setup::operation_order(w)
        .into_iter()
        .filter(|(_, o)| program_entry::effective_setup(w, o) == setup)
        .map(|(k, o)| json!({ "key": k, "name": model::operation_label(w, &k, &o) }))
        .collect();
    Ok(json!({ "operation": key, "setup": setup, "order": order }))
}

fn obj(props: Value, required: &[&str]) -> Value {
    json!({ "type": "object", "properties": props, "required": required, "additionalProperties": false })
}

fn s_rename() -> Value {
    obj(
        json!({
            "name": { "type": "string", "description": "Operation name (key or current display name)." },
            "new_name": { "type": "string", "description": "The new display name: 1-120 printable characters." }
        }),
        &["name", "new_name"],
    )
}

fn s_reorder() -> Value {
    obj(
        json!({
            "name": { "type": "string", "description": "The operation to move (key or display name)." },
            "target": { "type": "string", "description": "Another operation of the same setup to place it next to." },
            "place": { "type": "string", "enum": ["before", "after"], "description": "Before (default) or after the target." }
        }),
        &["name", "target"],
    )
}
