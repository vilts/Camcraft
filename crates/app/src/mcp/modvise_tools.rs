//! The MCP mod-vise tools (#820, #832): place a modular vise - a fixed
//! and an adjustable jaw block - on a fixture plate's grid, fitted to
//! the setup's stock; change a placed block's figures; re-fit the
//! adjustable block after the stock changed. Each is the very undoable
//! command the Fixtures page queues (`PlaceModVisePair`, `SetModJaw`,
//! `RefitModVise`), one History step, so one `undo` takes it back.
//!
//! Provenance: our own tool surface over this repo's mod-vise command.

use serde_json::{Map, Value, json};

use super::setup_tools::{cmd_err, loc_view, obj};
use super::tools::{Tool, req_str};
use crate::commands::{PlaceModVisePair, RefitModVise, SetModJaw};
use crate::{fixture, model};
use crate::modvise::{self, JawInsert, JawParams};
use crate::script::{Env, fnum};

pub(super) const TOOLS: &[Tool] = &[
    Tool {
        name: "add_mod_vise_pair",
        description: "Place a modular plate-mounted vise on a fixture plate: two jaw blocks, each bolted to grid holes. The FIXED block goes on `hole` (its anchor bolt), turned `yaw` (0, 90, 180 or 270 degrees; at 0 its jaw faces the plate's +Y). The ADJUSTABLE block, turned half a turn, goes on the same grid line at the hole nearest the setup's stock that leaves it clear, then slides along its slots until its jaw touches the stock. Refused by name: stock out of the slots' reach, a block off the plate, a plate with no grid, no stock, stock in the fixed jaw or beside the jaws. Generic preset sizes with optional overrides (mm); both blocks ride the plate. One undoable step.",
        schema: s_add,
        run: add_mod_vise_pair,
    },
    Tool {
        name: "set_mod_jaw",
        description: "Change a placed mod vise jaw block's figures (mm) or its jaw inserts; an adjustable block's `slide` too. The block stays on its hole and turn and its solid is regenerated. Refused by name: not a jaw block, a figure that breaks the block law, a bolt or pin off the grid where it stands, the block hanging off its plate, its jaw face pushed into the stock. A jaw lifted off the stock comes back as `warning`. One undoable step.",
        schema: s_set,
        run: set_mod_jaw,
    },
    Tool {
        name: "refit_mod_vise",
        description: "Re-fit a mod vise pair's ADJUSTABLE block to the setup's stock as it stands now (after the stock changed): the fixed block stays, the adjustable block moves to the hole nearest the stock that clears it and slides until its jaw touches the stock. `fixed` names its fixed block when its plate carries more than one. Refused by name, with nothing moved: stock out of the slots' reach, stock in the fixed jaw or beside the jaws, no stock. One undoable step.",
        schema: s_refit,
        run: refit_mod_vise,
    },
];

const DIMS: [&str; 9] =
    ["body_length", "body_width", "body_height", "jaw_width", "jaw_height", "jaw_thickness", "face_offset", "bolt_diameter", "slot_travel"];

fn points(v: &Value, k: &str) -> Result<Vec<[f64; 2]>, String> {
    let bad = || format!("\"{k}\" must be a list of [x, y] mm positions");
    v.as_array()
        .ok_or_else(bad)?
        .iter()
        .map(|p| match p.as_array().map(Vec::as_slice) {
            Some([x, y]) => Ok([x.as_f64().ok_or_else(bad)?, y.as_f64().ok_or_else(bad)?]),
            _ => Err(bad()),
        })
        .collect()
}

/// `p` with the overrides of object `key` in `a` applied.
fn with_overrides(mut p: JawParams, a: &Map<String, Value>, key: &str) -> Result<JawParams, String> {
    let Some(o) = a.get(key) else { return Ok(p) };
    let o = o.as_object().ok_or_else(|| format!("\"{key}\" must be an object of overrides"))?;
    for (k, v) in o {
        match k.as_str() {
            "bolts" => p.bolts = points(v, k)?,
            "pins" => p.pins = points(v, k)?,
            _ => {
                let slot = match k.as_str() {
                    "body_length" => &mut p.body_length,
                    "body_width" => &mut p.body_width,
                    "body_height" => &mut p.body_height,
                    "jaw_width" => &mut p.jaw_width,
                    "jaw_height" => &mut p.jaw_height,
                    "jaw_thickness" => &mut p.jaw_thickness,
                    "face_offset" => &mut p.face_offset,
                    "bolt_diameter" => &mut p.bolt_diameter,
                    "slot_travel" => &mut p.slot_travel,
                    "slide" if key == "figures" => &mut p.slide,
                    other => return Err(format!("unknown jaw block figure \"{other}\" (one of {}, bolts, pins)", DIMS.join(", "))),
                };
                *slot = v.as_f64().ok_or_else(|| format!("\"{k}\" must be a number of mm"))?;
            }
        }
    }
    Ok(p)
}

fn view(p: &JawParams) -> Value {
    json!({
        "size": p.size, "side": p.side.word(), "insert": if p.insert == JawInsert::Soft { "soft" } else { "hard" },
        "body_length": fnum(p.body_length), "body_width": fnum(p.body_width), "body_height": fnum(p.body_height),
        "jaw_width": fnum(p.jaw_width), "jaw_height": fnum(p.jaw_height), "jaw_thickness": fnum(p.jaw_thickness),
        "face_offset": fnum(p.face_offset), "slot_travel": fnum(p.slot_travel), "slide": fnum(p.slide),
        "bolts": p.bolts, "pins": p.pins,
    })
}

fn add_mod_vise_pair(env: &mut Env, a: &Map<String, Value>) -> Result<Value, String> {
    let plate = req_str(a, "plate")?;
    let hole = req_str(a, "hole")?;
    let label = req_str(a, "size")?;
    let row = modvise::preset(label).ok_or_else(|| {
        let all: Vec<&str> = modvise::PRESETS.iter().map(|s| s.label).collect();
        format!("no mod vise preset '{label}' (one of {})", all.join(", "))
    })?;
    let yaw = match a.get("yaw") {
        None | Some(Value::Null) => 0.0,
        Some(v) => v.as_f64().ok_or("\"yaw\" must be degrees")?,
    };
    let insert = insert_of(a)?.unwrap_or(JawInsert::Hard);
    let mut fixed = with_overrides(row.fixed(), a, "fixed")?;
    let mut adjustable = with_overrides(row.adjustable(), a, "adjustable")?;
    fixed.insert = insert;
    adjustable.insert = insert;
    let cmd = PlaceModVisePair::new(&env.world, plate, &fixed, &adjustable, hole, yaw).map_err(cmd_err)?;
    let (f, j) = cmd.names();
    let (f, j) = (f.to_owned(), j.to_owned());
    let fit = cmd.fit().clone();
    let (lf, la) = cmd.locations();
    env.history.execute(&env.world, cmd);
    let adj_view = view(&adjustable.with_slide(fit.slide));
    Ok(json!({
        "plate": plate,
        "fixed": { "fixture": f, "at": fit.fixed_at, "yaw": fnum(fit.yaw_deg), "location": loc_view(&lf), "block": view(&fixed) },
        "adjustable": { "fixture": j, "at": fit.adjustable_at, "yaw": fnum(fit.yaw_deg + 180.0), "location": loc_view(&la), "block": adj_view },
        "slide": fnum(fit.slide),
        "opening": fnum(fit.opening),
        "fixed_gap": fnum(fit.fixed_gap),
    }))
}

fn s_add() -> Value {
    let sizes: Vec<&str> = modvise::PRESETS.iter().map(|s| s.label).collect();
    let block = |what: &str| {
        let mut props: Map<String, Value> = DIMS.iter().map(|d| ((*d).to_owned(), json!({ "type": "number", "description": "mm" }))).collect();
        let pts = json!({ "type": "array", "items": { "type": "array", "items": {"type": "number"}, "minItems": 2, "maxItems": 2 },
            "description": "[x, y] mm in the block frame (origin = the anchor bolt, +Y toward the jaw face); one bolt is [0, 0]." });
        props.insert("bolts".into(), pts.clone());
        props.insert("pins".into(), pts);
        json!({ "type": "object", "properties": props, "additionalProperties": false, "description": format!("Override the {what} block's figures (mm).") })
    };
    obj(
        json!({
            "plate": { "type": "string", "description": "The fixture plate's name (a built-in plate or one with a detected grid)." },
            "hole": { "type": "string", "description": "The fixed block's anchor hole, e.g. C3." },
            "size": { "type": "string", "enum": sizes, "description": "The preset pair (generic figures for a grid)." },
            "yaw": { "type": "number", "description": "The fixed block's turn about the plate's Z: 0, 90, 180 or 270 degrees (default 0: its jaw faces +Y)." },
            "insert": { "type": "string", "enum": ["hard", "soft"], "description": "The jaw inserts (the same slab to the collision check)." },
            "fixed": block("fixed"),
            "adjustable": block("adjustable")
        }),
        &["plate", "hole", "size"],
    )
}

/// The `insert` argument: hard or soft, or not given.
fn insert_of(a: &Map<String, Value>) -> Result<Option<JawInsert>, String> {
    match a.get("insert") {
        None | Some(Value::Null) => Ok(None),
        Some(v) => match v.as_str() {
            Some("hard") => Ok(Some(JawInsert::Hard)),
            Some("soft") => Ok(Some(JawInsert::Soft)),
            _ => Err(format!("\"insert\" is hard or soft (got {v})")),
        },
    }
}

fn set_mod_jaw(env: &mut Env, a: &Map<String, Value>) -> Result<Value, String> {
    let name = req_str(a, "name")?;
    let current = model::find_by_name(&env.world, name)
        .and_then(|e| fixture::def_of(&env.world, &e))
        .and_then(|d| d.mod_jaw)
        .ok_or_else(|| format!("'{name}' is not a mod vise jaw block"))?;
    let mut params = with_overrides(current, a, "figures")?;
    if let Some(i) = insert_of(a)? {
        params.insert = i;
    }
    let cmd = SetModJaw::new(&env.world, name, params).map_err(cmd_err)?;
    let block = cmd.params().map(view);
    let warning = cmd.warning().map(str::to_owned);
    env.history.execute(&env.world, cmd);
    Ok(json!({ "fixture": name, "block": block, "warning": warning }))
}

fn refit_mod_vise(env: &mut Env, a: &Map<String, Value>) -> Result<Value, String> {
    let adjustable = req_str(a, "adjustable")?;
    let cmd = match a.get("fixed") {
        None | Some(Value::Null) => RefitModVise::to_partner(&env.world, adjustable),
        Some(v) => RefitModVise::new(&env.world, v.as_str().ok_or("\"fixed\" must be a jaw block's name")?, adjustable),
    }
    .map_err(cmd_err)?;
    let fit = cmd.fit().clone();
    let loc = cmd.location();
    let warning = cmd.warning().map(str::to_owned);
    env.history.execute(&env.world, cmd);
    Ok(json!({
        "fixture": adjustable,
        "at": fit.adjustable_at,
        "yaw": fnum(fit.yaw_deg + 180.0),
        "location": loc_view(&loc),
        "slide": fnum(fit.slide),
        "opening": fnum(fit.opening),
        "fixed_gap": fnum(fit.fixed_gap),
        "warning": warning,
    }))
}

fn s_set() -> Value {
    let mut props: Map<String, Value> = DIMS.iter().map(|d| ((*d).to_owned(), json!({ "type": "number", "description": "mm" }))).collect();
    props.insert("slide".into(), json!({ "type": "number", "description": "mm the adjustable block has slid along its slots (0 to its slot travel)" }));
    let pts = json!({ "type": "array", "items": { "type": "array", "items": {"type": "number"}, "minItems": 2, "maxItems": 2 },
        "description": "[x, y] mm in the block frame (origin = the anchor bolt, +Y toward the jaw face); one bolt is [0, 0]." });
    props.insert("bolts".into(), pts.clone());
    props.insert("pins".into(), pts);
    obj(
        json!({
            "name": { "type": "string", "description": "The jaw block fixture's name." },
            "figures": { "type": "object", "properties": props, "additionalProperties": false, "description": "The figures to change (mm); the rest stay." },
            "insert": { "type": "string", "enum": ["hard", "soft"], "description": "The jaw insert (the same slab to the collision check)." }
        }),
        &["name"],
    )
}

fn s_refit() -> Value {
    obj(
        json!({
            "adjustable": { "type": "string", "description": "The adjustable jaw block's name." },
            "fixed": { "type": "string", "description": "Its fixed jaw block (default: the one fixed block on its plate)." }
        }),
        &["adjustable"],
    )
}
