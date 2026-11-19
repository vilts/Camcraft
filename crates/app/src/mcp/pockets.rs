//! `detect_pockets` (#35): a model's recognised pockets and slots, and -
//! with `create` - one of them turned into an operation through
//! `CreateOperation`, one undo step, the way `detect_holes` fills a drill.
//!
//! Provenance: our own tool surface over `recognize` (this repo's M13.6
//! design); no vendor material consulted.

use super::pick::{feature_id, req_model};
use super::tools::{opt_str, req_str};
use crate::commands::CreateOperation;
use crate::geo2d::Point2;
use crate::operation::{FeatureSel, KindInfo};
use crate::recognize::{self, Pocket, PocketKind, SlotEnd};
use crate::script::{fnum, Env};

use serde_json::{json, Map, Value};

fn pt(p: Point2) -> Value {
    json!([fnum(p.x), fnum(p.y)])
}

fn end_word(e: SlotEnd) -> &'static str {
    match e {
        SlotEnd::Open => "open",
        SlotEnd::Round => "round",
        SlotEnd::Square => "square",
    }
}

fn pocket_view(w: &flecs_ecs::prelude::World, model_name: &str, i: usize, p: &Pocket) -> Value {
    let kind = match p.kind {
        PocketKind::Closed => "closed_pocket",
        PocketKind::Open => "open_pocket",
        PocketKind::Slot => "slot",
    };
    let op = recognize::op_kind_of(p);
    let floor_sel = crate::operation::GeoSel {
        entity: model_name.to_owned(),
        feature: FeatureSel::Face(p.floor),
    };
    json!({
        "index": i,
        "kind": kind,
        "floor": feature_id(FeatureSel::Face(p.floor)),
        "floor_name": crate::pick::geo_label(w, &floor_sel),
        "floor_z": fnum(p.floor_z),
        "top_z": fnum(p.top_z),
        "depth": fnum(p.depth),
        "area": fnum(p.area),
        "boundary": p.boundary.iter().map(|q| pt(*q)).collect::<Vec<_>>(),
        "islands": p.islands,
        "min_corner_radius": p.min_corner_radius.map(fnum),
        "open_spans": p.open_spans.iter().map(|s| json!([pt(s[0]), pt(s[1])])).collect::<Vec<_>>(),
        "slot": p.slot.map(|s| json!({
            "a": pt(s.a), "b": pt(s.b), "width": fnum(s.width), "length": fnum(s.length),
            "ends": [end_word(s.ends[0]), end_word(s.ends[1])],
        })),
        "operation_kind": KindInfo::ALL.get(op as usize).map_or("Operation", |k| k.label),
    })
}

fn index_of(a: &Map<String, Value>, count: usize) -> Result<Option<usize>, String> {
    match a.get("create") {
        None | Some(Value::Null) => Ok(None),
        Some(v) => match v.as_u64() {
            Some(n) if (n as usize) < count => Ok(Some(n as usize)),
            Some(n) => Err(format!("\"create\" is index {n}, but the model has {count} recognised pocket(s) or slot(s)")),
            None => Err(format!("\"create\" must be a feature index (a whole number), got {v}")),
        },
    }
}

pub(super) fn detect_pockets(env: &mut Env, a: &Map<String, Value>) -> Result<Value, String> {
    let name = req_str(a, "model")?;
    req_model(env, name)?;
    let found = recognize::pockets_of(&env.world, name)?;
    let rows: Vec<Value> = found.iter().enumerate().map(|(i, p)| pocket_view(&env.world, name, i, p)).collect();
    let mut out = json!({ "model": name, "frame": "world", "count": found.len(), "features": rows });
    let pick = index_of(a, found.len())?;
    let tool = opt_str(a, "tool")?;
    let op_name = opt_str(a, "name")?;
    let Some(i) = pick else {
        if tool.is_some() || op_name.is_some() {
            return Err("\"tool\" and \"name\" go with \"create\" (the feature index to machine)".into());
        }
        return Ok(out);
    };
    let tool = tool.ok_or("\"tool\" is required with \"create\"")?;
    let p = found.get(i).ok_or_else(|| format!("no recognised feature {i}"))?;
    let plan = recognize::plan_operation(&env.world, p, tool, op_name)?;
    let cmd = CreateOperation::new(&env.world, &plan.name, tool, plan.src, plan.params).map_err(|e| e.to_string())?;
    env.history.execute(&env.world, cmd);
    out["created"] = json!({ "operation": plan.name, "kind": plan.label, "feature": i, "status": "never" });
    Ok(out)
}

pub(super) fn s_detect_pockets() -> Value {
    json!({
        "type": "object",
        "properties": {
            "model": { "type": "string", "description": "Model name (see get_job)." },
            "create": { "type": "integer", "minimum": 0,
                "description": "Optional: the index of a listed feature to machine - adds a Pocket (or a Slot for a round- or open-ended slot) with its area and depths from the feature, one undo step." },
            "tool": { "type": "string", "description": "The document tool for the new operation (required with create)." },
            "name": { "type": "string", "description": "Name for the new operation (default Pocket1, Slot1, ...)." }
        },
        "required": ["model"],
        "additionalProperties": false,
    })
}
