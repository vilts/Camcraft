//! The read side of the MCP tools (#128): the job as JSON, the kind and
//! tool catalogs, and the verifier's findings as data. Nothing here
//! writes the world; every number is read from the seams the app's own
//! views read (the operation order, the file DTOs, the post gate).
//!
//! Provenance: our own views over this repo's document model.

use crate::geometry::Aabb;
use crate::model;
use crate::operation::{self, KindInfo, SrcRef};
use crate::project;
use crate::schema::WCS_SLOTS;
use crate::script::{api, fnum, inventory, Env};
use crate::setup;
use crate::verify::Warning;

use serde_json::{json, Map, Value};

fn aabb(b: &Aabb) -> Value {
    json!({
        "min": [fnum(b.min.x), fnum(b.min.y), fnum(b.min.z)],
        "max": [fnum(b.max.x), fnum(b.max.y), fnum(b.max.z)],
    })
}

fn src_view(src: &SrcRef) -> Value {
    match src {
        SrcRef::Stock => json!("stock"),
        SrcRef::Entity(n) => json!({ "model": n }),
        SrcRef::Selection(items) => json!({ "selection": items.len() }),
    }
}

/// The setup slice of the job: the implicit first setup (`null`) when
/// none is declared, else every declared one, each with its stock and
/// its one WCS (#342) with the word the post writes for it.
fn setups_view(env: &Env) -> Vec<Value> {
    let w = &env.world;
    let mut declared: Vec<Option<String>> = setup::names(w).into_iter().map(Some).collect();
    if declared.is_empty() {
        declared.push(None);
    }
    declared
        .iter()
        .map(|s| {
            let d = s.as_deref();
            let stock = setup::stock_entity(w, d).and_then(|e| model::stock_def_of(w, &e));
            // #342: one WCS per setup; the word is only its post label.
            let wcs = setup::wcs_entity(w, d).and_then(|e| model::transform_of(w, &e).map(|loc| (e, loc)));
            let wcs = json!({
                "label": setup::wcs_label(w, d),
                "word": WCS_SLOTS.get(setup::wcs_slot(w, d)).copied().unwrap_or("G54"),
                "placed": wcs.is_some(),
                "entity": setup::wcs_entity_name(w, d),
                "origin": wcs.as_ref().map(|(_, loc)| vec![fnum(loc.pos.x), fnum(loc.pos.y), fnum(loc.pos.z)]),
                "rotation": wcs.as_ref().map(|(_, loc)| loc.rot.iter().map(|v| fnum(*v)).collect::<Vec<_>>()),
            });
            json!({
                "name": s,
                "stock": stock.as_ref().map(project::stock_to_json),
                "stock_bounds": model::stock_world_aabb_for(w, d).as_ref().map(aabb),
                "wcs": wcs,
                // #1031: the chuck's or collet's rated maximum rpm.
                "workholding_max_rpm": s.as_deref().and_then(|n| crate::setup_workholding::of(w, Some(n))).map(fnum),
            })
        })
        .collect()
}

/// One operation with its whole definition: what `edit_operation`
/// patches is `params[<kind key>]`.
fn op_full(env: &Env, key: &str, e: &flecs_ecs::prelude::Entity, status: operation::OpStatus) -> Value {
    let w = &env.world;
    let op = model::operation_of(w, e);
    let warnings = model::toolpath_of(w, e).map_or(0, |t| t.warnings.len());
    json!({
        "name": model::operation_label(w, key, e),
        "key": key,
        "kind": op.as_ref().map(|o| o.params.kind().info().label),
        "setup": op.as_ref().and_then(|o| o.setup.clone()),
        "tool": op.as_ref().map(|o| o.tool.clone()),
        "source": op.as_ref().map(|o| src_view(&o.src)),
        "status": api::st_str(status),
        "warnings": warnings,
        "params": op.as_ref().map(|o| project::params_to_json(&o.params)),
    })
}

pub(super) fn get_job(env: &mut Env, _a: &Map<String, Value>) -> Result<Value, String> {
    let w = &env.world;
    let models: Vec<Value> = model::model_entities(w)
        .iter()
        .filter(|(_, e)| !model::is_stock(w, e))
        .map(|(n, e)| {
            let mut m = json!({
                "name": n,
                "facets": model::facets_of(w, e).map_or(0, |f| f.len()),
                "bounds": model::entity_world_aabb(w, e).as_ref().map(aabb),
                "fixture": model::is_fixture(w, e),
            });
            // #864: only a model imported coarser than promised says so.
            if let Some(chord) = crate::model_fidelity::chord_of(w, e) {
                m["reduced_fidelity"] = crate::model_fidelity::facts(w, chord);
            }
            // #1054: and one imported under a non-default STEP facet budget.
            if let Some(b) = crate::model_fidelity::budget_of(w, e) {
                m["facet_budget"] = json!(b);
            }
            m
        })
        .collect();
    let tools: Vec<Value> = model::named_entities(w)
        .iter()
        .filter_map(|(n, e)| model::tool_of(w, e).map(|t| json!({ "name": n, "definition": project::tool_to_json(&t), "holder": super::holders::holder_view(w, n) })))
        .collect();
    let ops: Vec<Value> = inventory(env).iter().map(|(k, e, s)| op_full(env, k, e, *s)).collect();
    Ok(json!({
        "document": env.doc_path.as_ref().map(|p| p.display().to_string()),
        "active_setup": setup::active(w),
        "setups": setups_view(env),
        "models": models,
        "tools": tools,
        "operations": ops,
        "can_undo": env.history.can_undo(),
        "can_redo": env.history.can_redo(),
    }))
}

/// The kind's file key (`"face"`, `"pocket3d"` ...) and its default
/// field object, read off the same default `add_operation` seeds.
pub(super) fn kind_defaults(index: usize) -> (String, Value) {
    let p = super::edit::seed(index);
    match project::params_to_json(&p) {
        Value::Object(m) => m.into_iter().next().unwrap_or((String::new(), Value::Null)),
        _ => (String::new(), Value::Null),
    }
}

fn json_type(v: &Value) -> &'static str {
    match v {
        Value::Null => "null",
        Value::Bool(_) => "boolean",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    }
}

pub(super) fn list_operation_types(_env: &mut Env, _a: &Map<String, Value>) -> Result<Value, String> {
    let kinds: Vec<Value> = KindInfo::ALL
        .iter()
        .enumerate()
        .map(|(i, k)| {
            let (key, defaults) = kind_defaults(i);
            let fields: Vec<Value> = defaults
                .as_object()
                .map(|m| m.iter().map(|(n, v)| json!({ "name": n, "type": json_type(v), "default": v })).collect())
                .unwrap_or_default();
            json!({
                "label": k.label,
                "key": key,
                "section": format!("{:?}", k.section),
                "tool_kinds": k.tool_kinds.iter().map(|t| format!("{t:?}")).collect::<Vec<_>>(),
                "fields": fields,
            })
        })
        .collect();
    Ok(json!({
        "kinds": kinds,
        "note": "Fields absent from the defaults are optional and off; set them in a params patch to turn them on.",
    }))
}

pub(super) fn list_tool_catalog(_env: &mut Env, _a: &Map<String, Value>) -> Result<Value, String> {
    let tools: Vec<Value> = crate::tools::catalog()
        .iter()
        .map(|(n, t)| json!({ "name": n, "definition": project::tool_to_json(t) }))
        .collect();
    Ok(json!({ "tools": tools }))
}

/// One verifier warning as data. Exhaustive on purpose: a new warning
/// kind fails to compile here until it says what it is called.
pub(super) fn finding(op: &str, w: &Warning) -> Value {
    let (kind, at) = match w {
        Warning::Crossing { moves } => ("crossing", moves[0]),
        Warning::Gouge { at } => ("gouge", *at),
        Warning::ShankHit { at, .. } => ("shank_hit", *at),
        Warning::HolderHit { at, .. } => ("holder_hit", *at),
        Warning::SweepHit { at, .. } => ("sweep_hit", *at),
        Warning::RapidInStock { at } => ("rapid_in_stock", *at),
        Warning::FixtureHit { at, .. } => ("fixture_hit", *at),
        Warning::NonFinite { at } => ("non_finite", *at),
        Warning::Envelope { at, .. } => ("envelope", *at),
        Warning::RestUnjudged { at, .. } => ("rest_unjudged", *at),
        Warning::Scallop { at, .. } => ("scallop", *at),
        Warning::ProbeSearch { at, .. } => ("probe_search", *at),
    };
    json!({
        "op": op,
        "move": at,
        "kind": kind,
        "message": w.to_string(),
        "blocks_post": w.blocks_post(),
    })
}

pub(super) fn verify(env: &mut Env, a: &Map<String, Value>) -> Result<Value, String> {
    let name = super::tools::opt_str(a, "name")?;
    let args = name.map(|n| vec![json!(n)]).unwrap_or_default();
    let mut report = api::v_verify(env, args)?;
    let scope: Vec<String> = report["operations"]
        .as_array()
        .map(|ops| ops.iter().filter_map(|o| o["key"].as_str().map(str::to_owned)).collect())
        .unwrap_or_default();
    let mut findings = Vec::new();
    for key in &scope {
        let Some(e) = model::find_by_name(&env.world, key) else { continue };
        let label = model::operation_label(&env.world, key, &e);
        if let Some(t) = model::toolpath_of(&env.world, &e) {
            findings.extend(t.warnings.iter().map(|w| finding(&label, w)));
        }
    }
    // The program gate's problems speak of the whole program, not one
    // move: they ride as findings with no op and no move index.
    if let Some(problems) = report["gate"]["problems"].as_array() {
        for p in problems {
            findings.push(json!({ "op": null, "move": null, "kind": "post_gate", "message": p, "blocks_post": true }));
        }
    }
    report["findings"] = Value::Array(findings);
    Ok(report)
}
