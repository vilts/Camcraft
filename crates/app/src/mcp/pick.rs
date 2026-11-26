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
use crate::geometry::sketch_chains::ChainId;
use crate::model;
use crate::operation::{self, ChainOverride, ChainSide, FeatureSel, GeoSel, OpParams, SrcRef};
use crate::script::Env;

use flecs_ecs::prelude::{Entity, World};
use serde_json::{json, Map, Value};

/// The string form of a feature id: `face:0x…`, `chain:0x…`,
/// `patch:0x…`, `vertex:0x…`, `sketch_chain:<32-hex>`, `sketch_layer:<layer>`.
pub(super) fn feature_id(f: &FeatureSel) -> String {
    match f {
        FeatureSel::Face(id) => format!("face:{id:#018x}"),
        FeatureSel::Chain(id) => format!("chain:{id:#018x}"),
        FeatureSel::Patch(id) => format!("patch:{id:#018x}"),
        FeatureSel::Vertex(id) => format!("vertex:{id:#018x}"),
        FeatureSel::SketchChain(id) => format!("sketch_chain:{id}"),
        FeatureSel::SketchLayer(layer) => format!("sketch_layer:{layer}"),
    }
}

/// [`feature_id`]'s inverse; anything else is refused by its text.
pub(super) fn parse_feature_id(s: &str) -> Result<FeatureSel, String> {
    let bad = || format!("bad feature id '{s}' - use an id list_features prints, like \"face:0x00ab…\" or \"sketch_chain:…\"");
    let (kind, rest) = s.trim().split_once(':').ok_or_else(bad)?;
    if kind == "sketch_layer" {
        if rest.is_empty() {
            return Err(bad());
        }
        return Ok(FeatureSel::SketchLayer(rest.to_owned()));
    }
    let hex = rest.strip_prefix("0x").or_else(|| rest.strip_prefix("0X")).unwrap_or(rest);
    if kind == "sketch_chain" {
        if hex.is_empty() || hex.len() > 32 {
            return Err(bad());
        }
        let id = u128::from_str_radix(hex, 16).map_err(|_| bad())?;
        return Ok(FeatureSel::SketchChain(ChainId(id)));
    }
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

/// A pickable entity for operation geometry: an imported part model or a placed DXF profile.
pub(super) fn req_pick_target(env: &Env, name: &str) -> Result<Entity, String> {
    let w = &env.world;
    let e = model::find_by_name(w, name)
        .ok_or_else(|| format!("no model named '{name}' - import a model or DXF profile first"))?;
    if crate::sketch::is_sketch(w, &e) {
        return Ok(e);
    }
    req_model(env, name)
}

/// One `{model, id}` pick, checked to be on a part model.
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

fn parse_chain_side(v: &Value) -> Result<Option<ChainSide>, String> {
    match v {
        Value::Null => Ok(None),
        Value::String(s) if s == "inside" => Ok(Some(ChainSide::Inside)),
        Value::String(s) if s == "outside" => Ok(Some(ChainSide::Outside)),
        other => Err(format!("\"side\" must be \"inside\", \"outside\", or null, got {other}")),
    }
}

struct ContourPick {
    sel: GeoSel,
    override_spec: Option<(Option<ChainSide>, bool)>,
}

fn contour_pick_of(env: &Env, v: &Value) -> Result<ContourPick, String> {
    let Value::Object(m) = v else {
        return Err(format!("a pick is an object {{\"model\", \"id\"}}, got {v}"));
    };
    if let Some(k) = m.keys().find(|k| !matches!(k.as_str(), "model" | "id" | "side" | "reverse")) {
        return Err(format!("unexpected pick key \"{k}\" - allowed: model, id, side, reverse"));
    }
    let entity = req_str(m, "model")?;
    req_pick_target(env, entity)?;
    let feature = parse_feature_id(req_str(m, "id")?)?;
    let pick = GeoSel { entity: entity.to_owned(), feature };
    crate::pick::check_pickable(&env.world, &pick)?;
    let side = match m.get("side") {
        None => None,
        Some(sv) => parse_chain_side(sv)?,
    };
    let reverse = match m.get("reverse") {
        None | Some(Value::Null) => false,
        Some(Value::Bool(b)) => *b,
        Some(other) => return Err(format!("\"reverse\" must be a boolean, got {other}")),
    };
    let has_override = side.is_some() || reverse;
    if has_override && !matches!(pick.feature, FeatureSel::SketchChain(_)) {
        return Err(format!(
            "per-chain \"side\" and \"reverse\" require a sketch_chain id, not '{}'",
            feature_id(&pick.feature)
        ));
    }
    Ok(ContourPick { sel: pick, override_spec: has_override.then_some((side, reverse)) })
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

pub(super) fn picks_view(w: &World, items: &[GeoSel]) -> Vec<Value> {
    items
        .iter()
        .map(|g| {
            json!({
                "model": g.entity,
                "id": feature_id(&g.feature),
                "name": crate::pick::geo_label(w, g),
            })
        })
        .collect()
}

/// The feature-kind door: refusals the generation walks make by name,
/// said at the pick instead, before anything lands in the History.
pub(super) fn kind_door(w: &World, label: &str, params: &OpParams, items: &[GeoSel]) -> Result<(), String> {
    let first = |pred: fn(&FeatureSel) -> bool| items.iter().find(|g| pred(&g.feature));
    if matches!(params, OpParams::MultiAxisContour { .. } | OpParams::RotaryFlow { .. }) {
        // Their picks ARE the path: curves only (selection's
        // drive_chains) - the contour's chains machine where they
        // stand, and the flow band's chains span the band.
        if let Some(g) = first(|f| !matches!(f, FeatureSel::Chain(_))) {
            return Err(format!(
                "{} is not an edge chain - a {label} operation machines picked edge chains",
                crate::pick::geo_label(w, g)
            ));
        }
    } else if operation::selection_drives_region(params)
        && let Some(g) = first(|f| matches!(f, FeatureSel::Vertex(_)))
    {
        // A point bounds no region (the selection walk's own refusal).
        return Err(format!(
            "{} is a point - a {label} operation's area needs faces or edge chains",
            crate::pick::geo_label(w, g)
        ));
    }
    Ok(())
}

/// `set_operation_geometry`: the op's source becomes the picked
/// features (`SrcRef::Selection`), one undo step.
pub(super) fn set_operation_geometry(env: &mut Env, a: &Map<String, Value>) -> Result<Value, String> {
    let key = find_op(env, req_str(a, "operation")?)?;
    let raw = match a.get("picks") {
        Some(Value::Array(items)) if !items.is_empty() => items,
        Some(Value::Array(_)) => return Err("\"picks\" needs at least one pick".into()),
        None | Some(Value::Null) => return Err("\"picks\" is required".into()),
        Some(other) => return Err(format!("\"picks\" must be an array of picks, got {other}")),
    };
    let mut items = Vec::with_capacity(raw.len());
    let mut overrides = Vec::new();
    for entry in raw {
        let parsed = contour_pick_of(env, entry)?;
        if let Some((side, reverse)) = parsed.override_spec
            && let FeatureSel::SketchChain(chain) = parsed.sel.feature
        {
            overrides.push(ChainOverride {
                entity: parsed.sel.entity.clone(),
                chain,
                side,
                reverse,
            });
        }
        items.push(parsed.sel);
    }
    let e = model::find_by_name(&env.world, &key).ok_or_else(|| format!("no operation named '{key}'"))?;
    let op = model::operation_of(&env.world, &e).ok_or_else(|| format!("'{key}' is not an operation"))?;
    kind_door(&env.world, op.params.kind().info().label, &op.params, &items)?;
    let cmd = if matches!(op.params, OpParams::Contour { .. }) {
        SetOperationGeometry::with_chain_overrides(&env.world, &key, items, overrides)
    } else if !overrides.is_empty() {
        return Err(format!(
            "per-chain overrides apply to 2D Contour only, not {}",
            op.params.kind()
        ));
    } else {
        SetOperationGeometry::new(&env.world, &key, items)
    }
    .map_err(|e| e.to_string())?;
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
            json!({ "picks": picks_view(w, items) }),
            operation::selection_notes(w, op.setup.as_deref(), items, &op.params),
        ),
    };
    let chain_overrides: Vec<Value> = match &op.params {
        OpParams::Contour { chain_overrides, .. } => chain_overrides
            .iter()
            .map(|o| {
                json!({
                    "model": o.entity,
                    "chain": o.chain.to_string(),
                    "side": o.side.map(|s| match s { ChainSide::Inside => "inside", ChainSide::Outside => "outside" }),
                    "reverse": o.reverse,
                })
            })
            .collect(),
        _ => Vec::new(),
    };
    Ok(json!({
        "operation": key,
        "kind": op.params.kind().info().label,
        "source": source,
        "chain_overrides": chain_overrides,
        "area": super::area::area_view(w, &op.area),
        "do_not_touch": super::do_not_touch::view(w, &op.area.do_not_touch),
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
                "model": { "type": "string", "description": "The model or DXF profile the feature is on." },
                "id": { "type": "string", "description": "A feature id from list_features: face:0x…, chain:0x…, patch:0x…, vertex:0x…, sketch_chain:<hex>, or sketch_layer:<layer>." }
            },
            "required": ["model", "id"],
            "additionalProperties": false
        }
    })
}

pub(super) fn s_list_features() -> Value {
    obj(
        json!({
            "model": { "type": "string", "description": "Model or DXF profile name (see get_job)." },
            "kinds": { "type": "array", "items": { "type": "string", "enum": ["faces", "chains", "patches", "vertices", "layers"] },
                "description": "Which feature kinds to list; default all model kinds or sketch chains and layers." },
            "limit": { "type": "integer", "minimum": 1, "description": "Rows per kind (default 500; 25 with compact)." },
            "compact": { "type": "boolean", "description": "Short rows (id, name and the facts a pick is chosen by; counts in place of loops and bounds) for a small context window (default false)." }
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
            "picks": {
                "type": "array",
                "description": "The features to machine, in order. For 2D Contour on DXF profiles, sketch_chain entries may also carry optional per-chain \"side\" (\"inside\"|\"outside\"|null) and \"reverse\" (boolean) overrides.",
                "items": {
                    "type": "object",
                    "properties": {
                        "model": { "type": "string", "description": "The model or DXF profile the feature is on." },
                        "id": { "type": "string", "description": "A feature id from list_features: face:0x…, chain:0x…, patch:0x…, vertex:0x…, sketch_chain:<hex>, or sketch_layer:<layer>." },
                        "side": { "type": ["string", "null"], "enum": ["inside", "outside", null], "description": "Optional per-chain side override for sketch_chain picks on 2D Contour." },
                        "reverse": { "type": "boolean", "description": "Optional direction flip relative to operation climb for sketch_chain picks on 2D Contour." }
                    },
                    "required": ["model", "id"],
                    "additionalProperties": false
                }
            }
        }),
        &["operation", "picks"],
    )
}
