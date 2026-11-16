//! The MCP units tool (#902): set the document's unit - the display
//! lens and the unit every post writes the program in (G20 / G21 or
//! the dialect's word). It is the very undoable command the Post and
//! Stock pages queue (`SetUnits`), one History step, so `undo` puts
//! the previous unit back; `get_job` reports the unit in force.
//!
//! Provenance: our own tool surface over this repo's SetUnits command.

use serde_json::{Map, Value, json};

use super::setup_tools::{cmd_err, obj};
use super::tools::{Tool, req_str};
use crate::commands::SetUnits;
use crate::posting;
use crate::script::Env;
use crate::units::{self, Unit};

pub(super) const TOOLS: &[Tool] = &[Tool {
    name: "set_units",
    description: "Set the document's unit, \"mm\" or \"inch\": every number the app shows, and the unit every post writes the program in (G21 / G20, or the control's own word). One undoable step. Refused by name: the unit the document already has, any other word. A post that cannot write inch (the Makera Carvera) refuses an inch document when posting.",
    schema: s_set_units,
    run: set_units,
}];

/// The unit word `get_job` and this tool report.
pub(super) fn word(world: &flecs_ecs::prelude::World) -> &'static str {
    posting::unit_word(units::of(world))
}

fn set_units(env: &mut Env, a: &Map<String, Value>) -> Result<Value, String> {
    let next = match req_str(a, "unit")? {
        "mm" => Unit::Millimeters,
        "inch" => Unit::Inches,
        other => return Err(format!("\"unit\" must be \"mm\" or \"inch\", not \"{other}\"")),
    };
    let cmd = SetUnits::new(&env.world, next).map_err(cmd_err)?;
    env.history.execute(&env.world, cmd);
    Ok(json!({ "unit": word(&env.world) }))
}

fn s_set_units() -> Value {
    obj(
        json!({ "unit": { "type": "string", "enum": ["mm", "inch"], "description": "The document's unit: display and posted program." } }),
        &["unit"],
    )
}
