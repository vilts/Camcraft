//! The MCP pick tools (#180): read an operation's machined features and
//! set them - faces, edge chains, loops (a face ring's chains) - through
//! `SetOperationGeometry`, so a pick is one undoable History step like a
//! click in the app. Feature ids are the geometry-hash ids `list_features`
//! prints, written as `"<kind>:0x<hex>"` strings: a JSON number cannot
//! carry a 64-bit hash through every client exactly.
//!
//! Provenance: our own tool surface over this repo's pick commands.

use super::edit::find_op;
use super::tools::req_str;
use crate::commands::SetOperationGeometry;
use crate::model;
use crate::operation::{self, FeatureSel, GeoSel, OpParams, SrcRef};
use crate::script::Env;

use flecs_ecs::prelude::Entity;
use serde_json::{json, Map, Value};

/// The string form of a feature id: `face:0x…`, `chain:0x…`,
/// `patch:0x…` (a welded smooth face), `vertex:0x…`.
pub(super) fn feature_id(f: FeatureSel) -> String {
    let kind = match f {
        FeatureSel::Face(_) => "face",
        FeatureSel::Chain(_) => "chain",
        FeatureSel::Patch(_) => "patch",
        FeatureSel::Vertex(_) => "vertex",
    };
    format!("{kind}:{:#018x}", f.id())
}

/// [`feature_id`]'s inverse; anything else is refused by its text.
pub(super) fn parse_feature_id(s: &str) -> Result<FeatureSel, String> {
    let bad = || format!("bad feature id '{s}' - use an id list_features prints, like \"face:0x00ab…\"");
    let (kind, hex) = s.trim().split_once(':').ok_or_else(bad)?;
    let hex = hex.strip_prefix("0x").or_else(|| hex.strip_prefix("0X")).unwrap_or(hex);
    if hex.is_empty() || hex.len() > 16 {
        return Err(bad());
    }
    let id = u64::from_str_radix(hex, 16).map_err(|_| bad())?;
    match kind {
        "face" => Ok(FeatureSel::Face(id)),
        "chain" => Ok(FeatureSel::Chain(id)),
        "patch" => Ok(FeatureSel::Patch(id)),
        "vertex" => Ok(FeatureSel::Vertex(id)),
        _ => Err(bad()),
    }
}

/// A part by name: a model the user imported, never stock, a tool, an
/// operation, a setup, a work offset or a fixture.
pub(super) fn req_model(env: &Env, name: &str) -> Result<Entity, String> {
    let w = &env.world;
    let e = model::find_by_name(w, name).ok_or_else(|| format!("no model named '{name}'"))?;
    if !model::is_model(w, &e) {
        return Err(format!("'{name}' is not a model"));
    }
    if model::is_fixture(w, &e) {
        return Err(format!("'{name}' is a fixture - workholding is checked for collisions, not machined"));
    }
    Ok(e)
}

/// One `{model, id}` pick, the model checked to be a part.
pub(super) fn pick_of(env: &Env, v: &Value) -> Result<GeoSel, String> {
    let Value::Object(m) = v else {
        return Err(format!("a pick is an object {{\"model\", \"id\"}}, got {v}"));
    };
    if let Some(k) = m.keys().find(|k| *k != "model" && *k != "id") {
        return Err(format!("a pick has \"model\" and \"id\" only, not \"{k}\""));
    }
    let entity = req_str(m, "model")?;
    req_model(env, entity)?;
    let feature = parse_feature_id(req_str(m, "id")?)?;
    let pick = GeoSel { entity: entity.to_owned(), feature };
    // #692: the one pick rule - an id is taken only if a click could land on it.
    crate::pick::check_pickable(&env.world, &pick)?;
    Ok(pick)
}

/// A pick list: a non-empty array of `{model, id}`.
pub(super) fn picks_of(env: &Env, v: Option<&Value>, key: &str) -> Result<Vec<GeoSel>, String> {
    match v {
        Some(Value::Array(items)) if !items.is_empty() => items.iter().map(|p| pick_of(env, p)).collect(),
        Some(Value::Array(_)) => Err(format!("\"{key}\" needs at least one pick")),
        None | Some(Value::Null) => Err(format!("\"{key}\" is required")),
        Some(other) => Err(format!("\"{key}\" must be an array of picks, got {other}")),
    }
}

pub(super) fn picks_view(items: &[GeoSel]) -> Vec<Value> {
    items.iter().map(|g| json!({ "model": g.entity, "id": feature_id(g.feature) })).collect()
}

/// The feature-kind door: refusals the generation walks make by name,
/// said at the pick instead, before anything lands in the History.
pub(super) fn kind_door(label: &str, params: &OpParams, items: &[GeoSel]) -> Result<(), String> {
    let first = |pred: fn(&FeatureSel) -> bool| items.iter().find(|g| pred(&g.feature));
    if matches!(params, OpParams::MultiAxisContour { .. }) {
        // Its picks ARE the path: curves only (selection's drive_chains).
        if let Some(g) = first(|f| !matches!(f, FeatureSel::Chain(_))) {
            return Err(format!(
                "{} of '{}' is not an edge chain - a {label} operation machines picked edge chains",
                g.feature, g.entity
            ));
        }
    } else if operation::selection_drives_region(params)
        && let Some(g) = first(|f| matches!(f, FeatureSel::Vertex(_)))
    {
        // A point bounds no region (the selection walk's own refusal).
        return Err(format!(
            "{} of '{}' is a point - a {label} operation's area needs faces or edge chains",
            g.feature, g.entity
        ));
    }
    Ok(())
}

/// `set_operation_geometry`: the op's source becomes the picked
/// features (`SrcRef::Selection`), one undo step.
pub(super) fn set_operation_geometry(env: &mut Env, a: &Map<String, Value>) -> Result<Value, String> {
    let key = find_op(env, req_str(a, "operation")?)?;
    let items = picks_of(env, a.get("picks"), "picks")?;
    let e = model::find_by_name(&env.world, &key).ok_or_else(|| format!("no operation named '{key}'"))?;
    let op = model::operation_of(&env.world, &e).ok_or_else(|| format!("'{key}' is not an operation"))?;
    kind_door(op.params.kind().info().label, &op.params, &items)?;
    let cmd = SetOperationGeometry::new(&env.world, &key, items).map_err(|e| e.to_string())?;
    env.history.execute(&env.world, cmd);
    geometry_view(env, &key)
}

/// `get_operation_geometry`: what an op machines - its source, the
/// picks, the area controls, and what the picks resolve to.
pub(super) fn get_operation_geometry(env: &mut Env, a: &Map<String, Value>) -> Result<Value, String> {
    let key = find_op(env, req_str(a, "operation")?)?;
    geometry_view(env, &key)
}

pub(super) fn geometry_view(env: &Env, key: &str) -> Result<Value, String> {
    let w = &env.world;
    let e = model::find_by_name(w, key).ok_or_else(|| format!("no operation named '{key}'"))?;
    let op = model::operation_of(w, &e).ok_or_else(|| format!("'{key}' is not an operation"))?;
    let (source, notes) = match &op.src {
        SrcRef::Stock => (json!("stock"), Vec::new()),
        SrcRef::Entity(n) => (json!({ "model": n }), Vec::new()),
        SrcRef::Selection(items) => (
            json!({ "picks": picks_view(items) }),
            operation::selection_notes(w, op.setup.as_deref(), items, &op.params),
        ),
    };
    Ok(json!({
        "operation": key,
        "kind": op.params.kind().info().label,
        "source": source,
        "area": super::area::area_view(&op.area),
        "area_applies": operation::area_unsupported(op.params.kind()).is_none(),
        "notes": notes,
    }))
}

// ---- input schemas ----------------------------------------------------

fn obj(props: Value, required: &[&str]) -> Value {
    json!({ "type": "object", "properties": props, "required": required, "additionalProperties": false })
}

pub(super) fn pick_list_schema(what: &str) -> Value {
    json!({
        "type": "array",
        "description": what,
        "items": {
            "type": "object",
            "properties": {
                "model": { "type": "string", "description": "The model the feature is on." },
                "id": { "type": "string", "description": "A feature id from list_features: face:0x…, chain:0x…, patch:0x… or vertex:0x…." }
            },
            "required": ["model", "id"],
            "additionalProperties": false
        }
    })
}

pub(super) fn s_list_features() -> Value {
    obj(
        json!({
            "model": { "type": "string", "description": "Model name (see get_job)." },
            "kinds": { "type": "array", "items": { "type": "string", "enum": ["faces", "chains", "patches", "vertices"] },
                "description": "Which feature kinds to list; default all." },
            "limit": { "type": "integer", "minimum": 1, "description": "Rows per kind (default 500)." }
        }),
        &["model"],
    )
}

pub(super) fn s_op() -> Value {
    obj(json!({ "operation": { "type": "string", "description": "Operation name (key or label)." } }), &["operation"])
}

pub(super) fn s_set_geometry() -> Value {
    obj(
        json!({
            "operation": { "type": "string", "description": "Operation name (key or label)." },
            "picks": pick_list_schema("The features to machine, in order. A loop is picked by every chain list_features names for it.")
        }),
        &["operation", "picks"],
    )
}
