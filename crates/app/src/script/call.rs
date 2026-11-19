//! `cam.call(tool, args?)` (#2393): execute an undoable MCP tool on the
//! script run's document and command history.
//!
//! Provenance: our own bridge over `crate::mcp::call` and `crate::script::Env`.

use super::{arg_str, Env};
use serde_json::Value;

/// `cam.call(tool, args?)` - run an undoable MCP tool on the run's
/// document and command history so `.camscript` jobs can construct
/// setups, stock, tools, and operations through the same seam as MCP.
pub(crate) fn v_call(env: &mut Env, args: Vec<Value>) -> Result<Value, String> {
    if args.len() > 2 {
        return Err(format!(
            "call takes at most two arguments (tool, args), got {}",
            args.len()
        ));
    }
    let tool = arg_str(&args, 0, "a tool name")?;
    let tool_args = match args.get(1) {
        None | Some(Value::Null) => Value::Object(Default::default()),
        Some(v @ Value::Object(_)) => v.clone(),
        Some(other) => {
            return Err(format!(
                "arguments for \"{tool}\" must be an object, got {other}"
            ));
        }
    };
    crate::mcp::call(env, &tool, &tool_args)
}
