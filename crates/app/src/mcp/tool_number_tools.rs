//! The MCP tool-number tool (#730): `set_tool_number` runs the very
//! undoable `SetToolNumber` command the GUI's tool editor queues (one
//! History step) and reports the number the tool posts as.
//!
//! Provenance: our own tool surface over this repo's `SetToolNumber`.

use serde_json::{Map, Value, json};

use super::setup_tools::{cmd_err, obj};
use super::tools::{Tool, req_str};
use crate::commands::SetToolNumber;
use crate::model;
use crate::script::Env;
use crate::tool_numbers;

pub(super) const TOOLS: &[Tool] = &[Tool {
    name: "set_tool_number",
    description: "Give a document tool its controller number (the T.. / H.. a post writes, which must match the machine's tool table), or pass number: null to go back to its list position. Returns the tool's own number and the number it posts as (get_job's tool rows show both). One undoable step. Refused by name: an unknown or non-tool name, a number outside 1-99999, a number another tool already has, and a no-op.",
    schema,
    run: set_tool_number,
}];

fn schema() -> Value {
    obj(
        json!({
            "tool": { "type": "string", "description": "A document tool's name." },
            "number": { "type": ["integer", "null"], "description": "The tool number, 1-99999; null clears it." },
        }),
        &["tool", "number"],
    )
}

fn set_tool_number(env: &mut Env, a: &Map<String, Value>) -> Result<Value, String> {
    let tool = req_str(a, "tool")?;
    let next = match a.get("number") {
        None => return Err("\"number\" is required: an integer, or null to clear".into()),
        Some(Value::Null) => None,
        Some(v) => {
            let n = v.as_u64().ok_or("\"number\" must be a positive integer, or null")?;
            Some(u32::try_from(n).map_err(|_| format!("tool number {n} is out of range - tool numbers run {}", tool_numbers::RANGE_TEXT))?)
        }
    };
    let cmd = SetToolNumber::new(&env.world, tool, next).map_err(cmd_err)?;
    env.history.execute(&env.world, cmd);
    Ok(number_view(env, tool))
}

/// The tool's own number and the number it posts as.
pub(super) fn number_view(env: &Env, tool: &str) -> Value {
    let own = model::find_by_name(&env.world, tool).and_then(|e| model::tool_number_of(&env.world, &e));
    json!({ "tool": tool, "number": own, "posts_as": tool_numbers::number_of(&env.world, tool) })
}
