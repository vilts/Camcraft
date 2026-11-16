//! The MCP built-in accessory tools (#789): place a generic workholding
//! accessory (parallels, a 1-2-3 block, a step block, a strap clamp, a
//! soft jaw, a rotary subplate - #821, a 3-jaw, 4-jaw or collet lathe
//! chuck - #1442) into a setup - at a pose, or parallels fitted under the
//! stock inside a built-in vise - and resize a placed one. Each is the
//! very undoable command the Fixtures page queues (`InsertAccessory`,
//! `SetAccessory`), one History step, so `undo` takes it back.
//!
//! Provenance: our own tool surface over this repo's accessory commands.

use super::setup_tools::{cmd_err, loc_view, obj, opt_nums, point, setup_or_active};
use super::tools::{Tool, req_str};
use crate::accessory::{self, Accessory};
use crate::commands::{AccessoryPlacement, InsertAccessory, SetAccessory};
use crate::fixture;
use crate::geometry::IDENTITY_Q;
use crate::model;
use crate::schema::Location;
use crate::script::{Env, fnum};

use serde_json::{Map, Value, json};

pub(super) const TOOLS: &[Tool] = &[
    Tool { name: "add_accessory", description: "Place a built-in workholding accessory as a fixture of a setup (the active one by default): parallels (a pair), block (1-2-3 block), step_block, strap_clamp (strap, stud and heel block; the toe tip at the frame origin on the table, the strap running +X, its underside at clamp_height), soft_jaw or rotary_subplate (#821: the base plate a 4th axis is bolted to - generic default figures, enter your own; its frame's origin on the first pin bore at the underside, +X toward the second; it lands fixed, then goes on a fixture plate's dowel pair with place_on_plate, and its thickness - with every plate under it - raises the rotary's centerline for the index-swing check; its height over the machine table counts too, the larger winning, so one placed on a plate by position is never judged on its own thickness alone), three_jaw_chuck, four_jaw_chuck or collet_chuck (#1442: a lathe chuck - body, jaws or collet nose - its frame's z = 0 the face the work stands on, +Z the spindle axis out of it, the body below; it turns with the part, so the lathe holder law reads it as revolved steel). Either at a position/rotation of its frame (z = 0 its underside), or - a chuck only - fit_to_stock: the chuck closes on the setup's round stock (standing on a vertical axis) at its underside, the grip or collet bore set to the stock's diameter, or - parallels only - under_stock_in_vise: the parallels' height is set so the stock's top stands `above` mm over that built-in vise's jaws, and the vise drops to suit. Optional dimension overrides in mm. One undoable step; nonsense dimensions are refused by name.", schema: s_add, run: add_accessory },
    Tool { name: "set_accessory", description: "Resize a placed built-in accessory: dimension overrides in mm (and steps for a step block). The mesh is regenerated; one undoable step.", schema: s_set, run: set_accessory },
];

/// `a` with every dimension `args.dimensions` names (and `steps`) set.
fn with_dims(mut a: Accessory, args: &Map<String, Value>) -> Result<Accessory, String> {
    if let Some(d) = args.get("dimensions") {
        let d = d.as_object().ok_or("\"dimensions\" must be an object of mm lengths")?;
        for (k, v) in d {
            let v = v.as_f64().ok_or_else(|| format!("dimension \"{k}\" must be a number of mm"))?;
            a = a.with_dim(k, v)?;
        }
    }
    if let Some(s) = args.get("steps") {
        let n = s.as_u64().and_then(|n| u32::try_from(n).ok()).ok_or("\"steps\" must be a whole number")?;
        let kind = a.label();
        *a.steps_mut().ok_or_else(|| format!("a {kind} has no steps"))? = n;
    }
    Ok(a)
}

fn view(a: &Accessory) -> Value {
    let mut out = Map::new();
    out.insert("kind".into(), json!(a.kind()));
    for (k, _, v) in a.dims() {
        out.insert(k.into(), json!(fnum(v)));
    }
    if let Accessory::StepBlock { steps, .. } = a {
        out.insert("steps".into(), json!(steps));
    }
    Value::Object(out)
}

fn add_accessory(env: &mut Env, args: &Map<String, Value>) -> Result<Value, String> {
    let kind = req_str(args, "kind")?;
    let fresh = Accessory::default_of(kind)
        .ok_or_else(|| format!("no built-in accessory '{kind}' (one of {})", accessory::KINDS.join(", ")))?;
    let params = with_dims(fresh, args)?;
    let target = setup_or_active(env, args)?;
    let on_stock = match args.get("fit_to_stock") {
        None | Some(Value::Null) => false,
        Some(v) => v.as_bool().ok_or("\"fit_to_stock\" must be true or false")?,
    };
    let placement = if on_stock {
        AccessoryPlacement::OnStock
    } else if let Some(v) = args.get("under_stock_in_vise").filter(|v| !v.is_null()) {
        let vise = v.as_str().ok_or("\"under_stock_in_vise\" must be a vise fixture's name")?.to_owned();
        let above = match args.get("above") {
            None | Some(Value::Null) => 0.25 * 25.4,
            Some(v) => v.as_f64().ok_or("\"above\" must be a number of mm")?,
        };
        AccessoryPlacement::UnderStockInVise { vise, above }
    } else {
        let pos = opt_nums::<3>(args, "position")?.unwrap_or([0.0; 3]);
        let rot = opt_nums::<4>(args, "rotation")?.unwrap_or(IDENTITY_Q);
        AccessoryPlacement::At(Location { pos: point(pos), rot })
    };
    let cmd = InsertAccessory::new(&env.world, &params, target.as_deref(), placement).map_err(cmd_err)?;
    let name = cmd.name().to_owned();
    let shown = cmd.params().map(view);
    let loc = cmd.location();
    env.history.execute(&env.world, cmd);
    let mut out = loc_view(&loc);
    out["fixture"] = json!(name);
    out["setup"] = json!(target);
    out["accessory"] = json!(shown);
    Ok(out)
}

fn set_accessory(env: &mut Env, args: &Map<String, Value>) -> Result<Value, String> {
    let name = req_str(args, "name")?;
    let current = model::find_by_name(&env.world, name)
        .and_then(|e| fixture::def_of(&env.world, &e))
        .and_then(|d| d.accessory)
        .ok_or_else(|| format!("'{name}' is not a built-in accessory"))?;
    let params = with_dims(current, args)?;
    let cmd = SetAccessory::new(&env.world, name, params).map_err(cmd_err)?;
    let shown = cmd.params().map(view);
    env.history.execute(&env.world, cmd);
    Ok(json!({ "fixture": name, "accessory": shown }))
}

fn dims_schema() -> Value {
    json!({ "type": "object", "additionalProperties": { "type": "number" },
        "description": "Dimension overrides, mm. parallels: height, thickness, length, spacing. block: length, width, height. step_block: rise, run, width. strap_clamp: strap_length, strap_width, strap_thickness, stud_diameter, stud_height (above the strap), stud_offset (toe to stud centre), heel_length, clamp_height. soft_jaw: width, height, thickness. rotary_subplate: length, width, thickness, bore_spacing, bore_x (first bore from the -X end), bore_y (bores from the -Y edge), bore_diameter. three_jaw_chuck, four_jaw_chuck: body_diameter, body_length, bore_diameter (through bore), grip_diameter (the work diameter the jaws close on), jaw_length (radial), jaw_width, jaw_height (above the face). collet_chuck: body_diameter, body_length, nose_diameter, nose_length, collet_bore." })
}

fn s_add() -> Value {
    obj(
        json!({
            "kind": { "type": "string", "enum": accessory::KINDS, "description": "The accessory." },
            "setup": { "type": "string", "description": "The declared setup it holds the part in; defaults to the active one." },
            "position": { "type": "array", "items": {"type": "number"}, "minItems": 3, "maxItems": 3,
                "description": "Where the accessory frame's origin goes [x, y, z] mm (default the origin)." },
            "rotation": { "type": "array", "items": {"type": "number"}, "minItems": 4, "maxItems": 4,
                "description": "Unit quaternion [x, y, z, w] (default none)." },
            "under_stock_in_vise": { "type": "string", "description": "Parallels only: a built-in vise's fixture name; fit the parallels under the stock in it (ignores position/rotation and the height)." },
            "fit_to_stock": { "type": "boolean", "description": "Lathe chucks only: close the chuck on the setup's round stock (ignores position/rotation and the grip or collet bore)." },
            "above": { "type": "number", "description": "With under_stock_in_vise: how far the stock's top stands over the jaws, mm (default 6.35)." },
            "steps": { "type": "integer", "minimum": 1, "description": "step_block: the step count." },
            "dimensions": dims_schema()
        }),
        &["kind"],
    )
}

fn s_set() -> Value {
    obj(
        json!({
            "name": { "type": "string", "description": "A built-in accessory fixture's name." },
            "steps": { "type": "integer", "minimum": 1, "description": "step_block: the step count." },
            "dimensions": dims_schema()
        }),
        &["name"],
    )
}
