//! The MCP fixture plate tools (#819): add a built-in grid plate, change
//! its figures, place any fixture at a grid location or on a dowel pair,
//! detect an imported plate's grid from its holes, and read a plate's
//! grid. Each edit is the very undoable command the Fixtures page queues
//! (`InsertPlate`, `SetPlate`, `PlaceOnPlate`, `SetPlateGrid`), one
//! History step.
//!
//! Provenance: our own tool surface over this repo's plate commands.

use serde_json::{Map, Value, json};

use super::setup_tools::{cmd_err, loc_view, obj, opt_nums, point, setup_or_active};
use super::tools::{Tool, req_str};
use crate::commands::fixture_plate::plate_of;
use crate::commands::{InsertPlate, PlaceOnPlate, PlatePlacement, SetPlate, SetPlateGrid};
use crate::geometry::IDENTITY_Q;
use crate::plate::{self, Counterbore, Dowels, Grid, MountAt, PlateDef, PlateParams, mount};
use crate::schema::Location;
use crate::script::{Env, fnum};

pub(super) const TOOLS: &[Tool] = &[
    Tool { name: "add_fixture_plate", description: "Place a built-in generic grid (tooling) plate as a fixture of a setup - the active one by default. Its frame's origin is the plate's corner on its TOP face (x along the length, y along the width, the slab below). Grid locations: rows lettered along +Y from A, columns numbered along +X from 1 (A1 nearest the origin corner). Either lay it centred on the machine's table (on_table) or give a position/rotation. Optional overrides in mm, thread, dowel pattern and counterbore. The collision mesh is a solid slab. One undoable step; nonsense figures are refused by name.", schema: s_add, run: add_fixture_plate },
    Tool { name: "set_fixture_plate", description: "Change a placed built-in plate's figures (dimensions, thread, dowels, counterbore). The mesh is regenerated and every fixture on it is re-placed at its named location (refused if a location is gone, a fixture would hang off, or a mod vise jaw would reach into the stock); `warning` names a re-placed fixture or the new slab sitting in a part or the stock, or a mod vise jaw lifted off the stock. One undoable step.", schema: s_set, run: set_fixture_plate },
    Tool { name: "place_on_plate", description: "Place a fixture (a built-in vise, an imported fixture, another plate) on a plate: at a grid hole (\"C3\": its frame's origin on the hole at the plate top) or on a dowel pair ([\"A1\",\"A3\"]: origin on the first, +X toward the second), turned yaw degrees about the plate's Z. The place is remembered: moving the plate moves it. A rotary subplate goes only on a dowel pair as far apart as its pin bores (#821). Refused by name: an unknown location, a fixture hanging off the plate, a subplate's pins that do not fit. One undoable step.", schema: s_place, run: place_on_plate },
    Tool { name: "detect_plate_grid", description: "Make an imported fixture a plate by detecting its hole grid (the Detect holes rule, read in the model's own frame: holes on its Z, an axis-aligned lattice, each hole snapped to its node within 0.05 mm; a plate turned in its model is refused). Refused by name when its holes form no regular grid. clear: true stops it being a plate. One undoable step.", schema: s_detect, run: detect_plate_grid },
    Tool { name: "get_plate_grid", description: "Read a plate's grid: pitch, rows and columns, the location names, gaps, the dowel lattice, and the fixtures standing on it.", schema: s_name, run: get_plate_grid },
];

const DIMS: [&str; 8] = ["length", "width", "thickness", "pitch_x", "pitch_y", "offset_x", "offset_y", "thread_diameter"];

fn num(v: &Value, k: &str) -> Result<f64, String> {
    v.as_f64().ok_or_else(|| format!("\"{k}\" must be a number of mm"))
}

/// `p` with every override in `a` applied.
fn with_overrides(mut p: PlateParams, a: &Map<String, Value>) -> Result<PlateParams, String> {
    if let Some(d) = a.get("dimensions") {
        let d = d.as_object().ok_or("\"dimensions\" must be an object of mm lengths")?;
        for (k, v) in d {
            let slot = match k.as_str() {
                "length" => &mut p.length,
                "width" => &mut p.width,
                "thickness" => &mut p.thickness,
                "pitch_x" => &mut p.pitch_x,
                "pitch_y" => &mut p.pitch_y,
                "offset_x" => &mut p.offset_x,
                "offset_y" => &mut p.offset_y,
                "thread_diameter" => &mut p.thread_diameter,
                other => return Err(format!("unknown plate dimension \"{other}\" (one of {})", DIMS.join(", "))),
            };
            *slot = num(v, k)?;
        }
    }
    if let Some(t) = a.get("thread") {
        p.thread = t.as_str().ok_or("\"thread\" must be a string such as 3/8-16 or M10")?.to_owned();
    }
    match a.get("dowels") {
        None => {}
        Some(Value::Null | Value::Bool(false)) => p.dowels = None,
        Some(Value::Object(o)) => {
            let f = |k: &str| o.get(k).ok_or_else(|| format!("dowels needs \"{k}\"")).and_then(|v| num(v, k));
            p.dowels = Some(Dowels { pitch_x: f("pitch_x")?, pitch_y: f("pitch_y")?, offset_x: f("offset_x")?, offset_y: f("offset_y")?, diameter: f("diameter")? });
        }
        Some(_) => return Err("\"dowels\" must be an object or null".into()),
    }
    match a.get("counterbore") {
        None => {}
        Some(Value::Null | Value::Bool(false)) => p.counterbore = None,
        Some(Value::Object(o)) => {
            let f = |k: &str| o.get(k).ok_or_else(|| format!("counterbore needs \"{k}\"")).and_then(|v| num(v, k));
            p.counterbore = Some(Counterbore { diameter: f("diameter")?, depth: f("depth")? });
        }
        Some(_) => return Err("\"counterbore\" must be an object or null".into()),
    }
    Ok(p)
}

fn grid_view(g: &Grid) -> Value {
    json!({
        "first": [fnum(g.x0), fnum(g.y0)], "pitch": [fnum(g.pitch_x), fnum(g.pitch_y)],
        "rows": g.rows, "cols": g.cols, "extent": g.extent(), "diameter": fnum(g.diameter), "missing": g.missing, "staggered": g.stagger.is_some(),
    })
}

fn plate_view(d: &PlateDef) -> Value {
    json!({
        "builtin": d.params.is_some(),
        "params": d.params.as_ref().map(|p| json!({
            "size": p.size, "length": fnum(p.length), "width": fnum(p.width), "thickness": fnum(p.thickness),
            "thread": p.thread, "thread_diameter": fnum(p.thread_diameter),
            "counterbore": p.counterbore.as_ref().map(|c| json!({ "diameter": fnum(c.diameter), "depth": fnum(c.depth) })),
        })),
        "top_z": fnum(d.top_z),
        "grid": grid_view(&d.holes),
        "dowels": d.dowels.as_ref().map(grid_view),
    })
}

fn add_fixture_plate(env: &mut Env, a: &Map<String, Value>) -> Result<Value, String> {
    let label = req_str(a, "preset")?;
    let row = plate::preset(label).ok_or_else(|| {
        let all: Vec<&str> = plate::PRESETS.iter().map(|s| s.label).collect();
        format!("no built-in plate '{label}' (one of {})", all.join(", "))
    })?;
    let params = with_overrides(row.params(), a)?;
    let target = setup_or_active(env, a)?;
    let placement = if matches!(a.get("on_table"), Some(Value::Bool(true))) {
        PlatePlacement::OnTable
    } else {
        let pos = opt_nums::<3>(a, "position")?.unwrap_or([0.0; 3]);
        let rot = opt_nums::<4>(a, "rotation")?.unwrap_or(IDENTITY_Q);
        PlatePlacement::At(Location { pos: point(pos), rot })
    };
    let cmd = InsertPlate::new(&env.world, &params, target.as_deref(), placement).map_err(cmd_err)?;
    let name = cmd.name().to_owned();
    let view = cmd.plate().map(plate_view);
    let loc = cmd.location();
    env.history.execute(&env.world, cmd);
    let mut out = loc_view(&loc);
    out["fixture"] = json!(name);
    out["setup"] = json!(target);
    out["plate"] = json!(view);
    Ok(out)
}

fn set_fixture_plate(env: &mut Env, a: &Map<String, Value>) -> Result<Value, String> {
    let name = req_str(a, "name")?;
    let current = plate_of(&env.world, name).map_err(cmd_err)?.params.ok_or_else(|| format!("'{name}' is not a built-in plate"))?;
    let cmd = SetPlate::new(&env.world, name, with_overrides(current, a)?).map_err(cmd_err)?;
    let moved: Vec<String> = cmd.moved().map(str::to_owned).collect();
    let warning = cmd.warning().map(str::to_owned);
    env.history.execute(&env.world, cmd);
    let view = plate_of(&env.world, name).ok().map(|d| plate_view(&d));
    Ok(json!({ "fixture": name, "plate": view, "moved": moved, "warning": warning }))
}

fn place_on_plate(env: &mut Env, a: &Map<String, Value>) -> Result<Value, String> {
    let name = req_str(a, "fixture")?;
    let plate = req_str(a, "plate")?;
    let at = match (a.get("hole"), a.get("dowels")) {
        (Some(h), None) => MountAt::Hole(h.as_str().ok_or("\"hole\" must be a location such as C3")?.to_owned()),
        (None, Some(Value::Array(d))) if d.len() == 2 => {
            let s = |v: &Value| v.as_str().map(str::to_owned).ok_or("\"dowels\" must be two locations such as [\"A1\", \"A3\"]");
            MountAt::Dowels([s(&d[0])?, s(&d[1])?])
        }
        (None, Some(_)) => return Err("\"dowels\" must be two locations such as [\"A1\", \"A3\"]".into()),
        _ => return Err("give \"hole\" or \"dowels\", one of them".into()),
    };
    let yaw = match a.get("yaw") {
        None | Some(Value::Null) => 0.0,
        Some(v) => v.as_f64().ok_or("\"yaw\" must be degrees")?,
    };
    let label = at.label();
    let cmd = PlaceOnPlate::new(&env.world, name, plate, at, yaw).map_err(cmd_err)?;
    let loc = cmd.location();
    env.history.execute(&env.world, cmd);
    let mut out = loc_view(&loc);
    out["fixture"] = json!(name);
    out["plate"] = json!(plate);
    out["at"] = json!(label);
    Ok(out)
}

fn detect_plate_grid(env: &mut Env, a: &Map<String, Value>) -> Result<Value, String> {
    let name = req_str(a, "name")?;
    let cmd = if matches!(a.get("clear"), Some(Value::Bool(true))) {
        SetPlateGrid::clear(&env.world, name)
    } else {
        SetPlateGrid::detect(&env.world, name)
    }
    .map_err(cmd_err)?;
    let view = cmd.plate().map(plate_view);
    env.history.execute(&env.world, cmd);
    Ok(json!({ "fixture": name, "plate": view }))
}

fn get_plate_grid(env: &mut Env, a: &Map<String, Value>) -> Result<Value, String> {
    let name = req_str(a, "name")?;
    let d = plate_of(&env.world, name).map_err(cmd_err)?;
    let names: Vec<String> = d.holes.holes().into_iter().map(|(n, _, _)| n).collect();
    let mut out = plate_view(&d);
    out["fixture"] = json!(name);
    out["locations"] = json!(names);
    out["carries"] = json!(mount::riders(&env.world, name));
    Ok(out)
}

fn overrides_schema() -> Value {
    let props: Map<String, Value> =
        DIMS.iter().map(|d| ((*d).to_owned(), json!({ "type": "number", "description": "mm" }))).collect();
    json!({
        "dimensions": { "type": "object", "properties": props, "additionalProperties": false,
            "description": "Override the preset's figures (mm). offset_x/offset_y place A1 from the origin corner." },
        "thread": { "type": "string", "description": "The thread's name, e.g. 3/8-16 or M10." },
        "dowels": { "type": ["object", "null"], "description": "A dowel lattice {pitch_x, pitch_y, offset_x, offset_y, diameter} mm, or null for none." },
        "counterbore": { "type": ["object", "null"], "description": "Counterbored grid holes {diameter, depth} mm, or null for none." }
    })
}

fn s_add() -> Value {
    let presets: Vec<&str> = plate::PRESETS.iter().map(|s| s.label).collect();
    let mut props = overrides_schema();
    props["preset"] = json!({ "type": "string", "enum": presets, "description": "The preset plate." });
    props["setup"] = json!({ "type": "string", "description": "The declared setup; defaults to the active one." });
    props["on_table"] = json!({ "type": "boolean", "description": "Centre it on the setup machine's table, lying on its top (ignores position/rotation)." });
    props["position"] = json!({ "type": "array", "items": {"type": "number"}, "minItems": 3, "maxItems": 3, "description": "Where the plate frame's origin goes [x, y, z] mm." });
    props["rotation"] = json!({ "type": "array", "items": {"type": "number"}, "minItems": 4, "maxItems": 4, "description": "Unit quaternion [x, y, z, w]." });
    obj(props, &["preset"])
}

fn s_set() -> Value {
    let mut props = overrides_schema();
    props["name"] = json!({ "type": "string", "description": "A built-in plate fixture's name." });
    obj(props, &["name"])
}

fn s_place() -> Value {
    obj(
        json!({
            "fixture": { "type": "string", "description": "The fixture to place." },
            "plate": { "type": "string", "description": "The plate fixture it stands on." },
            "hole": { "type": "string", "description": "A grid location, e.g. C3 (row letter, column number)." },
            "dowels": { "type": "array", "items": {"type": "string"}, "minItems": 2, "maxItems": 2, "description": "Two dowel-lattice locations." },
            "yaw": { "type": "number", "description": "Turn about the plate's Z, degrees (default 0)." }
        }),
        &["fixture", "plate"],
    )
}

fn s_detect() -> Value {
    obj(
        json!({
            "name": { "type": "string", "description": "An imported fixture's name." },
            "clear": { "type": "boolean", "description": "Stop it being a plate instead." }
        }),
        &["name"],
    )
}

fn s_name() -> Value {
    obj(json!({ "name": { "type": "string", "description": "A plate fixture's name." } }), &["name"])
}
