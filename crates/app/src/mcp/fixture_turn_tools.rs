//! The MCP fixture-turn tool (#2174): turn a fixture about world X, Y or
//! Z through its own origin, by a quarter turn or any typed angle - the
//! same `TurnFixture` the Fixtures page's turn buttons queue, one History
//! step, so `undo` walks it. Refusals are the command's own text.
//!
//! Provenance: our own tool surface over this repo's fixture command.

use super::setup_tools::{cmd_err, loc_view, obj};
use super::tools::{req_str, Tool};
use crate::commands::TurnFixture;
use crate::model;

use serde_json::{json, Map, Value};

pub(super) const TOOLS: &[Tool] = &[Tool {
    name: "turn_fixture",
    description: "Turn a fixture about world axis \"x\", \"y\" or \"z\" through its own origin by \"degrees\" (90 or -90 for a quarter turn; + is counter-clockwise looking down the axis). Plate riders turn with it; a part or stock it held stays and is named in \"warning\". The post gate checks toolpaths against the turned fixture. One undo step.",
    schema: s_turn_fixture,
    run: turn_fixture,
}];

fn turn_fixture(env: &mut crate::script::Env, a: &Map<String, Value>) -> Result<Value, String> {
    let name = req_str(a, "name")?;
    let axis = match req_str(a, "axis")?.to_ascii_lowercase().as_str() {
        "x" => 0,
        "y" => 1,
        "z" => 2,
        other => return Err(format!("\"axis\" is \"x\", \"y\" or \"z\", not \"{other}\"")),
    };
    let degrees = a
        .get("degrees")
        .and_then(Value::as_f64)
        .ok_or("\"degrees\" is required: the turn angle in degrees")?;
    let cmd = TurnFixture::new(&env.world, name, axis, degrees).map_err(cmd_err)?;
    let warning = cmd.warning().map(str::to_owned);
    env.history.execute(&env.world, cmd);
    let placed = model::find_by_name(&env.world, name).and_then(|e| model::transform_of(&env.world, &e));
    let mut out = loc_view(&placed.unwrap_or_default());
    out["fixture"] = json!(name);
    out["warning"] = json!(warning);
    Ok(out)
}

fn s_turn_fixture() -> Value {
    obj(
        json!({
            "name": { "type": "string", "description": "A fixture's name." },
            "axis": { "type": "string", "enum": ["x", "y", "z"], "description": "The world axis to turn about." },
            "degrees": { "type": "number", "description": "The turn angle in degrees; 90 or -90 is a quarter turn." },
        }),
        &["name", "axis", "degrees"],
    )
}
