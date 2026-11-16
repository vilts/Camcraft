//! The MCP setup tools (#181): declare and activate setups, place the
//! active setup's work offset (WCS), pick its offset index, and mount a
//! machine. Each tool is exactly one existing command run through the
//! session History, so `undo` walks it; every refusal is the command's
//! own text, returned as a tool error.
//!
//! Provenance: our own tool surface over this repo's setup commands;
//! work-offset semantics per RS-274 / ISO 6983 (G54-G59).

use super::tools::{opt_str, req_str, Tool};
use crate::commands::{
    ActivateSetup, CommandError, CreateSetup, DeleteSetup, LoadMachine, SetOperationSetup, SetProgramInfo, SetSetupMachine, SetSetupPost,
    SetSetupWorkOffset, SetStockSourceOnly, SetWcsOrigin,
};
use crate::geometry::{Point3, IDENTITY_Q};
use crate::model;
use crate::schema::{Location, PostRef};
use crate::script::{api, fnum, inventory, Env};
use crate::setup;

use serde_json::{json, Map, Value};

pub(super) const TOOLS: &[Tool] = &[
    Tool { name: "add_setup", description: "Declare a new setup and make it active. With datum_from, it starts as a copy of that setup's stock and WCS placement. The first setup on a document without setups adopts the existing stock and G54.", schema: s_add_setup, run: add_setup },
    Tool { name: "activate_setup", description: "Make a declared setup the active one: where new operations, stock and WCS edits go.", schema: s_setup_name, run: activate_setup },
    Tool { name: "set_operation_setup", description: "Move an operation into another declared setup (setup null: the implicit first position). The stamp is kept, so the operation reads ok or stale by the staleness law. One undo step. Refused by name: an unknown operation or setup, a move to where it already is.", schema: s_set_operation_setup, run: set_operation_setup },
    Tool { name: "delete_setup", description: "Delete a declared setup with its private stock and WCS; one undo restores them. Refused by name while an operation still machines in it (move or delete it first) or another setup is placed relative to it.", schema: s_setup_name, run: delete_setup },
    Tool { name: "set_wcs", description: "Place the active setup's work coordinate system: an absolute origin, or a point of the stock's bounding box (min/center/max per axis) plus an optional offset. Rotation is a unit quaternion [x, y, z, w]; omitted keeps the current one.", schema: s_set_wcs, run: set_wcs },
    Tool { name: "set_work_offset", description: "Set the work offset the post uses for a declared setup's WCS: index is 1-based (G54-G59 accepted as aliases of 1-6), bounded by the setup's own post - an offset it cannot express is refused by setup name. null clears it (offset 1). Returns index and the post's name for it (word is an alias of name). Moves no origin.", schema: s_work_offset, run: set_work_offset },
    Tool { name: "set_program", description: "State the document's program number (1 to 9999: the O-word O0001-O9999) and program comment (the program's name on Heidenhain and SINUMERIK). An omitted key keeps its value; null clears it (O0001 and the machine's name). One undo step.", schema: s_set_program, run: set_program },
    Tool { name: "set_raw_stock_only", description: "For a stock made from a model (set_stock from_model): say whether that source model is raw stock only - a casting or blank you cut into, not the finished part - so the post's gouge verdict does not judge it (on), or is judged like every model (off, the default). An operation that machines the model judges it again. One undo step.", schema: s_raw_stock_only, run: set_raw_stock_only },
    Tool { name: "load_machine", description: "Mount a machine: a built-in by name or a .mch file, or clear it. With setup, the machine is that setup's own (clear: the setup follows the document's machine again); without, it is the document's.", schema: s_load_machine, run: load_machine },
    Tool { name: "set_setup_post", description: "Give a declared setup its own post: a built-in post name or a .cps path; null clears it (the setup follows the document's post again). One undo step. This sets the field only - posting judges whether the post serves the setup's machine.", schema: s_setup_post, run: set_setup_post },
];

// ---- shared readers (the fixture tools use them too) --------------------

pub(super) fn cmd_err(e: CommandError) -> String {
    e.to_string()
}

/// `n` finite numbers under `key`, or `None` when absent.
pub(super) fn opt_nums<const N: usize>(a: &Map<String, Value>, key: &str) -> Result<Option<[f64; N]>, String> {
    let v = match a.get(key) {
        None | Some(Value::Null) => return Ok(None),
        Some(v) => v,
    };
    let bad = || format!("\"{key}\" must be an array of {N} finite numbers, got {v}");
    let arr = v.as_array().filter(|x| x.len() == N).ok_or_else(bad)?;
    let mut out = [0.0; N];
    for (o, x) in out.iter_mut().zip(arr) {
        *o = x.as_f64().filter(|f| f.is_finite()).ok_or_else(bad)?;
    }
    Ok(Some(out))
}

pub(super) fn point(p: [f64; 3]) -> Point3 {
    Point3::new(p[0], p[1], p[2])
}

pub(super) fn loc_view(loc: &Location) -> Value {
    json!({
        "origin": [fnum(loc.pos.x), fnum(loc.pos.y), fnum(loc.pos.z)],
        "rotation": loc.rot.iter().map(|v| fnum(*v)).collect::<Vec<_>>(),
    })
}

/// The setup a tool acts on: the one named (it must be declared), else
/// the active one - `None` only on a document with no setups.
pub(super) fn setup_or_active(env: &Env, a: &Map<String, Value>) -> Result<Option<String>, String> {
    match opt_str(a, "setup")? {
        Some(s) if !setup::exists(&env.world, s) => Err(format!("no setup named '{s}'")),
        Some(s) => Ok(Some(s.to_owned())),
        None => Ok(setup::active(&env.world)),
    }
}

pub(super) fn obj(props: Value, required: &[&str]) -> Value {
    json!({ "type": "object", "properties": props, "required": required, "additionalProperties": false })
}

// ---- handlers ---------------------------------------------------------

fn add_setup(env: &mut Env, a: &Map<String, Value>) -> Result<Value, String> {
    let cmd = CreateSetup::new(&env.world, opt_str(a, "name")?, opt_str(a, "datum_from")?).map_err(cmd_err)?;
    let name = cmd.name().to_owned();
    env.history.execute(&env.world, cmd);
    Ok(json!({ "setup": name, "active": setup::active(&env.world), "setups": setup::names(&env.world) }))
}

fn activate_setup(env: &mut Env, a: &Map<String, Value>) -> Result<Value, String> {
    let name = req_str(a, "name")?;
    if setup::active(&env.world).as_deref() == Some(name) {
        return Err(format!("setup '{name}' is already active"));
    }
    let cmd = ActivateSetup::new(&env.world, Some(name)).map_err(cmd_err)?;
    env.history.execute(&env.world, cmd);
    Ok(json!({ "active": setup::active(&env.world) }))
}

fn set_operation_setup(env: &mut Env, a: &Map<String, Value>) -> Result<Value, String> {
    let op = req_str(a, "operation")?;
    let target = match a.get("setup") {
        None => return Err("\"setup\" is required (a declared setup, or null for the first position)".into()),
        Some(Value::Null) => None,
        Some(Value::String(s)) => Some(s.as_str()),
        Some(other) => return Err(format!("\"setup\" must be a string or null, got {other}")),
    };
    let cmd = SetOperationSetup::new(&env.world, op, target).map_err(cmd_err)?;
    env.history.execute(&env.world, cmd);
    let status = inventory(env)
        .iter()
        .find(|(k, e, _)| k == op || model::operation_label(&env.world, k, e) == op)
        .map(|(_, _, s)| api::st_str(*s));
    Ok(json!({ "operation": op, "setup": target, "status": status }))
}

fn delete_setup(env: &mut Env, a: &Map<String, Value>) -> Result<Value, String> {
    let name = req_str(a, "name")?;
    let cmd = DeleteSetup::new(&env.world, name).map_err(cmd_err)?;
    env.history.execute(&env.world, cmd);
    Ok(json!({ "deleted": name, "active": setup::active(&env.world), "setups": setup::names(&env.world) }))
}

/// One axis of a `stock_point`: where on the stock box's span it sits.
fn pick(span: (f64, f64), word: Option<&str>, axis: &str) -> Result<f64, String> {
    match word {
        Some("min") => Ok(span.0),
        Some("max") => Ok(span.1),
        Some("center") => Ok(0.5 * (span.0 + span.1)),
        _ => Err(format!("\"stock_point.{axis}\" must be \"min\", \"center\" or \"max\"")),
    }
}

fn set_wcs(env: &mut Env, a: &Map<String, Value>) -> Result<Value, String> {
    let w = &env.world;
    let active = setup::active(w);
    let origin = opt_nums::<3>(a, "origin")?;
    let offset = opt_nums::<3>(a, "offset")?;
    let pos = match (origin, a.get("stock_point")) {
        (Some(o), None) => {
            if offset.is_some() {
                return Err("\"offset\" goes with \"stock_point\", not with an absolute \"origin\"".into());
            }
            point(o)
        }
        (None, Some(Value::Object(sp))) => {
            if let Some(k) = sp.keys().find(|k| !["x", "y", "z"].contains(&k.as_str())) {
                return Err(format!("\"stock_point\" has no axis \"{k}\" - give x, y and z"));
            }
            let b = model::stock_world_aabb_for(w, active.as_deref())
                .ok_or("the active setup has no stock to measure from - set_stock first")?;
            let word = |k: &str| sp.get(k).and_then(Value::as_str);
            let d = offset.unwrap_or([0.0; 3]);
            Point3::new(
                pick((b.min.x, b.max.x), word("x"), "x")? + d[0],
                pick((b.min.y, b.max.y), word("y"), "y")? + d[1],
                pick((b.min.z, b.max.z), word("z"), "z")? + d[2],
            )
        }
        (None, Some(other)) => return Err(format!("\"stock_point\" must be an object {{x, y, z}}, got {other}")),
        _ => return Err("give exactly one of \"origin\" or \"stock_point\"".into()),
    };
    // Omitted rotation keeps the WCS's current one (identity when unplaced).
    let current = setup::wcs_entity(w, active.as_deref()).and_then(|e| model::transform_of(w, &e));
    let rot = match opt_nums::<4>(a, "rotation")? {
        Some(q) => q,
        None => current.map_or(IDENTITY_Q, |l| l.rot),
    };
    let cmd = SetWcsOrigin::new(w, Location { pos, rot }).map_err(cmd_err)?;
    env.history.execute(&env.world, cmd);
    let w = &env.world;
    let placed = setup::wcs_entity(w, active.as_deref()).and_then(|e| model::transform_of(w, &e)).unwrap_or_default();
    let mut out = loc_view(&placed);
    out["setup"] = json!(active);
    out["label"] = json!(setup::wcs_label(w, active.as_deref()));
    out["index"] = json!(crate::setup_offsets::index(w, active.as_deref()));
    out["name"] = json!(setup::wcs_name(w, active.as_deref()));
    out["word"] = json!(setup::wcs_name(w, active.as_deref()));
    Ok(out)
}

fn set_work_offset(env: &mut Env, a: &Map<String, Value>) -> Result<Value, String> {
    // #931: `index` is the 1-based offset; `word` (G54-G59) stays as an
    // alias of 1-6 for callers written before it. null clears either.
    let pick = |k: &str| -> Result<Option<Option<u32>>, String> {
        match a.get(k) {
            None => Ok(None),
            Some(Value::Null) => Ok(Some(None)),
            Some(v) => crate::setup_offsets::parse_index(v).map(|n| Some(Some(n))),
        }
    };
    let n = match (pick("index")?, pick("word")?) {
        (None, None) => return Err("\"index\" is required (1-based work offset, G54-G59 as aliases of 1-6, or null to clear)".into()),
        (Some(i), None) | (None, Some(i)) => i,
        (Some(i), Some(w)) if i == w => i,
        (Some(_), Some(_)) => return Err("\"index\" and \"word\" name different work offsets - give one".into()),
    };
    let slot = n.and_then(|n| usize::try_from(n).ok()).map(|n| n.saturating_sub(1));
    let name = setup_or_active(env, a)?
        .ok_or("no setup is declared - add_setup first (a document without setups posts at work offset 1)")?;
    let cmd = SetSetupWorkOffset::new(&env.world, &name, slot).map_err(cmd_err)?;
    env.history.execute(&env.world, cmd);
    let w = &env.world;
    let offset_name = setup::wcs_name(w, Some(&name));
    Ok(json!({
        "setup": name,
        "index": crate::setup_offsets::index(w, Some(&name)),
        "name": offset_name,
        "word": offset_name,
        "label": setup::wcs_label(w, Some(&name)),
    }))
}

fn set_program(env: &mut Env, a: &Map<String, Value>) -> Result<Value, String> {
    let mut next = crate::posting::program_info(&env.world);
    match a.get("number") {
        None => {}
        Some(Value::Null) => next.number = None,
        Some(v) => {
            let n = v.as_u64().ok_or_else(|| format!("\"number\" must be a whole number 1 to 9999 or null, got {v}"))?;
            next.number = Some(u32::try_from(n).map_err(|_| format!("program number {n} is outside 1 to 9999"))?);
        }
    }
    match a.get("comment") {
        None => {}
        Some(Value::Null) => next.comment = None,
        Some(Value::String(c)) => next.comment = Some(c.clone()),
        Some(other) => return Err(format!("\"comment\" must be a string or null, got {other}")),
    }
    let cmd = SetProgramInfo::new(&env.world, next).map_err(cmd_err)?;
    env.history.execute(&env.world, cmd);
    let p = crate::posting::program_info(&env.world);
    Ok(json!({ "number": p.number, "comment": p.comment }))
}

/// #1233: `SetSetupPost` through the session History, as the Setup pane runs it.
fn set_setup_post(env: &mut Env, a: &Map<String, Value>) -> Result<Value, String> {
    let next = match a.get("post") {
        None => return Err("\"post\" is required (a post name or .cps path, or null to clear)".into()),
        Some(Value::Null) => None,
        Some(Value::String(s)) if s.trim().is_empty() => return Err("\"post\" must not be empty (null clears it)".into()),
        Some(Value::String(s)) if s.trim().to_ascii_lowercase().ends_with(".cps") => Some(PostRef::File(s.trim().to_owned())),
        Some(Value::String(s)) => Some(PostRef::Named(s.trim().to_owned())),
        Some(other) => return Err(format!("\"post\" must be a string or null, got {other}")),
    };
    let name = setup_or_active(env, a)?
        .ok_or("no setup is declared - add_setup first (the document's post is select_post's)")?;
    let cmd = SetSetupPost::new(&env.world, &name, next).map_err(cmd_err)?;
    env.history.execute(&env.world, cmd);
    let w = &env.world;
    let own = crate::setup_mount::own_post(w, Some(&name));
    Ok(json!({
        "setup": name,
        "post": own.as_ref().map(crate::posting::post_ref_label),
        "own_post": own.is_some(),
        "posts_with": crate::posting::post_ref_label(&crate::setup_mount::post_of(w, Some(&name))),
    }))
}

fn load_machine(env: &mut Env, a: &Map<String, Value>) -> Result<Value, String> {
    let builtin = opt_str(a, "builtin")?;
    let path = opt_str(a, "path")?;
    let clear = match a.get("clear") {
        None | Some(Value::Null) | Some(Value::Bool(false)) => false,
        Some(Value::Bool(true)) => true,
        Some(other) => return Err(format!("\"clear\" must be true or false, got {other}")),
    };
    if usize::from(builtin.is_some()) + usize::from(path.is_some()) + usize::from(clear) != 1 {
        return Err("give exactly one of \"builtin\", \"path\" or \"clear\": true".into());
    }
    if let Some(b) = builtin
        && !crate::posting::MACHINES.iter().any(|(n, _)| *n == b)
    {
        return Err(format!("no built-in machine named '{b}' - names: {}", machine_names().join(", ")));
    }
    let w = &env.world;
    let target = match opt_str(a, "setup")? {
        Some(s) if !setup::exists(w, s) => return Err(format!("no setup named '{s}'")),
        s => s,
    };
    match target {
        None => {
            let cmd = match (builtin, path) {
                (Some(b), _) => LoadMachine::builtin(w, b),
                (_, Some(p)) => LoadMachine::from_file(w, std::path::Path::new(p)),
                _ => LoadMachine::new(w, String::new(), None),
            }
            .map_err(cmd_err)?;
            env.history.execute(&env.world, cmd);
        }
        Some(s) => {
            let cmd = match (builtin, path) {
                (Some(b), _) => SetSetupMachine::builtin(w, s, b),
                (_, Some(p)) => SetSetupMachine::from_file(w, s, std::path::Path::new(p)),
                _ => SetSetupMachine::new(w, s, None),
            }
            .map_err(cmd_err)?;
            env.history.execute(&env.world, cmd);
        }
    }
    let (path, m) = match target {
        Some(s) => crate::machine::of_setup(&env.world, Some(s)),
        None => crate::machine::document_of(&env.world),
    };
    Ok(json!({
        "setup": target,
        "machine": m.as_ref().map(|m| m.model.clone()),
        "path": (!path.is_empty()).then_some(path),
        // #828: the holder system and the holder new tools start with.
        "spindle_mount": m.as_ref().and_then(|m| m.spindle_mount.clone()),
        "default_holder": m.as_ref().and_then(|m| m.default_holder.clone()),
        // #388: the table of the machine just named - the setup's own,
        // or the document's - never the active setup's.
        "table": m.as_ref().and_then(crate::fixture::table_of).map(|b| json!({
            "min": [fnum(b.min.x), fnum(b.min.y), fnum(b.min.z)],
            "max": [fnum(b.max.x), fnum(b.max.y), fnum(b.max.z)],
        })),
    }))
}

fn machine_names() -> Vec<&'static str> {
    crate::posting::MACHINES.iter().map(|(n, _)| *n).collect()
}

// ---- input schemas ----------------------------------------------------

fn s_add_setup() -> Value {
    obj(
        json!({
            "name": { "type": "string", "description": "Setup name; defaults to the next free SetupN." },
            "datum_from": { "type": "string", "description": "A declared setup whose stock and WCS placement the new one copies." }
        }),
        &[],
    )
}

fn s_raw_stock_only() -> Value {
    obj(json!({ "on": { "type": "boolean", "description": "true: the source model is raw stock only; false: judge it as the part (default)." } }), &["on"])
}

/// #1094: the stock page's checkbox, through the same command.
fn set_raw_stock_only(env: &mut Env, a: &Map<String, Value>) -> Result<Value, String> {
    let on = a.get("on").and_then(Value::as_bool).ok_or("\"on\" must be true or false")?;
    let cmd = SetStockSourceOnly::new(&env.world, on).map_err(cmd_err)?;
    env.history.execute(&env.world, cmd);
    Ok(json!({ "raw_stock_only": on, "judged_out": crate::stock_source::stock_only_model(&env.world).and_then(|e| model::name_of(&env.world, &e)) }))
}

fn s_setup_name() -> Value {
    obj(json!({ "name": { "type": "string", "description": "A declared setup's name." } }), &["name"])
}

fn s_set_operation_setup() -> Value {
    obj(
        json!({
            "operation": { "type": "string", "description": "The operation's name." },
            "setup": { "type": ["string", "null"], "description": "A declared setup; null moves it to the implicit first position." }
        }),
        &["operation", "setup"],
    )
}

fn s_set_wcs() -> Value {
    let span = json!({ "type": "string", "enum": ["min", "center", "max"] });
    obj(
        json!({
            "origin": { "type": "array", "items": {"type": "number"}, "minItems": 3, "maxItems": 3,
                "description": "Absolute origin [x, y, z] in mm." },
            "stock_point": { "type": "object", "description": "A point of the active setup's stock bounding box, e.g. {x: \"min\", y: \"min\", z: \"max\"} for the top front-left corner.",
                "properties": { "x": span, "y": span, "z": span }, "required": ["x", "y", "z"] },
            "offset": { "type": "array", "items": {"type": "number"}, "minItems": 3, "maxItems": 3,
                "description": "Added to the stock_point, [dx, dy, dz] in mm." },
            "rotation": { "type": "array", "items": {"type": "number"}, "minItems": 4, "maxItems": 4,
                "description": "Unit quaternion [x, y, z, w]; omitted keeps the current rotation." }
        }),
        &[],
    )
}

fn s_work_offset() -> Value {
    let pick = json!({ "type": ["integer", "string", "null"] });
    let mut index = pick.clone();
    index["description"] = json!("The 1-based work offset to post, up to the setup's post's maximum; the words G54-G59 are aliases of 1-6; null clears it (offset 1).");
    let mut word = pick;
    word["description"] = json!("Alias of index, kept for older callers: G54-G59 (or a number); null clears it.");
    obj(
        json!({
            "index": index,
            "word": word,
            "setup": { "type": "string", "description": "A declared setup; defaults to the active one." }
        }),
        &[],
    )
}

fn s_setup_post() -> Value {
    obj(
        json!({
            "post": { "type": ["string", "null"], "description": "A built-in post name or a .cps path; null clears the setup's own post (it follows the document's)." },
            "setup": { "type": "string", "description": "A declared setup; defaults to the active one." }
        }),
        &["post"],
    )
}

fn s_load_machine() -> Value {
    obj(
        json!({
            "builtin": { "type": "string", "enum": machine_names(), "description": "A built-in machine." },
            "path": { "type": "string", "description": "A .mch machine file." },
            "clear": { "type": "boolean", "description": "true: no machine (with setup: follow the document's)." },
            "setup": { "type": "string", "description": "Mount on this declared setup only; omitted mounts the document's machine." }
        }),
        &[],
    )
}

fn s_set_program() -> Value {
    obj(
        json!({
            "number": { "type": ["integer", "null"], "minimum": 1, "maximum": 9999,
                "description": "The program number; null clears it (O0001)." },
            "comment": { "type": ["string", "null"],
                "description": "The program comment, at most 40 printable ASCII characters without ( ) % ; *; null clears it (the machine's name)." }
        }),
        &[],
    )
}
