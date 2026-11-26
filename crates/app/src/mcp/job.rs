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
                "index": crate::setup_offsets::index(w, d),
                "name": setup::wcs_name(w, d),
                // #931: kept as an alias of `name` for older callers.
                "word": setup::wcs_name(w, d),
                "placed": wcs.is_some(),
                "entity": setup::wcs_entity_name(w, d),
                "origin": wcs.as_ref().map(|(_, loc)| vec![fnum(loc.pos.x), fnum(loc.pos.y), fnum(loc.pos.z)]),
                "rotation": wcs.as_ref().map(|(_, loc)| loc.rot.iter().map(|v| fnum(*v)).collect::<Vec<_>>()),
                // #1720: the reversed axes (all false: no flip).
                "flip": super::wcs_flip_tools::flip_view(crate::setup_wcs_flip::of_setup(w, d)),
            });
            json!({
                "name": s,
                "stock": stock.as_ref().map(project::stock_to_json),
                "stock_bounds": model::stock_world_aabb_for(w, d).as_ref().map(aabb),
                "wcs": wcs,
                // #1233: the setup's own post (null: follows the document's) and what it posts with.
                "post": crate::setup_mount::own_post(w, d).as_ref().map(crate::posting::post_ref_label),
                "posts_with": crate::posting::post_ref_label(&crate::setup_mount::post_of(w, d)),
                // #2374: the post its machine asks for, until the agent takes it.
                "post_offer": super::post_offer_view::offer_view(w, d),
                // #1031: the chuck's or collet's rated maximum rpm.
                "workholding_max_rpm": s.as_deref().and_then(|n| crate::setup_workholding::of(w, Some(n))).map(fnum),
                // #3070: the guided sheet job (allowance always shown).
                "sheet_job": s.as_deref().and_then(|n| crate::sheet_job::of(w, n)).as_ref().map(super::sheet_job_tools::job_view),
                // #2410: the router options the setup's machine offers (the UI reads the same list).
                "router_options": crate::router_offer::router_options(w, d).iter().map(|o| o.word()).collect::<Vec<_>>(),
                // #1862: what stands for the setup's model - a part, or the
                // virtual reference of its placed DXF profiles (no solid).
                "model_reference": super::sketch_tools::model_reference(env, d),
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
    let needs_review = model::operation_meta_of(w, e).needs_review;
    json!({
        "name": model::operation_label(w, key, e),
        "key": key,
        "kind": op.as_ref().map(|o| o.params.kind().info().label),
        "setup": op.as_ref().and_then(|o| o.setup.clone()),
        "tool": op.as_ref().map(|o| o.tool.clone()),
        "source": op.as_ref().map(|o| src_view(&o.src)),
        "coolant": op.as_ref().and_then(|o| serde_json::to_value(o.coolant).ok()),
        "status": api::st_str(status),
        "warnings": warnings,
        "needs_review": needs_review,
        "params": op.as_ref().map(|o| project::params_to_json(&o.params)),
        // #1862: an operation on a DXF profile says what it is checked against.
        "checks_against": op.as_ref().and_then(|o| crate::sketch_model::checked_against(w, o)),
    })
}

/// #1162: the document's mounted machine, its whole definition, so an
/// agent reads back what `edit_machine` changed.
fn machine_view(w: &flecs_ecs::prelude::World) -> Value {
    let (path, m) = crate::machine::document_of(w);
    json!({
        "path": (!path.is_empty()).then_some(path),
        "definition": m.as_ref().and_then(|m| super::machine_tools::definition(m).ok()),
    })
}

pub(super) fn get_job(env: &mut Env, a: &Map<String, Value>) -> Result<Value, String> {
    // #1944: the short form for a model's context window.
    let summary = super::compact::flag(a, "summary")?;
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
                m["reduced_fidelity"] = crate::mesh_open::facts(w, e, chord);
            }
            // #1339: a mesh a person vouched for says who.
            if let Some(by) = crate::model_vouch::by_of(w, e) {
                m["fidelity_vouched"] = crate::model_vouch::facts(&by);
            }
            // #1054: and one imported under a non-default STEP facet budget.
            if let Some(b) = crate::model_fidelity::budget_of(w, e) {
                m["facet_budget"] = json!(b);
            }
            // #2463: a linked model names its source file and digest.
            if let Some(s) = crate::model_source::of(w, e) {
                m["source"] = json!({ "path": s.path, "sha256": s.sha256 });
            }
            m
        })
        .collect();
    let tools: Vec<Value> = model::named_entities(w)
        .iter()
        .filter_map(|(n, e)| model::tool_of(w, e).map(|t| {
            let mut row = super::tool_number_tools::number_view(env, n);
            row["definition"] = project::tool_to_json(&t);
            row["name"] = json!(n);
            if let Some(o) = row.as_object_mut() {
                o.remove("tool");
            }
            row["holder"] = super::holders::holder_view(w, n);
            row
        }))
        .collect();
    let ops: Vec<Value> = inventory(env).iter().map(|(k, e, s)| op_full(env, k, e, *s)).collect();
    let mut job = json!({
        "document": env.doc_path.as_ref().map(|p| p.display().to_string()),
        // #902: the document's unit - display and posted program.
        "units": super::units_tools::word(w),
        "active_setup": setup::active(w),
        "work_material": super::cutting_tools::material_view(env),
        "setups": setups_view(env),
        "models": models,
        // #1862: DXF profiles placed on a setup's stock - not models.
        "profiles": super::sketch_tools::profiles_view(env),
        "tools": tools,
        "operations": ops,
        "machine": machine_view(w),
        "can_undo": env.history.can_undo(),
        "can_redo": env.history.can_redo(),
    });
    if summary {
        super::compact::summarize_job(&mut job);
    }
    Ok(job)
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

fn push_missing_fields(fields: &mut Vec<Value>, extra: &[(&str, &str, Value)]) {
    let mut added = false;
    for (name, ty, default) in extra {
        if !fields.iter().any(|f| f["name"] == *name) {
            fields.push(json!({ "name": name, "type": ty, "default": default }));
            added = true;
        }
    }
    if added {
        fields.sort_by(|a, b| a["name"].as_str().cmp(&b["name"].as_str()));
    }
}

pub(super) fn list_operation_types(_env: &mut Env, a: &Map<String, Value>) -> Result<Value, String> {
    let kinds: Vec<Value> = KindInfo::ALL
        .iter()
        .enumerate()
        .map(|(i, k)| {
            let (key, defaults) = kind_defaults(i);
            let mut fields: Vec<Value> = defaults
                .as_object()
                .map(|m| m.iter().map(|(n, v)| json!({ "name": n, "type": json_type(v), "default": v })).collect())
                .unwrap_or_default();
            // `break_through` is skipped when `None` in ParamsDto serialization,
            // so inject it into `list_operation_types` for every through-hole kind (#2441).
            if matches!(key.as_str(), "drill" | "bore" | "thread") {
                push_missing_fields(&mut fields, &[("break_through", "number", Value::Null)]);
            }
            // `WireContourDto` skips empty/default optional fields in project
            // serialization, so inject them into `list_operation_types` (#3217).
            if key == "wire_contour" {
                push_missing_fields(
                    &mut fields,
                    &[
                        ("condition", "string", json!("")),
                        ("picks", "object", json!({})),
                        ("tabs", "object", Value::Null),
                        ("capture", "boolean", json!(false)),
                        ("both_pieces_restrained", "boolean", json!(false)),
                        ("stock_centered_in_xy_travel", "boolean", json!(false)),
                        ("taper_deg", "number", json!(0.0)),
                        ("burnout_clearance", "number", Value::Null),
                        ("clearance", "number", json!(crate::operation::DEFAULT_CLEARANCE)),
                        ("skims", "array", json!([])),
                        ("four_axis", "object", Value::Null),
                        ("sharp_corners", "boolean", json!(false)),
                        ("lead_arc_radius", "number", Value::Null),
                    ],
                );
            }
            if matches!(key.as_str(), "sinker_burn" | "sinker_side_burn") {
                fields.push(json!({ "name": "cavity_outline", "type": "array", "default": Value::Null }));
                fields.sort_by(|a, b| a["name"].as_str().cmp(&b["name"].as_str()));
            }
            // The zero default is omitted by ParamsDto to preserve old
            // project bytes, but it is still an authored operation
            // parameter on the MCP surface (#3153).
            if key == "chamfer2_d"
                && !fields.iter().any(|f| f["name"] == "minimum_cone_radius")
            {
                fields.push(json!({
                    "name": "minimum_cone_radius",
                    "type": "number",
                    "default": 0.0
                }));
                fields.sort_by(|a, b| a["name"].as_str().cmp(&b["name"].as_str()));
            }
            // #3449: every accepted field, optional ones and enum values too.
            let traced = super::kind_fields::complete(&key, &defaults, &mut fields);
            let mut kind = json!({
                "label": k.label,
                "key": key,
                "section": format!("{:?}", k.section),
                "tool_kinds": k.tool_kinds.iter().map(|t| format!("{t:?}")).collect::<Vec<_>>(),
                "mounts_tool": k.mounts_tool(),
                "fields": fields,
                "start_exit": start_exit(k),
            });
            if !traced {
                kind["fields_incomplete"] = json!(true);
            }
            if !k.mounts_tool() {
                kind["note"] = json!("mounts no library tool: add_operation takes no \"tool\" for this kind");
            }
            kind
        })
        .collect();
    // #1944: one kind, or one section, for a small context window.
    let kinds = super::compact::filter_kinds(kinds, a)?;
    Ok(json!({
        "kinds": kinds,
        "note": "Each kind lists every field it accepts. \"optional\": true marks a field absent from the defaults: it is off until a params patch sets it. \"values\" lists the words of an enum field; \"shape\" spells an object or array field (key? is optional). Type \"any\" is a value that reads itself, such as a height: a number or {reference, offset}.",
    }))
}

/// #3324: does the kind take a start / exit pick, and where; or why not.
fn start_exit(k: &KindInfo) -> Value {
    use crate::path_picks::PickRole;
    let cap = k.pick();
    let mut out = json!({ "start": cap.takes(PickRole::Start), "exit": cap.takes(PickRole::Exit) });
    match cap {
        operation::PickCapability::NotTaken(why) => out["why_not"] = json!(why),
        // #3331: a turning pick is [x, y, z] - the lathe (Z, X) plane needs the z.
        operation::PickCapability::Meridian => {
            out["field"] = json!("picks");
            out["plane"] = json!("lathe (Z, X): each pick states its z; the radius is read about the turning axis");
        }
        _ => out["field"] = json!("picks"),
    }
    out
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
        Warning::Gouge { at } | Warning::StockChamferGouge { at } | Warning::StockDeburrGouge { at } => ("gouge", *at),
        Warning::ShankHit { at, .. } => ("shank_hit", *at),
        Warning::HolderHit { at, .. } => ("holder_hit", *at),
        Warning::SweepHit { at, .. } => ("sweep_hit", *at),
        Warning::RapidInStock { at } => ("rapid_in_stock", *at),
        Warning::RapidInPart { at } => ("rapid_in_part", *at),
        Warning::FirstTraverseLow { at, .. } => ("first_traverse_low", *at),
        Warning::FirstTraverseUnderTable { at, .. } => ("first_traverse_under_table", *at),
        Warning::FixtureHit { at, .. } => ("fixture_hit", *at),
        Warning::AvoidHit { at, .. } => ("avoid_hit", *at),
        Warning::NonFinite { at } => ("non_finite", *at),
        Warning::Envelope { at, .. } => ("envelope", *at),
        Warning::RestUnjudged { at, .. } => ("rest_unjudged", *at),
        Warning::Scallop { at, .. } => ("scallop", *at),
        Warning::ProbeSearch { at, .. } => ("probe_search", *at),
        Warning::SinkerFinishOrder { at, .. } => ("sinker_finish_order", *at),
        Warning::DrillUndersize { at, .. } => ("drill_undersize", *at),
        Warning::UndercutLift { at } => ("undercut_lift", *at),
        Warning::StrategyFallback { at, .. } => ("strategy_fallback", *at),
        Warning::FluteShort { at, .. } => ("flute_short", *at),
        Warning::PlungeTooDeep { at, .. } => ("plunge_too_deep", *at),
        Warning::EntryNotRecommended { at, .. } => ("entry_not_recommended", *at),
        Warning::EntryStandIn { at, .. } => ("entry_stand_in", *at),
        Warning::PlungeIntoStock { at } => ("plunge_into_stock", *at),
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
    // #3456: a program law's warning (a plasma tip-up, a kerf crossing)
    // names its op but no move; the law decides it holds nothing.
    if let Some(warnings) = report["gate"]["warnings"].as_array() {
        for w in warnings {
            findings.push(json!({ "op": w["op"], "move": null, "kind": w["kind"], "message": w["message"], "blocks_post": false }));
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

#[cfg(test)]
mod tests {
    use super::{Warning, finding};
    use crate::verify::FallbackCause;

    #[test]
    fn part_and_stock_rapids_keep_their_kind_message_and_blocking_state() {
        for (warning, kind, message) in [
            (Warning::RapidInPart { at: 7 }, "rapid_in_part", "tool meets the part on rapid move 7"),
            (Warning::RapidInStock { at: 7 }, "rapid_in_stock", "rapid move 7 travels through stock (below the clearance plane, or at it below the stock top)"),
        ] {
            let f = finding("op", &warning);
            assert_eq!(f["kind"], kind, "{f}");
            assert_eq!(f["message"], message, "{f}");
            assert_eq!(f["move"], 7, "{f}");
            assert_eq!(f["blocks_post"], true, "{f}");
        }
    }

    /// #3542: the pass-start cause keeps the MCP kind `strategy_fallback`
    /// and stays advisory; only its message names the other cause.
    #[test]
    fn a_pass_start_fallback_keeps_the_strategy_fallback_kind() {
        for cause in [FallbackCause::Adaptive, FallbackCause::PassStart] {
            let f = finding("op", &Warning::StrategyFallback { at: 3, areas: 1, cause });
            assert_eq!(f["kind"], "strategy_fallback", "{f}");
            assert_eq!(f["move"], 3, "{f}");
            assert_eq!(f["blocks_post"], false, "{f}");
        }
    }
}
