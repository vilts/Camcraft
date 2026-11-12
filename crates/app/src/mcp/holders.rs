//! `set_tool_holder` and `list_holder_library` (#799): set a document
//! tool's holder from the built-in holder library, from a drawn spun
//! profile, or back to the holder cylinder - through the same undoable
//! `SetToolBody` command the tool editor uses, so one call is one undo
//! step. Bad input is refused by name before anything enters history.
//!
//! Provenance: our own tool surface over `camcraft_tools::holder_library`
//! (#793, #792) and `SetToolBody` (#93); no vendor material consulted.

use super::tools::req_str;
use crate::commands::SetToolBody;
use crate::model;
use crate::script::{fnum, Env};
use crate::tool_body;
use camcraft_tools::holder_library::{self, Clamp};
use camcraft_tools::spun::SpunPoint;

use serde_json::{json, Map, Value};

/// A tool's holder as `get_job` and `set_tool_holder` state it: `null`
/// for the plain holder cylinder, else the drawn profile's points.
pub(super) fn holder_view(world: &flecs_ecs::prelude::World, tool: &str) -> Value {
    let body = model::find_by_name(world, tool).and_then(|e| tool_body::body_of(world, &e));
    match body {
        None => Value::Null,
        Some(b) => json!({
            "source": b.source,
            "lathe": matches!(b.frame, tool_body::BodyFrame::Lathe { .. }),
            "points": b.spun.as_ref().map(|s| s.points().iter()
                .map(|p| json!({ "z": fnum(p.z), "r": fnum(p.r), "arc": fnum(p.arc) })).collect::<Vec<_>>()),
        }),
    }
}

fn mm(a: &Map<String, Value>, key: &str) -> Result<Option<f64>, String> {
    match a.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(v) => match v.as_f64() {
            Some(x) if x.is_finite() && x >= 0.0 => Ok(Some(x)),
            Some(x) => Err(format!("\"{key}\" must be a finite length of 0 mm or more, got {x}")),
            None => Err(format!("\"{key}\" must be a number (mm), got {v}")),
        },
    }
}

fn points_of(v: &Value) -> Result<Vec<SpunPoint>, String> {
    let list = v.as_array().ok_or_else(|| format!("\"points\" must be an array of {{z, r, arc}}, got {v}"))?;
    if list.is_empty() {
        return Err("\"points\" is empty - give the profile's points, or use \"clear\": true".into());
    }
    list.iter()
        .enumerate()
        .map(|(i, p)| {
            let num = |k: &str, dflt: Option<f64>| match p.get(k) {
                None | Some(Value::Null) => dflt.ok_or_else(|| format!("\"points\"[{i}] needs \"{k}\"")),
                Some(x) => x.as_f64().filter(|x| x.is_finite()).ok_or_else(|| format!("\"points\"[{i}].{k} must be a finite number, got {x}")),
            };
            Ok(SpunPoint { z: num("z", None)?, r: num("r", None)?, arc: num("arc", Some(0.0))? })
        })
        .collect()
}

pub(super) fn set_tool_holder(env: &mut Env, a: &Map<String, Value>) -> Result<Value, String> {
    let tool = req_str(a, "tool")?;
    let library = match a.get("library") {
        None | Some(Value::Null) => None,
        Some(Value::String(s)) if !s.is_empty() => Some(s.as_str()),
        Some(v) => return Err(format!("\"library\" must be a holder library name (see list_holder_library), got {v}")),
    };
    let points = a.get("points").filter(|v| !v.is_null());
    let clear = match a.get("clear") {
        None | Some(Value::Null) | Some(Value::Bool(false)) => false,
        Some(Value::Bool(true)) => true,
        Some(v) => return Err(format!("\"clear\" must be true or false, got {v}")),
    };
    let given = usize::from(library.is_some()) + usize::from(points.is_some()) + usize::from(clear);
    if given != 1 {
        return Err(format!("give exactly one of \"library\", \"points\" or \"clear\": true (got {given})"));
    }
    let (gauge, projection) = (mm(a, "gauge_len")?, mm(a, "projection")?);
    if (gauge.is_some() || projection.is_some()) && library.is_none() {
        return Err("\"gauge_len\" and \"projection\" go with \"library\"".into());
    }
    let cmd = if let Some(entry) = library {
        SetToolBody::library(&env.world, tool, entry, gauge, projection)
    } else if let Some(p) = points {
        SetToolBody::spun(&env.world, tool, Some(points_of(p)?))
    } else {
        SetToolBody::spun(&env.world, tool, None)
    }
    .map_err(|e| e.to_string())?;
    env.history.execute(&env.world, cmd);
    Ok(json!({ "tool": tool, "holder": holder_view(&env.world, tool) }))
}

pub(super) fn list_holder_library(_env: &mut Env, _a: &Map<String, Value>) -> Result<Value, String> {
    let entries: Vec<Value> = holder_library::library()
        .iter()
        .map(|e| {
            let takes = match e.clamp {
                Clamp::Er(er) => json!({ "kind": "ER", "min_shank": er.min_bore, "max_shank": er.max_bore, "nut_d": er.nut_d, "nut_len": er.nut_len }),
                Clamp::Weldon { bore, body_d } => json!({ "kind": "Weldon", "shank": bore, "body_d": body_d }),
            };
            json!({
                "name": e.name(),
                "taper": e.taper.map(|t| json!({ "name": t.name, "gauge_d": t.gauge_d, "flange_d": t.flange_d })),
                "clamp": takes,
                "default_gauge_len": e.gauge_len,
                "min_gauge_len": fnum(e.min_gauge()),
                "max_gauge_len": holder_library::MAX_GAUGE,
            })
        })
        .collect();
    Ok(json!({ "count": entries.len(), "entries": entries }))
}

pub(super) fn s_set_tool_holder() -> Value {
    json!({
        "type": "object",
        "properties": {
            "tool": { "type": "string", "description": "A document tool name (see get_job)." },
            "library": { "type": "string", "description": "A list_holder_library entry name, e.g. \"BT30 ER20\"." },
            "gauge_len": { "type": "number", "minimum": 0, "description": "With library: gauge line to nose face, mm (default: the entry's)." },
            "projection": { "type": "number", "minimum": 0, "description": "With library: nose face above the tool tip, mm (default: the tool's flute plus shank length)." },
            "points": { "type": "array", "description": "A drawn spun holder profile, tip-up: [{z, r, arc}] (mm; arc 0 = line, else the signed arc radius to the next point).",
                "items": { "type": "object", "properties": { "z": { "type": "number" }, "r": { "type": "number" }, "arc": { "type": "number" } }, "required": ["z", "r"] } },
            "clear": { "type": "boolean", "description": "true: back to the plain holder cylinder." }
        },
        "required": ["tool"],
        "additionalProperties": false,
    })
}
