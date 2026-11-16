//! The MCP box pick (#979): the viewport's Ctrl+drag box select, headless.
//! A world X/Y box seen from straight above picks every visible feature
//! edge chain (or welded face) in it - window or crossing - and adds them
//! to an operation's picks through `SetOperationGeometry`: one undo step,
//! the #692 one pick rule (feature chains only; nothing hidden unless
//! `through`, the viewport's Pick through toggle, asks for it).
//!
//! Provenance: our own tool surface over this repo's pick commands.

use serde_json::{Map, Value, json};

use super::edit::find_op;
use super::pick::{geometry_view, kind_door, picks_view};
use super::tools::{Tool, opt_str, req_str};
use crate::commands::SetOperationGeometry;
use crate::model;
use crate::operation::{GeoSel, SrcRef};
use crate::pick::{BoxMode, PickKinds, box_select_top};
use crate::script::Env;

pub(super) const TOOLS: &[Tool] = &[Tool {
    name: "pick_in_box",
    description: "Box-select features into an operation's picks, seen from straight above (+Z, like the Top view): every visible feature edge chain (kind \"chains\", the default) or face (kind \"faces\") whose outline lies inside the world X/Y box (mode \"window\", the default) or touches it (mode \"crossing\"). Without \"through\" the box is visible-only: hidden edges and faces (under the top face) are left out; \"through\": true takes them too, like the viewport's Pick through toggle. Mesh triangulation edges are never taken. Adds to the current picks unless replace is true. One undo step.",
    schema: s_pick_in_box,
    run: pick_in_box,
}];

fn num(v: Option<&Value>, k: &str) -> Result<f64, String> {
    v.and_then(Value::as_f64)
        .filter(|x| x.is_finite())
        .ok_or_else(|| format!("\"box\" needs a finite number \"{k}\" (mm)"))
}

fn pick_in_box(env: &mut Env, a: &Map<String, Value>) -> Result<Value, String> {
    let key = find_op(env, req_str(a, "operation")?)?;
    let Some(Value::Object(b)) = a.get("box") else {
        return Err(String::from("\"box\" is required: {\"x0\", \"y0\", \"x1\", \"y1\"} in mm"));
    };
    let corners = [[num(b.get("x0"), "x0")?, num(b.get("y0"), "y0")?], [num(b.get("x1"), "x1")?, num(b.get("y1"), "y1")?]];
    if corners[0][0] == corners[1][0] || corners[0][1] == corners[1][1] {
        return Err(String::from("the box has no area - give two opposite corners"));
    }
    let mode = match opt_str(a, "mode")?.unwrap_or("window") {
        "window" => BoxMode::Window,
        "crossing" => BoxMode::Crossing,
        m => return Err(format!("unknown mode '{m}' - \"window\" or \"crossing\"")),
    };
    let (kinds, what) = match opt_str(a, "kind")?.unwrap_or("chains") {
        "chains" => (PickKinds::EDGES, "edge chain"),
        "faces" => (PickKinds::FACES, "face"),
        k => return Err(format!("unknown kind '{k}' - \"chains\" or \"faces\"")),
    };
    let replace = match a.get("replace") {
        None | Some(Value::Null) => false,
        Some(Value::Bool(r)) => *r,
        Some(v) => return Err(format!("\"replace\" must be true or false, got {v}")),
    };
    let through = match a.get("through") {
        None | Some(Value::Null) => false,
        Some(Value::Bool(t)) => *t,
        Some(v) => return Err(format!("\"through\" must be true or false, got {v}")),
    };
    let found = box_select_top(&env.world, corners, mode, kinds, through);
    if found.is_empty() {
        let seen = if through { "" } else { "visible " };
        return Err(format!("no {seen}{what} in the box - nothing picked, nothing changed"));
    }
    let e = model::find_by_name(&env.world, &key).ok_or_else(|| format!("no operation named '{key}'"))?;
    let op = model::operation_of(&env.world, &e).ok_or_else(|| format!("'{key}' is not an operation"))?;
    let mut items: Vec<GeoSel> = match (&op.src, replace) {
        (SrcRef::Selection(items), false) => items.clone(),
        _ => Vec::new(),
    };
    let before = items.len();
    for g in &found {
        if !items.contains(g) {
            items.push(g.clone());
        }
    }
    let added = items.len() - before;
    if added == 0 && !replace {
        return Err(format!("every {what} in the box is already picked in '{key}' - nothing changed"));
    }
    kind_door(op.params.kind().info().label, &op.params, &items)?;
    let cmd = SetOperationGeometry::new(&env.world, &key, items).map_err(|e| e.to_string())?;
    env.history.execute(&env.world, cmd);
    let mut view = geometry_view(env, &key)?;
    if let Value::Object(m) = &mut view {
        m.insert("boxed".into(), json!(picks_view(&found)));
        m.insert("added".into(), json!(added));
    }
    Ok(view)
}

fn s_pick_in_box() -> Value {
    json!({
        "type": "object",
        "properties": {
            "operation": { "type": "string", "description": "Operation name (key or label)." },
            "box": {
                "type": "object",
                "description": "Two opposite corners of the box in world X/Y, mm, seen from above.",
                "properties": {
                    "x0": { "type": "number" }, "y0": { "type": "number" },
                    "x1": { "type": "number" }, "y1": { "type": "number" }
                },
                "required": ["x0", "y0", "x1", "y1"],
                "additionalProperties": false
            },
            "mode": { "type": "string", "enum": ["window", "crossing"], "description": "window: wholly inside (default); crossing: inside or touching." },
            "kind": { "type": "string", "enum": ["chains", "faces"], "description": "What to pick: edge chains (default) or faces." },
            "replace": { "type": "boolean", "description": "Replace the current picks instead of adding to them (default false)." },
            "through": { "type": "boolean", "description": "Pick through: also take hidden edges/faces (default false = visible only, as the viewport with Pick through off)." }
        },
        "required": ["operation", "box"],
        "additionalProperties": false
    })
}
