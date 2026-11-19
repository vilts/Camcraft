//! The MCP part-model tools: place and turn an imported part (#716)
//! through the same undoable `MoveModel` the GUI's model move issues, and
//! repair its mesh (#717) through the Model page's `RepairModel`, so a
//! headless session can bring a part's bottom to Z0, stand it up, or
//! close an STL's open edges. Every refusal is the command's own text,
//! returned as a tool error.
//!
//! Provenance: our own tool surface over this repo's `MoveModel` and
//! `RepairModel` commands; no vendor material consulted.

use super::setup_tools::{cmd_err, loc_view, obj, opt_nums, point};
use super::tools::{req_str, Tool};
use crate::commands::{DeleteEntity, MoveModel, RepairModel};
use crate::model;
use crate::operation::SrcRef;
use crate::schema::Location;
use crate::script::Env;
use crate::stock::StockShape;

use serde_json::{json, Map, Value};

/// The Model page's repair defaults (`RepairUi::default` in the UI
/// crate), so a headless repair with no arguments does what the GUI's
/// Repair button does: weld within a micron, fill any hole of up to 256
/// edges.
const DEFAULT_WELD_TOL_MM: f64 = 0.001;
const DEFAULT_MAX_HOLE_EDGES: usize = 256;

pub(super) const TOOLS: &[Tool] = &[
    Tool {
        name: "delete_model",
        description: "Delete an imported part through DeleteEntity. One undo step restores it; redo deletes it again. Refused by name without changing history: an unknown name, a non-model, a fixture, or a model still used by an operation (source, selection or area boundary) or from-model stock, including a setup's stock. Remove those references first; remove_fixture unmarks workholding before deletion.",
        schema: s_delete_model,
        run: delete_model,
    },
    Tool {
        name: "move_model",
        description: "Place an imported part: position [x, y, z] in mm and an optional unit quaternion rotation [x, y, z, w] (omitted keeps the current one). Stock made from the part, or the stock of a one-part document, moves with it and is listed under \"stock\". One undo step. Refused by name: a fixture (move_fixture places it), a work offset (set_wcs), the machine table, an unknown name, a rotation that is not a quaternion.",
        schema: s_move_model,
        run: move_model,
    },
    Tool {
        name: "repair_model",
        description: "Repair an imported model's mesh, as the Model page's Repair button does: weld vertices within weld_tol mm (default 0.001), drop collapsed or duplicate triangles, and fill every open boundary loop of at most max_hole_edges edges (default 256). Returns the counts before and after. One undo step that restores the exact previous facets. Refused by name: an unknown name, the setup's stock or work offsets, a negative tolerance, a budget under 3 edges, a mesh the repair can't close (non-manifold, over budget), and a mesh with nothing to repair.",
        schema: s_repair_model,
        run: repair_model,
    },
];

fn s_delete_model() -> Value {
    obj(json!({ "name": { "type": "string", "description": "An imported part's name." } }), &["name"])
}

fn delete_model(env: &mut Env, a: &Map<String, Value>) -> Result<Value, String> {
    let name = req_str(a, "name")?;
    super::pick::req_model(env, name)?;
    // Refuse dangling references before executing the one undoable command.
    for (key, entity) in model::named_entities(&env.world) {
        if let Some(op) = model::operation_of(&env.world, &entity) {
            let source = match &op.src {
                SrcRef::Stock => false,
                SrcRef::Entity(source) => source == name,
                SrcRef::Selection(picks) => picks.iter().any(|p| p.entity == name),
            };
            if source || op.area.picks().any(|p| p.entity == name) {
                let label = model::operation_label(&env.world, &key, &entity);
                return Err(format!("model '{name}' is used by operation '{label}' - remove its model references first; nothing deleted"));
            }
        }
        if matches!(model::stock_def_of(&env.world, &entity), Some(StockShape::FromModel { source }) if source == name) {
            return Err(format!("model '{name}' is used by stock '{key}' - change that stock's source first; nothing deleted"));
        }
    }
    let cmd = DeleteEntity::new(&env.world, name).map_err(cmd_err)?;
    env.history.execute(&env.world, cmd);
    Ok(json!({ "deleted": name }))
}

/// The named entity's current placement; identity when it has none.
fn pose_of(env: &Env, name: &str) -> Location {
    model::find_by_name(&env.world, name)
        .and_then(|e| model::transform_of(&env.world, &e))
        .unwrap_or_default()
}

fn move_model(env: &mut Env, a: &Map<String, Value>) -> Result<Value, String> {
    let name = req_str(a, "name")?;
    let pos = opt_nums::<3>(a, "position")?.ok_or("\"position\" is required")?;
    let rot = match opt_nums::<4>(a, "rotation")? {
        Some(q) => q,
        None => pose_of(env, name).rot,
    };
    let cmd = MoveModel::new(&env.world, name, Location { pos: point(pos), rot }).map_err(cmd_err)?;
    // `execute` takes the command, so read who rides along first.
    let followers: Vec<String> = cmd.following_stock().map(str::to_owned).collect();
    env.history.execute(&env.world, cmd);
    let stock: Vec<Value> = followers
        .iter()
        .map(|s| {
            let mut view = loc_view(&pose_of(env, s));
            view["name"] = json!(s);
            view
        })
        .collect();
    let mut out = loc_view(&pose_of(env, name));
    out["model"] = json!(name);
    out["stock"] = json!(stock);
    Ok(out)
}

fn s_move_model() -> Value {
    obj(
        json!({
            "name": { "type": "string", "description": "An imported part's name (not a fixture or work offset)." },
            "position": { "type": "array", "items": {"type": "number"}, "minItems": 3, "maxItems": 3,
                "description": "World position [x, y, z] in mm." },
            "rotation": { "type": "array", "items": {"type": "number"}, "minItems": 4, "maxItems": 4,
                "description": "Unit quaternion [x, y, z, w]; omitted keeps the current rotation." }
        }),
        &["name", "position"],
    )
}

/// An optional length in mm under `key`, or `default` when absent. The
/// sign and finiteness are the command's to judge, so its refusal names
/// the rule ("weld tolerance must be a finite, non-negative length").
fn opt_mm(a: &Map<String, Value>, key: &str, default: f64) -> Result<f64, String> {
    match a.get(key) {
        None | Some(Value::Null) => Ok(default),
        Some(v) => v.as_f64().ok_or_else(|| format!("\"{key}\" must be a number in mm, got {v}")),
    }
}

/// An optional non-negative whole number under `key`, or `default`.
fn opt_count(a: &Map<String, Value>, key: &str, default: usize) -> Result<usize, String> {
    match a.get(key) {
        None | Some(Value::Null) => Ok(default),
        Some(v) => v
            .as_u64()
            .and_then(|n| usize::try_from(n).ok())
            .ok_or_else(|| format!("\"{key}\" must be a whole number, got {v}")),
    }
}

fn repair_model(env: &mut Env, a: &Map<String, Value>) -> Result<Value, String> {
    let name = req_str(a, "name")?;
    let weld_tol = opt_mm(a, "weld_tol", DEFAULT_WELD_TOL_MM)?;
    let max_hole_edges = opt_count(a, "max_hole_edges", DEFAULT_MAX_HOLE_EDGES)?;
    // The repair runs at construction, so a refusal never reaches the history.
    let cmd = RepairModel::new(&env.world, name, weld_tol, max_hole_edges).map_err(cmd_err)?;
    let r = cmd.report();
    let out = json!({
        "model": name,
        "weld_tol": weld_tol,
        "max_hole_edges": max_hole_edges,
        "vertices_before": r.vertices_before,
        "vertices_after": r.vertices_after,
        "triangles_before": r.triangles_before,
        "triangles_after": r.triangles_after,
        "welded": r.welded,
        "dropped": r.dropped,
        "holes_filled": r.holes_filled,
        "triangles_added": r.triangles_added,
        "summary": cmd.summary(),
    });
    env.history.execute(&env.world, cmd);
    Ok(out)
}

fn s_repair_model() -> Value {
    obj(
        json!({
            "name": { "type": "string", "description": "An imported model's name." },
            "weld_tol": { "type": "number", "minimum": 0,
                "description": "Weld vertices closer than this, in mm (default 0.001, the Model page's)." },
            "max_hole_edges": { "type": "integer", "minimum": 3,
                "description": "Fill open boundary loops of at most this many edges (default 256)." }
        }),
        &["name"],
    )
}
