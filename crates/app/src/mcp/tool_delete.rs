//! `delete_tool` (#734): remove a document tool through the app's
//! undoable `DeleteTool`, refusing a tool an operation still uses.
//!
//! Provenance: our own tool surface over this repo's command layer.

use super::tools::req_str;
use crate::commands::DeleteTool;
use crate::model;
use crate::script::Env;

use serde_json::{json, Map, Value};

pub(super) fn delete_tool(env: &mut Env, a: &Map<String, Value>) -> Result<Value, String> {
    let name = req_str(a, "name")?;
    let entity = model::find_by_name(&env.world, name).ok_or_else(|| format!("no tool named '{name}'"))?;
    if model::tool_of(&env.world, &entity).is_none() {
        return Err(format!("'{name}' is not a tool"));
    }
    // Deleting a used tool would strand its operations without one.
    let users: Vec<String> = model::named_entities(&env.world)
        .into_iter()
        .filter(|(_, e)| model::operation_of(&env.world, e).is_some_and(|op| op.tool == name))
        .map(|(n, _)| n)
        .collect();
    if let Some(first) = users.first() {
        let more = match users.len().saturating_sub(1) {
            0 => String::new(),
            n => format!(" (and {n} more)"),
        };
        return Err(format!(
            "tool '{name}' is used by operation {first}{more} - give the operation another tool first (set_tool); nothing deleted"
        ));
    }
    let cmd = DeleteTool::new(&env.world, name).map_err(|e| e.to_string())?;
    env.history.execute(&env.world, cmd);
    Ok(json!({ "deleted": name }))
}
