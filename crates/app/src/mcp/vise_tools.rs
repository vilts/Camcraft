//! The MCP built-in vise tools (#788): place a generic parametric vise
//! from the size table into a setup (at a pose, or fitted to the stock),
//! and change a placed vise's jaw opening or dimensions. Each is the very
//! undoable command the Fixtures page queues (`InsertVise`, `SetVise`),
//! one History step, so `undo` takes it back. The 5-axis self-centering
//! vises (#787) are rows of the same size list, centred on the stock, with
//! their own `min_opening` / `lip_height` / `lip_depth` dimensions, and
//! the grip step's `grip_depth` / `grip_width` (#1274).
//!
//! Provenance: our own tool surface over this repo's vise commands.

use super::setup_tools::{cmd_err, loc_view, obj, opt_nums, point, setup_or_active};
use super::tools::{Tool, req_str};
use crate::commands::{InsertVise, SetVise, VisePlacement};
use crate::fixture;
use crate::geometry::IDENTITY_Q;
use crate::model;
use crate::schema::Location;
use crate::script::{Env, fnum};
use crate::vise::{self, ViseParams, ViseStyle};

use serde_json::{Map, Value, json};

pub(super) const TOOLS: &[Tool] = &[
    Tool { name: "add_builtin_vise", description: "Place a built-in generic milling vise (size table: 3-8 in, 100-200 mm jaw widths, and the 5-axis self-centering 52/75/80/125 mm rows) as a fixture of a setup - the active one by default. Either fit it to the setup's stock (standard: fixed jaw on the stock's +Y side, centred in X, floor under the stock, jaws opened to the stock's Y width; self-centering: centred on the stock in X and Y, jaws closed to its Y width within the clamping range; with a grip step - dimensions grip_depth/grip_width, not beside the lip_height/lip_depth dovetail - the stock sits on the step's ledge grip_depth below the jaw top, the frame origin is that ledge and the vise floor sits jaw_height - grip_depth lower) or place its frame (standard: origin = the fixed jaw's clamping face, centred, at the floor, jaws open toward -Y; self-centering: origin = the centre between the jaws at the floor, or at the ledge with a grip step) at a position/rotation with an opening. Optional dimension overrides in mm. One undoable step; nonsense dimensions are refused by name.", schema: s_add, run: add_builtin_vise },
    Tool { name: "set_vise", description: "Change a placed built-in vise: its jaw opening in mm, or from_stock to open the jaws to the setup's stock as the vise stands, and/or dimension overrides in mm. The mesh is regenerated; one undoable step.", schema: s_set, run: set_vise },
];

const DIMS: [&str; 15] = [
    "jaw_width", "jaw_height", "jaw_thickness", "jaw_body_depth", "max_opening", "length", "width", "base_height", "housing_length",
    "housing_height", "min_opening", "lip_height", "lip_depth", "grip_depth", "grip_width",
];

/// `p` with every dimension `dims` names replaced.
fn with_dims(mut p: ViseParams, a: &Map<String, Value>) -> Result<ViseParams, String> {
    let Some(d) = a.get("dimensions") else { return Ok(p) };
    let d = d.as_object().ok_or("\"dimensions\" must be an object of mm lengths")?;
    for (k, v) in d {
        let v = v.as_f64().ok_or_else(|| format!("dimension \"{k}\" must be a number of mm"))?;
        let slot = match k.as_str() {
            "min_opening" | "lip_height" | "lip_depth" | "grip_depth" | "grip_width" => {
                let ViseStyle::SelfCentering(sc) = &mut p.style else {
                    return Err(format!("dimension \"{k}\" is a self-centering vise's; this one is a standard vise"));
                };
                match k.as_str() {
                    "min_opening" => &mut sc.min_opening,
                    "lip_height" => &mut sc.lip_height,
                    "lip_depth" => &mut sc.lip_depth,
                    "grip_depth" => &mut sc.grip_depth,
                    _ => &mut sc.grip_width,
                }
            }
            "jaw_width" => &mut p.jaw_width,
            "jaw_height" => &mut p.jaw_height,
            "jaw_thickness" => &mut p.jaw_thickness,
            "jaw_body_depth" => &mut p.jaw_body_depth,
            "max_opening" => &mut p.max_opening,
            "length" => &mut p.length,
            "width" => &mut p.width,
            "base_height" => &mut p.base_height,
            "housing_length" => &mut p.housing_length,
            "housing_height" => &mut p.housing_height,
            other => return Err(format!("unknown vise dimension \"{other}\" (one of {})", DIMS.join(", "))),
        };
        *slot = v;
    }
    Ok(p)
}

fn opt_f64(a: &Map<String, Value>, key: &str) -> Result<Option<f64>, String> {
    match a.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(v) => v.as_f64().map(Some).ok_or_else(|| format!("\"{key}\" must be a number")),
    }
}

fn params_view(p: &ViseParams) -> Value {
    let mut v = json!({
        "size": p.size, "opening": fnum(p.opening), "max_opening": fnum(p.max_opening),
        "jaw_width": fnum(p.jaw_width), "jaw_height": fnum(p.jaw_height), "jaw_thickness": fnum(p.jaw_thickness),
        "jaw_body_depth": fnum(p.jaw_body_depth),
        "length": fnum(p.length), "width": fnum(p.width), "base_height": fnum(p.base_height),
        "housing_length": fnum(p.housing_length), "housing_height": fnum(p.housing_height),
    });
    if let ViseStyle::SelfCentering(sc) = &p.style {
        v["style"] = json!("self-centering");
        v["min_opening"] = json!(fnum(sc.min_opening));
        v["lip_height"] = json!(fnum(sc.lip_height));
        v["lip_depth"] = json!(fnum(sc.lip_depth));
        v["grip_depth"] = json!(fnum(sc.grip_depth));
        v["grip_width"] = json!(fnum(sc.grip_width));
    } else {
        v["style"] = json!("standard");
    }
    v
}

fn add_builtin_vise(env: &mut Env, a: &Map<String, Value>) -> Result<Value, String> {
    let label = req_str(a, "size")?;
    let row = vise::size(label).ok_or_else(|| {
        let all: Vec<&str> = vise::presets().map(|s| s.label).collect();
        format!("no built-in vise '{label}' (one of {})", all.join(", "))
    })?;
    let mut params = with_dims(row.half_open(), a)?;
    if let Some(o) = opt_f64(a, "opening")? {
        params.opening = o;
    } else {
        let lo = match &params.style {
            ViseStyle::SelfCentering(sc) => sc.min_opening,
            ViseStyle::Standard => 0.0,
        };
        if params.opening > params.max_opening || params.opening < lo {
            params.opening = 0.5 * (lo + params.max_opening);
        }
    }
    let target = setup_or_active(env, a)?;
    let fit = matches!(a.get("fit_to_stock"), Some(Value::Bool(true)));
    let placement = if fit {
        VisePlacement::FitStock
    } else {
        let pos = opt_nums::<3>(a, "position")?.unwrap_or([0.0; 3]);
        let rot = opt_nums::<4>(a, "rotation")?.unwrap_or(IDENTITY_Q);
        VisePlacement::At(Location { pos: point(pos), rot })
    };
    let cmd = InsertVise::new(&env.world, &params, target.as_deref(), placement).map_err(cmd_err)?;
    let name = cmd.name().to_owned();
    let view = cmd.params().map(params_view);
    let loc = cmd.location();
    env.history.execute(&env.world, cmd);
    let mut out = loc_view(&loc);
    out["fixture"] = json!(name);
    out["setup"] = json!(target);
    out["vise"] = json!(view);
    Ok(out)
}

fn set_vise(env: &mut Env, a: &Map<String, Value>) -> Result<Value, String> {
    let name = req_str(a, "name")?;
    let current = model::find_by_name(&env.world, name)
        .and_then(|e| fixture::def_of(&env.world, &e))
        .and_then(|d| d.vise)
        .ok_or_else(|| format!("'{name}' is not a built-in vise"))?;
    let from_stock = matches!(a.get("from_stock"), Some(Value::Bool(true)));
    let opening = opt_f64(a, "opening")?;
    if from_stock && opening.is_some() {
        return Err("give \"opening\" or \"from_stock\", not both".into());
    }
    let params = with_dims(current, a)?;
    let params = match opening {
        Some(o) => params.with_opening(o),
        None => params,
    };
    let cmd = if from_stock {
        SetVise::from_stock(&env.world, name, params).map_err(cmd_err)?
    } else {
        SetVise::new(&env.world, name, params).map_err(cmd_err)?
    };
    let view = cmd.params().map(params_view);
    env.history.execute(&env.world, cmd);
    Ok(json!({ "fixture": name, "vise": view }))
}

fn dims_schema() -> Value {
    let props: Map<String, Value> =
        DIMS.iter().map(|d| ((*d).to_owned(), json!({ "type": "number", "description": "mm" }))).collect();
    json!({ "type": "object", "properties": props, "additionalProperties": false,
        "description": "Override the size's dimensions (mm)." })
}

fn s_add() -> Value {
    let sizes: Vec<&str> = vise::presets().map(|s| s.label).collect();
    obj(
        json!({
            "size": { "type": "string", "enum": sizes, "description": "The size table row (jaw width)." },
            "setup": { "type": "string", "description": "The declared setup it holds the part in; defaults to the active one." },
            "fit_to_stock": { "type": "boolean", "description": "Place against the setup's stock and open the jaws to it (ignores position/rotation/opening)." },
            "position": { "type": "array", "items": {"type": "number"}, "minItems": 3, "maxItems": 3,
                "description": "Where the vise frame's origin goes [x, y, z] mm (default the origin)." },
            "rotation": { "type": "array", "items": {"type": "number"}, "minItems": 4, "maxItems": 4,
                "description": "Unit quaternion [x, y, z, w] (default none)." },
            "opening": { "type": "number", "description": "Jaw opening mm (default half the max)." },
            "dimensions": dims_schema()
        }),
        &["size"],
    )
}

fn s_set() -> Value {
    obj(
        json!({
            "name": { "type": "string", "description": "A built-in vise fixture's name." },
            "opening": { "type": "number", "description": "The new jaw opening, mm." },
            "from_stock": { "type": "boolean", "description": "Open the jaws to the setup's stock as the vise stands." },
            "dimensions": dims_schema()
        }),
        &["name"],
    )
}
