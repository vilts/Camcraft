//! `edit_tool` (#1062): a partial tool definition applied through the
//! app's undoable `ChangeTool`, so its gates and refusals are the GUI's.
//!
//! Provenance: our own tool surface over this repo's command layer.

use super::edit::{lost, merge};
use super::tools::req_str;
use crate::commands::ChangeTool;
use crate::model;
use crate::project;
use crate::script::Env;

use serde_json::{json, Map, Value};

/// Keys of a tool definition `ChangeTool` does not own: the number is
/// `SetToolNumber`'s, the body and cutter outline their own commands.
const NOT_HERE: [&str; 3] = ["number", "body", "cutter"];

pub(super) fn edit_tool(env: &mut Env, a: &Map<String, Value>) -> Result<Value, String> {
    let name = req_str(a, "name")?;
    let Some(Value::Object(patch)) = a.get("patch") else {
        return Err("\"patch\" must be an object of tool fields".into());
    };
    let entity = model::find_by_name(&env.world, name).ok_or_else(|| format!("no tool named '{name}'"))?;
    let prev = model::tool_of(&env.world, &entity).ok_or_else(|| format!("'{name}' is not a tool"))?;
    if let Some(k) = NOT_HERE.iter().find(|k| patch.contains_key(**k)) {
        return Err(format!("bad tool patch: \"{k}\" is not edited by edit_tool - tool {name} keeps its {k}"));
    }
    let mut def = project::tool_to_json(&prev);
    merge(&mut def, &Value::Object(patch.clone()));
    let next = project::tool_from_json(&def).map_err(|e| format!("bad tool patch: {e}"))?;
    // serde drops keys it does not know; a dropped edit reads as done.
    let Value::Object(back) = project::tool_to_json(&next) else {
        return Err("the patched tool could not be read back".into());
    };
    let mut checked = patch.clone();
    // Written only when false, so a `true` leaves no trace to find.
    if checked.get("center_cutting") == Some(&Value::Bool(true)) {
        checked.remove("center_cutting");
    }
    // #1152: an electrode's outline is written only when it states one,
    // so an empty one (back to a rectangular face) leaves no trace.
    if checked.get("outline").and_then(Value::as_array).is_some_and(Vec::is_empty) {
        checked.remove("outline");
    }
    if let Some(path) = lost(&checked, &back, "") {
        return Err(format!("bad tool patch: unknown field {path} - a tool definition has no field \"{path}\""));
    }
    let cmd = ChangeTool::new(&env.world, name, next).map_err(|e| e.to_string())?;
    env.history.execute(&env.world, cmd);
    let now = model::tool_of(&env.world, &entity).map(|t| project::tool_to_json(&t));
    Ok(json!({ "tool": name, "definition": now }))
}
