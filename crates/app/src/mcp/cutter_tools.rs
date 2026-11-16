//! The MCP drawn-cutter tools (#93 slice 2): read a milling tool's
//! cutter, neck and shank outline (drawn, or the one its numbers draw)
//! and set or clear the drawn one - the very `SetToolCutter` the Tool
//! editor's Profile tab queues, one History step, so `undo` takes it back.
//!
//! #93 slice 3: `set_tool_cutter_params` edits the tool by its
//! parameters inside the outline - the very `ReshapeTool` the editor's
//! Apply queues: the numbers and the drawn cutter carried onto them (or
//! a neck drawn), one History step.
//!
//! Provenance: our own tool surface over this repo's tool commands.

use super::setup_tools::{cmd_err, obj};
use super::tools::{Tool, req_str};
use crate::commands::{ReshapeTool, SetToolCutter};
use crate::cutter_fit::{self, Neck};
use crate::model;
use crate::script::{Env, fnum};
use crate::spun::{SpunPoint, SpunProfile};
use crate::tool_cutter;

use serde_json::{Map, Value, json};

pub(super) const TOOLS: &[Tool] = &[
    Tool { name: "get_tool_cutter_profile", description: "A milling tool's cutter, neck and shank as a spun half-outline: points [z, r, arc] from the tip (z = 0, on the axis) up to the shank top, arc = 0 for a straight segment else the signed radius of an arc to the next point (positive bulges away from the axis). \"drawn\" says whether it is a hand-drawn outline or the one the tool's numbers draw. Turning tools have none (they are not spun profiles).", schema: s_get, run: get_cutter },
    Tool { name: "set_tool_cutter_params", description: "Edit a milling tool by the parameters its cutter outline is drawn from - any of diameter, corner_r, tip_angle (drill point / chamfer cone, degrees), flute_len, shank_d, shank_len (mm) - and optionally its neck. A drawn outline is carried onto the new numbers (the tip regenerated from the kind, a drawn neck kept above the new flute top, the shank top moved), so outline and numbers keep agreeing. \"neck\": {diameter, length} draws a straight neck from the flute top; null drops the drawn outline (back to the numbers' own); omitted keeps whatever is drawn. Every tool-edit and cutter refusal by name. One undoable step for numbers and outline together.", schema: s_params, run: set_params },
    Tool { name: "set_tool_cutter_profile", description: "Set a milling tool's drawn cutter outline (points [z, r, arc] tip-up, closed to the axis at both ends), or clear it with points null (back to the outline its numbers draw). It must agree with the tool's numbers: start at the tip, reach exactly the stated diameter within the flute length, end at flute + shank length, never be wider than the cutter or shank above the flutes, not fold over itself - else refused by name. Toolpaths keep reading the numbers; the collision checks and the simulation read a drawn neck where it is narrower than the stated shank (never wider). One undoable step.", schema: s_set, run: set_cutter },
];

fn tool_of(env: &Env, name: &str) -> Result<(crate::tools::Tool, Option<SpunProfile>), String> {
    let e = model::find_by_name(&env.world, name).ok_or_else(|| format!("no tool named '{name}'"))?;
    let t = model::tool_of(&env.world, &e).ok_or_else(|| format!("'{name}' is not a tool"))?;
    Ok((t, tool_cutter::cutter_of(&env.world, &e)))
}

fn view(name: &str, tool: &crate::tools::Tool, drawn: Option<&SpunProfile>) -> Value {
    let shown = tool_cutter::shown(tool, drawn);
    let points: Option<Vec<Value>> =
        shown.as_ref().map(|s| s.points().iter().map(|p| json!([fnum(p.z), fnum(p.r), fnum(p.arc)])).collect());
    let neck = shown.as_ref().and_then(|s| cutter_fit::neck_of(tool, s.points())).map(|n| json!({ "diameter": fnum(n.d), "length": fnum(n.len) }));
    json!({ "tool": name, "drawn": drawn.is_some(), "points": points, "neck": neck })
}

fn get_cutter(env: &mut Env, a: &Map<String, Value>) -> Result<Value, String> {
    let name = req_str(a, "tool")?;
    let (t, drawn) = tool_of(env, name)?;
    Ok(view(name, &t, drawn.as_ref()))
}

fn set_cutter(env: &mut Env, a: &Map<String, Value>) -> Result<Value, String> {
    let name = req_str(a, "tool")?;
    let points = match a.get("points") {
        None => return Err("\"points\" is required (null clears the drawn outline)".to_owned()),
        Some(Value::Null) => None,
        Some(v) => Some(read_points(v)?),
    };
    let cmd = SetToolCutter::new(&env.world, name, points).map_err(cmd_err)?;
    env.history.execute(&env.world, cmd);
    let (t, drawn) = tool_of(env, name)?;
    Ok(view(name, &t, drawn.as_ref()))
}

/// The shape numbers `set_tool_cutter_params` takes, mm (tip_angle deg).
const PARAMS: [&str; 6] = ["diameter", "corner_r", "tip_angle", "flute_len", "shank_d", "shank_len"];

fn set_params(env: &mut Env, a: &Map<String, Value>) -> Result<Value, String> {
    let name = req_str(a, "tool")?;
    let (old, drawn) = tool_of(env, name)?;
    let mut next = old.clone();
    let empty = Map::new();
    let params = match a.get("params") {
        None | Some(Value::Null) => &empty,
        Some(Value::Object(m)) => m,
        Some(_) => return Err("\"params\" must be an object of numbers".to_owned()),
    };
    for (k, v) in params {
        let x = v.as_f64().ok_or_else(|| format!("params.{k} must be a number"))?;
        match k.as_str() {
            "diameter" => next.diameter = x,
            "corner_r" => next.corner_r = x,
            "tip_angle" => next.tip_angle = x,
            "flute_len" => next.flute_len = x,
            "shank_d" => next.shank_d = x,
            "shank_len" => next.shank_len = x,
            _ => return Err(format!("params.{k} is not a cutter parameter (one of {})", PARAMS.join(", "))),
        }
    }
    let points = match a.get("neck") {
        None => drawn.as_ref().map(|s| cutter_fit::refit(&old, &next, s.points())),
        Some(Value::Null) => None,
        Some(Value::Object(m)) => {
            let n = |k: &str| m.get(k).and_then(Value::as_f64).ok_or_else(|| format!("neck.{k} must be a number"));
            let neck = Neck { d: n("diameter")?, len: n("length")? };
            Some(cutter_fit::necked(&next, neck).map_err(|e| format!("tool '{name}': neck: {e}"))?)
        }
        Some(_) => return Err("\"neck\" must be {diameter, length} or null".to_owned()),
    };
    let cmd = ReshapeTool::new(&env.world, name, next, points).map_err(cmd_err)?;
    env.history.execute(&env.world, cmd);
    let (t, drawn) = tool_of(env, name)?;
    Ok(view(name, &t, drawn.as_ref()))
}

/// `[[z, r, arc?], ...]` - arc defaults to 0 (a straight segment).
fn read_points(v: &Value) -> Result<Vec<SpunPoint>, String> {
    let rows = v.as_array().ok_or("\"points\" must be an array of [z, r, arc] rows")?;
    if rows.len() > crate::spun::MAX_POINTS {
        return Err(format!("\"points\" holds more than {} rows", crate::spun::MAX_POINTS));
    }
    rows.iter()
        .enumerate()
        .map(|(i, row)| {
            let n: Option<Vec<f64>> = row.as_array().map(|r| r.iter().filter_map(Value::as_f64).collect());
            match n.as_deref() {
                Some([z, r]) if row.as_array().is_some_and(|a| a.len() == 2) => Ok(SpunPoint::line(*z, *r)),
                Some([z, r, arc]) if row.as_array().is_some_and(|a| a.len() == 3) => Ok(SpunPoint { z: *z, r: *r, arc: *arc }),
                _ => Err(format!("point {} must be [z, r] or [z, r, arc] numbers", i + 1)),
            }
        })
        .collect()
}

fn s_get() -> Value {
    obj(json!({ "tool": { "type": "string", "description": "The document tool's name." } }), &["tool"])
}

fn s_set() -> Value {
    obj(
        json!({
            "tool": { "type": "string", "description": "The document tool's name (a milling tool)." },
            "points": { "type": ["array", "null"], "items": { "type": "array", "items": { "type": "number" }, "minItems": 2, "maxItems": 3 },
                "description": "The outline tip-up, mm: [z, r] or [z, r, arc] per point; null clears the drawn outline." }
        }),
        &["tool", "points"],
    )
}


fn s_params() -> Value {
    obj(
        json!({
            "tool": { "type": "string", "description": "The document tool's name (a milling tool)." },
            "params": { "type": "object", "description": "Any of diameter, corner_r, tip_angle, flute_len, shank_d, shank_len - mm, tip_angle in degrees; the others keep their value.",
                "properties": { "diameter": { "type": "number" }, "corner_r": { "type": "number" }, "tip_angle": { "type": "number" }, "flute_len": { "type": "number" }, "shank_d": { "type": "number" }, "shank_len": { "type": "number" } } },
            "neck": { "type": ["object", "null"], "description": "{diameter, length} mm: a straight neck up from the flute top, below the shank's diameter and inside its length; null drops the drawn outline; omitted keeps the drawn one, carried onto the new numbers.",
                "properties": { "diameter": { "type": "number" }, "length": { "type": "number" } } }
        }),
        &["tool"],
    )
}
