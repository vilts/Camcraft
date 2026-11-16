//! The MCP tool-body tools (#361): give a document tool a modelled body
//! from a holder or assembly file (STEP, STL, OBJ) - a milling body along
//! the spindle axis, a lathe holder in the turning tool frame with its
//! optional insert zone - read it back, or clear it. Each write is the
//! very `SetToolBody` the Tool editor's Body tab queues, one History
//! step, so `undo` takes it back.
//!
//! Provenance: our own tool surface over this repo's tool-body commands.

use std::path::Path;

use super::setup_tools::{cmd_err, obj};
use super::tools::{Tool, req_str};
use crate::commands::{LatheBodySpec, SetToolBody};
use crate::model;
use crate::script::{Env, fnum};
use crate::tool_body::{self, BodyFrame};
use crate::tool_body_assembly::{self, PiecePlace};
use crate::tool_body_file;
use crate::tool_body_lathe_view;
use camcraft_tools::lathe_holder::LathePlacement;
use camcraft_tools::lathe_insert_zone::InsertZone;

use serde_json::{Map, Value, json};

pub(super) const TOOLS: &[Tool] = &[
    Tool { name: "get_tool_body", description: "A document tool's modelled body, or body: null when it has none (the holder cylinder is judged). A \"spindle\" body (milling) states the model direction that is the tool axis, the model origin's height above the tip, and its revolved envelope (foot, top, max_radius above the tip); a \"lathe\" body states its placement in the turning tool frame (radial, along, tip), its insert zone and its extent from the tip (radial, normal, spindle); \"pieces\" lists every assembled piece and its carried coordinate systems. Lengths in mm.", schema: s_tool, run: get_body },
    Tool { name: "set_tool_body", description: "Give a milling tool (or a boring bar on a mill) a body from a holder or assembly file (STEP, STL, OBJ), placed along the spindle axis. With axis and tip_offset: the model direction that points from the tip toward the spindle, and the model origin's height above the tip along it. With neither: a STEP is aligned by the ISO 13399 tool coordinate systems it names, and refused by name when it names none. The collision laws then judge the body's revolved envelope instead of the holder cylinder. Refused by name: a lathe-held tool (use set_lathe_tool_body), a body reaching into the flutes, a mesh that is no body, a model too detailed to import at full fidelity. One undoable step.", schema: s_set, run: set_body },
    Tool { name: "set_lathe_tool_body", description: "Give a lathe-held tool (turning, grooving, threading or form insert, or a boring bar) a holder body from a file (STEP, STL, OBJ), placed in the turning tool frame: radial is the model direction that points away from the spindle axis, along the model direction of the lathe's +Z (chuck toward the free end), tip the model point the moves carry (the insert's nose centre). The body need not be symmetric: every move of a turning operation is then judged against the part and the standing stock, a rub blocking the post. insert_zone (optional) is a box about the tip, in that frame, holding the insert plate the file models - the judge leaves it out. Refused by name: a tool that is no lathe steel, directions that are not square, a tip far from the body, a zone larger than one insert. One undoable step.", schema: s_set_lathe, run: set_lathe_body },
    Tool { name: "attach_tool_body_piece", description: "Attach a modular holder/adapter/clamping-unit piece (STEP, STL, OBJ) onto a tool's existing body as one undoable step. With no explicit placement arguments, mates by unambiguous ISO 13399 MCS <-> CSW coordinate systems (or pass target_piece / target_csys / piece_csys to pick the pair, axis + tip_offset for spindle placement, or radial + along + optional tip for lathe placement). Combined pieces are judged and displayed together.", schema: s_attach_piece, run: attach_piece },
    Tool { name: "remove_tool_body_piece", description: "Remove piece at 0-based index from a multi-piece tool body assembly and rebuild its collision envelope as one undoable step. Refused by name when index is out of range or only one piece remains (use clear_tool_body to remove the whole body).", schema: s_remove_piece, run: remove_piece },
    Tool { name: "set_lathe_insert_zone", description: "Set or clear (insert_zone: null) the insert zone of a tool's lathe body, keeping its mesh and placement: a box about the tip, in the turning tool frame (x radial, y normal, z along the spindle), holding the insert plate - lo at or below 0 and hi at or above 0 on every axis, no farther from the tip than one insert reaches. One undoable step.", schema: s_zone, run: set_zone },
    Tool { name: "clear_tool_body", description: "Remove a tool's modelled body (either frame): the holder cylinder is judged again, and a turning tool's holder is no longer judged (its operations say so). One undoable step.", schema: s_tool, run: clear_body },
];

fn view(env: &Env, name: &str) -> Result<Value, String> {
    let e = model::find_by_name(&env.world, name).ok_or_else(|| format!("no tool named '{name}'"))?;
    if model::tool_of(&env.world, &e).is_none() {
        return Err(format!("'{name}' is not a tool"));
    }
    let Some(body) = tool_body::body_of(&env.world, &e) else {
        return Ok(json!({ "tool": name, "body": null }));
    };
    let v3 = |a: [f64; 3]| json!([fnum(a[0]), fnum(a[1]), fnum(a[2])]);
    let pieces: Vec<Value> = tool_body_assembly::pieces_of(&body)
        .into_iter()
        .enumerate()
        .map(|(i, p)| {
            json!({
                "index": i,
                "source": p.source,
                "facets": p.facets.len(),
                "csys": p.csys.iter().map(|c| c.name.as_str()).collect::<Vec<_>>(),
            })
        })
        .collect();
    let mut out = json!({
        "source": body.source,
        "facets": body.facets.len(),
        "drawn": body.spun.is_some(),
        "machine_default": body.machine_default,
        "pieces": pieces,
    });
    match &body.frame {
        BodyFrame::Spindle { axis, offset, profile } => {
            out["frame"] = json!("spindle");
            out["axis"] = v3(*axis);
            out["tip_offset"] = fnum(*offset);
            out["foot"] = fnum(profile.bottom());
            out["top"] = fnum(profile.top());
            out["max_radius"] = fnum(profile.bands().iter().map(|b| b.r).fold(0.0, f64::max));
        }
        BodyFrame::Lathe { place, zone, .. } => {
            out["frame"] = json!("lathe");
            out["radial"] = v3(place.radial);
            out["along"] = v3(place.along);
            out["tip"] = v3(place.tip);
            out["insert_zone"] = zone.as_ref().map_or(Value::Null, |z| json!({ "lo": v3(z.lo), "hi": v3(z.hi) }));
            if let Some((lo, hi)) = tool_body_lathe_view::extent(&body) {
                let span = |k: usize| json!([fnum(lo[k]), fnum(hi[k])]);
                out["extent"] = json!({ "radial": span(0), "normal": span(1), "spindle": span(2) });
            }
        }
    }
    Ok(json!({ "tool": name, "body": out }))
}

fn get_body(env: &mut Env, a: &Map<String, Value>) -> Result<Value, String> {
    view(env, req_str(a, "tool")?)
}

fn set_body(env: &mut Env, a: &Map<String, Value>) -> Result<Value, String> {
    let name = req_str(a, "tool")?;
    let path = Path::new(req_str(a, "path")?);
    let axis = vec3(a, "axis")?;
    let offset = match a.get("tip_offset") {
        None | Some(Value::Null) => None,
        Some(v) => Some(v.as_f64().ok_or("\"tip_offset\" must be a number (mm)")?),
    };
    let (cmd, aligned) = match (axis, offset) {
        (Some(axis), Some(offset)) => (SetToolBody::from_file(&env.world, name, path, axis, offset).map_err(cmd_err)?, "numeric"),
        (None, None) => (SetToolBody::from_step_auto(&env.world, name, path).map_err(cmd_err)?, "ISO 13399 coordinate systems"),
        _ => return Err("give both \"axis\" and \"tip_offset\" for a numeric placement, or neither to align a STEP by its tool coordinate systems".to_owned()),
    };
    env.history.execute(&env.world, cmd);
    let mut out = view(env, name)?;
    out["placed_by"] = json!(aligned);
    Ok(out)
}

fn set_lathe_body(env: &mut Env, a: &Map<String, Value>) -> Result<Value, String> {
    let name = req_str(a, "tool")?;
    let path = Path::new(req_str(a, "path")?);
    let radial = vec3(a, "radial")?.ok_or("\"radial\" is required: the model direction that points away from the spindle axis")?;
    let along = vec3(a, "along")?.ok_or("\"along\" is required: the model direction of the lathe's +Z")?;
    let tip = vec3(a, "tip")?.unwrap_or([0.0; 3]);
    let zone = zone_arg(a)?.flatten();
    let file = tool_body_file::read(path)?;
    let raw_csys = tool_body_assembly::read_csys_if_step(path)?;
    let spec = LatheBodySpec { source: file.source, facets: file.facets, place: LathePlacement { radial, along, tip }, zone };
    let cmd = SetToolBody::lathe_with_csys(&env.world, name, spec, raw_csys).map_err(cmd_err)?;
    env.history.execute(&env.world, cmd);
    view(env, name)
}

fn opt_str(a: &Map<String, Value>, key: &str) -> Result<Option<String>, String> {
    match a.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(v) => v.as_str().map(|s| Some(s.to_owned())).ok_or_else(|| format!("\"{key}\" must be a string")),
    }
}

fn opt_usize(a: &Map<String, Value>, key: &str) -> Result<Option<usize>, String> {
    match a.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(v) => v
            .as_u64()
            .and_then(|n| usize::try_from(n).ok())
            .map(Some)
            .ok_or_else(|| format!("\"{key}\" must be a non-negative integer")),
    }
}

fn attach_piece(env: &mut Env, a: &Map<String, Value>) -> Result<Value, String> {
    let name = req_str(a, "tool")?;
    let path = Path::new(req_str(a, "path")?);
    let target_piece = opt_usize(a, "target_piece")?;
    let target_csys = opt_str(a, "target_csys")?;
    let piece_csys = opt_str(a, "piece_csys")?;
    let axis = vec3(a, "axis")?;
    let tip_offset = match a.get("tip_offset") {
        None | Some(Value::Null) => None,
        Some(v) => Some(v.as_f64().ok_or("\"tip_offset\" must be a number (mm)")?),
    };
    let radial = vec3(a, "radial")?;
    let along = vec3(a, "along")?;
    let tip = vec3(a, "tip")?;

    let has_csys = target_piece.is_some() || target_csys.is_some() || piece_csys.is_some();
    let has_spindle = axis.is_some() || tip_offset.is_some();
    let has_lathe = radial.is_some() || along.is_some() || tip.is_some();
    let place = match (has_csys, has_spindle, has_lathe) {
        (_, true, true) | (true, true, _) | (true, _, true) => {
            return Err("give either CSYS mating arguments (target_piece/target_csys/piece_csys), spindle placement (axis + tip_offset), or lathe placement (radial + along), not a mix".to_owned());
        }
        (false, true, false) => match (axis, tip_offset) {
            (Some(axis), Some(offset)) => PiecePlace::Spindle { axis, offset },
            _ => return Err("give both \"axis\" and \"tip_offset\" for spindle placement".to_owned()),
        },
        (false, false, true) => match (radial, along) {
            (Some(radial), Some(along)) => PiecePlace::Lathe(LathePlacement { radial, along, tip: tip.unwrap_or([0.0; 3]) }),
            _ => return Err("give both \"radial\" and \"along\" for lathe placement".to_owned()),
        },
        (_, false, false) => PiecePlace::Csys { target_piece, target_csys, piece_csys },
    };
    let cmd = SetToolBody::attach_piece(&env.world, name, path, place).map_err(cmd_err)?;
    env.history.execute(&env.world, cmd);
    view(env, name)
}

fn remove_piece(env: &mut Env, a: &Map<String, Value>) -> Result<Value, String> {
    let name = req_str(a, "tool")?;
    let index = opt_usize(a, "index")?.ok_or("\"index\" is required (0-based piece index)")?;
    let cmd = SetToolBody::remove_piece(&env.world, name, index).map_err(cmd_err)?;
    env.history.execute(&env.world, cmd);
    view(env, name)
}

fn set_zone(env: &mut Env, a: &Map<String, Value>) -> Result<Value, String> {
    let name = req_str(a, "tool")?;
    let zone = zone_arg(a)?.ok_or("\"insert_zone\" is required (null clears the zone)")?;
    let cmd = SetToolBody::insert_zone(&env.world, name, zone).map_err(cmd_err)?;
    env.history.execute(&env.world, cmd);
    view(env, name)
}

fn clear_body(env: &mut Env, a: &Map<String, Value>) -> Result<Value, String> {
    let name = req_str(a, "tool")?;
    let cmd = SetToolBody::new(&env.world, name, None).map_err(cmd_err)?;
    env.history.execute(&env.world, cmd);
    view(env, name)
}

/// `[x, y, z]` under `key`: absent or null is `None`; anything else
/// must be exactly three numbers.
fn vec3(a: &Map<String, Value>, key: &str) -> Result<Option<[f64; 3]>, String> {
    match a.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(v) => three(v).map(Some).ok_or_else(|| format!("\"{key}\" must be [x, y, z] numbers")),
    }
}

fn three(v: &Value) -> Option<[f64; 3]> {
    match v.as_array()?.as_slice() {
        [x, y, z] => Some([x.as_f64()?, y.as_f64()?, z.as_f64()?]),
        _ => None,
    }
}

/// `insert_zone`: absent is `None`, null is `Some(None)` (no zone), an
/// object `{lo, hi}` the box.
fn zone_arg(a: &Map<String, Value>) -> Result<Option<Option<InsertZone>>, String> {
    let bad = || "\"insert_zone\" must be {\"lo\": [x, y, z], \"hi\": [x, y, z]} or null".to_owned();
    match a.get("insert_zone") {
        None => Ok(None),
        Some(Value::Null) => Ok(Some(None)),
        Some(Value::Object(o)) => {
            if o.keys().any(|k| k != "lo" && k != "hi") {
                return Err(bad());
            }
            let lo = o.get("lo").and_then(three).ok_or_else(bad)?;
            let hi = o.get("hi").and_then(three).ok_or_else(bad)?;
            Ok(Some(Some(InsertZone { lo, hi })))
        }
        Some(_) => Err(bad()),
    }
}

fn vec_schema(description: &str) -> Value {
    json!({ "type": "array", "items": { "type": "number" }, "minItems": 3, "maxItems": 3, "description": description })
}

fn zone_schema() -> Value {
    json!({
        "type": ["object", "null"],
        "properties": {
            "lo": vec_schema("The box's low corner from the tip, mm: [x radial, y normal, z along the spindle], each at or below 0."),
            "hi": vec_schema("The box's high corner from the tip, mm, each at or above 0.")
        },
        "required": ["lo", "hi"],
        "description": "The insert plate's box about the tip in the turning tool frame; null for none."
    })
}

fn s_tool() -> Value {
    obj(json!({ "tool": { "type": "string", "description": "The document tool's name." } }), &["tool"])
}

fn s_set() -> Value {
    obj(
        json!({
            "tool": { "type": "string", "description": "The document tool's name (a milling tool, or a boring bar)." },
            "path": { "type": "string", "description": "The holder or assembly file: .step/.stp, .stl or .obj." },
            "axis": vec_schema("The model direction that is the tool axis, pointing from the tip toward the spindle. Needs tip_offset."),
            "tip_offset": { "type": "number", "description": "The model origin's height above the tool tip along the axis, mm. Needs axis." }
        }),
        &["tool", "path"],
    )
}

fn s_set_lathe() -> Value {
    obj(
        json!({
            "tool": { "type": "string", "description": "The document tool's name (a lathe steel or a boring bar)." },
            "path": { "type": "string", "description": "The holder file: .step/.stp, .stl or .obj." },
            "radial": vec_schema("The model direction that points away from the spindle axis (the turning frame's +x)."),
            "along": vec_schema("The model direction of the lathe's +Z, chuck toward the free end; square to radial."),
            "tip": vec_schema("The model point that is the tool tip (the insert's nose centre), in mm, in the model's frame after import (an inch or metre STEP is scaled to mm; STL and OBJ numbers are read as mm). Default the model origin."),
            "insert_zone": zone_schema()
        }),
        &["tool", "path", "radial", "along"],
    )
}

fn s_zone() -> Value {
    obj(
        json!({
            "tool": { "type": "string", "description": "The document tool's name; it carries a lathe body." },
            "insert_zone": zone_schema()
        }),
        &["tool", "insert_zone"],
    )
}

fn s_attach_piece() -> Value {
    obj(
        json!({
            "tool": { "type": "string", "description": "The document tool's name; it already carries piece 0." },
            "path": { "type": "string", "description": "The piece file (extension, adapter, clamping unit): .step/.stp, .stl or .obj." },
            "target_piece": { "type": "integer", "minimum": 0, "description": "Optional 0-based index of the existing piece to mate onto; omit to search all existing pieces." },
            "target_csys": { "type": "string", "description": "Optional coordinate system name on the existing piece (e.g. \"MCS\" or \"CSW\"); omit for ISO 13399 auto-mate." },
            "piece_csys": { "type": "string", "description": "Optional coordinate system name on the incoming piece (e.g. \"CSW\" or \"MCS\"); omit for ISO 13399 auto-mate." },
            "axis": vec_schema("Optional explicit spindle axis direction (with tip_offset) instead of CSYS mating."),
            "tip_offset": { "type": "number", "description": "Optional explicit spindle tip offset in mm (with axis) instead of CSYS mating." },
            "radial": vec_schema("Optional explicit lathe radial direction (with along) instead of CSYS mating."),
            "along": vec_schema("Optional explicit lathe spindle +Z direction (with radial) instead of CSYS mating."),
            "tip": vec_schema("Optional explicit lathe tip point in model coordinates (with radial and along).")
        }),
        &["tool", "path"],
    )
}

fn s_remove_piece() -> Value {
    obj(
        json!({
            "tool": { "type": "string", "description": "The document tool's name; it carries a multi-piece body." },
            "index": { "type": "integer", "minimum": 0, "description": "0-based index of the piece to remove." }
        }),
        &["tool", "index"],
    )
}
