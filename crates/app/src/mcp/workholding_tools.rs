//! The MCP workholding-rpm tool (#1031): state, change or clear the rated
//! maximum rpm of a setup's chuck or collet. One `SetSetupWorkholdingRpm`
//! run through the session History, so `undo` walks it; every refusal is
//! the command's own text, returned as a tool error.
//!
//! Provenance: our own tool surface over this repo's setup command.

use super::setup_tools::{cmd_err, obj, setup_or_active};
use super::tools::Tool;
use crate::commands::SetSetupWorkholdingRpm;
use crate::script::{fnum, Env};

use serde_json::{json, Map, Value};

pub(super) const TOOLS: &[Tool] = &[Tool {
    name: "set_workholding_max_rpm",
    description: "State the rated maximum rpm of a lathe setup's chuck or collet (null clears it). Constant surface speed clamps to the lowest of the machine's, the tool's and this limit (Fanuc G50 S, Tormach G96 D); a constant-rpm turning operation over it is refused by name at the post. One undo step.",
    schema: s_workholding,
    run: set_workholding_max_rpm,
}];

fn set_workholding_max_rpm(env: &mut Env, a: &Map<String, Value>) -> Result<Value, String> {
    let next = match a.get("max_rpm") {
        None => return Err("\"max_rpm\" is required (an rpm, or null to clear)".into()),
        Some(Value::Null) => None,
        Some(Value::Number(n)) => Some(n.as_f64().ok_or_else(|| format!("\"max_rpm\" {n} is not a number"))?),
        Some(other) => return Err(format!("\"max_rpm\" must be a number or null, got {other}")),
    };
    let name = setup_or_active(env, a)?.ok_or("no setup is declared - add_setup first (the limit belongs to a setup)")?;
    let cmd = SetSetupWorkholdingRpm::new(&env.world, &name, next).map_err(cmd_err)?;
    env.history.execute(&env.world, cmd);
    let now = crate::setup_workholding::of(&env.world, Some(&name));
    Ok(json!({ "setup": name, "max_rpm": now.map(fnum) }))
}

fn s_workholding() -> Value {
    obj(
        json!({
            "max_rpm": { "type": ["number", "null"], "description": "The chuck's or collet's rated maximum rpm; null clears it." },
            "setup": { "type": "string", "description": "A declared setup; defaults to the active one." }
        }),
        &["max_rpm"],
    )
}
