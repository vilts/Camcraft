//! The MCP fixture tools (#181): mark an imported model as workholding,
//! place it, unmark it, and read the verifier's live fixture findings
//! back as data. Marking and moving are the fixture commands the app's
//! fixtures page issues, one History step each; the findings are the
//! post gate's own workholding walk (`posting::fixture_findings`), so
//! this tool and the post can never disagree about the steel.
//!
//! Provenance: our own tool surface over this repo's fixture commands
//! and verifier; fixtures as imported solids is plain shop practice.

use super::setup_tools::{cmd_err, loc_view, obj, opt_nums, point, setup_or_active};
use super::tools::{req_str, Tool};
use crate::commands::{Locate, MoveFixture, TagFixture, UntagFixture};
use crate::fixture_locate::{self, LocateBody, LocateOffset, LocateSpec, Mate, Mover};
use crate::operation::GeoSel;
use crate::fixture;
use crate::model;
use crate::schema::Location;
use crate::script::{fnum, Env};
use crate::setup;
use crate::verify::Warning;

use serde_json::{json, Map, Value};

pub(super) const TOOLS: &[Tool] = &[
    Tool { name: "add_fixture", description: "Mark an imported model as a fixture (workholding steel the verifier checks every toolpath against) of a setup - the active one by default. Import the model with import_model first.", schema: s_add_fixture, run: add_fixture },
    Tool { name: "remove_fixture", description: "Unmark a fixture: the model stays, the verifier stops checking against it.", schema: s_fixture_name, run: remove_fixture },
    Tool { name: "move_fixture", description: "Place a fixture: position [x, y, z] in mm and an optional unit quaternion rotation [x, y, z, w] (omitted keeps the current one).", schema: s_move_fixture, run: move_fixture },
    Tool { name: "locate_part", description: "Locate the part in a fixture (or, with move \"fixture\", the fixture on the part) by mates: each mate pairs a part face or vertex id (or, with body \"stock\", a face or vertex id of the setup's stock) with the fixture face or vertex it sits on (list_features prints the ids). One to three mates - floor, fixed jaw, stop (3-2-1) - then an optional offset [dx, dy, dz] mm and turn (degrees about the first face mate's normal). One undoable step; refusals and an interpenetration warning are said by name.", schema: s_locate_part, run: locate_part },
    Tool { name: "get_fixture_findings", description: "The verifier's fixture check, live: every setup's fixtures and machine table with their boxes, and each collision of a toolpath with them (op, move index, struck part, message). Needs every operation generated.", schema: s_empty, run: get_fixture_findings },
];

fn add_fixture(env: &mut Env, a: &Map<String, Value>) -> Result<Value, String> {
    let name = req_str(a, "model")?;
    let target = setup_or_active(env, a)?;
    let cmd = TagFixture::new(&env.world, name, target.as_deref()).map_err(cmd_err)?;
    env.history.execute(&env.world, cmd);
    Ok(json!({ "fixture": name, "setup": target }))
}

fn remove_fixture(env: &mut Env, a: &Map<String, Value>) -> Result<Value, String> {
    let name = req_str(a, "name")?;
    let cmd = UntagFixture::new(&env.world, name).map_err(cmd_err)?;
    env.history.execute(&env.world, cmd);
    Ok(json!({ "removed": name }))
}

fn move_fixture(env: &mut Env, a: &Map<String, Value>) -> Result<Value, String> {
    let name = req_str(a, "name")?;
    let pos = opt_nums::<3>(a, "position")?.ok_or("\"position\" is required")?;
    let current = model::find_by_name(&env.world, name).and_then(|e| model::transform_of(&env.world, &e));
    let rot = match opt_nums::<4>(a, "rotation")? {
        Some(q) => q,
        None => current.unwrap_or_default().rot,
    };
    let cmd = MoveFixture::new(&env.world, name, Location { pos: point(pos), rot }).map_err(cmd_err)?;
    let warning = cmd.warning().map(str::to_owned);
    env.history.execute(&env.world, cmd);
    let placed = model::find_by_name(&env.world, name).and_then(|e| model::transform_of(&env.world, &e));
    let mut out = loc_view(&placed.unwrap_or_default());
    out["fixture"] = json!(name);
    out["warning"] = json!(warning);
    Ok(out)
}

/// #561: one locate, as the Fixtures page applies it.
fn locate_part(env: &mut Env, a: &Map<String, Value>) -> Result<Value, String> {
    let part = req_str(a, "part")?;
    let steel = req_str(a, "fixture")?;
    let mover = match super::tools::opt_str(a, "move")? {
        None | Some("part") => Mover::Part,
        Some("fixture") => Mover::Fixture,
        Some(other) => return Err(format!("\"move\" is \"part\" or \"fixture\", not \"{other}\"")),
    };
    // #2173: the mates' first ids are on the part, or on its setup's stock.
    let (body, held) = match super::tools::opt_str(a, "body")? {
        None | Some("part") => (LocateBody::Part, part.to_owned()),
        Some("stock") => (
            LocateBody::Stock,
            fixture_locate::stock_of(&env.world, steel)
                .ok_or_else(|| format!("the setup of '{steel}' has no stock to locate - set the stock first"))?,
        ),
        Some(other) => return Err(format!("\"body\" is \"part\" or \"stock\", not \"{other}\"")),
    };
    let Some(Value::Array(items)) = a.get("mates") else {
        return Err("\"mates\" is required: an array of {\"part\": id, \"fixture\": id}".into());
    };
    let side = |m: &Map<String, Value>, key: &str, entity: &str| -> Result<GeoSel, String> {
        let feature = super::pick::parse_feature_id(req_str(m, key)?)?;
        let g = GeoSel { entity: entity.to_owned(), feature };
        // #692: the one pick rule - a mate id is taken only if a click could land on it.
        crate::pick::check_pickable(&env.world, &g)?;
        Ok(g)
    };
    let mates = items
        .iter()
        .map(|v| match v {
            Value::Object(m) => Ok(Mate { part: side(m, "part", &held)?, fixture: side(m, "fixture", steel)? }),
            other => Err(format!("a mate is an object {{\"part\", \"fixture\"}}, got {other}")),
        })
        .collect::<Result<Vec<_>, String>>()?;
    let [dx, dy, dz] = opt_nums::<3>(a, "offset")?.unwrap_or([0.0; 3]);
    let turn_deg = match a.get("turn") {
        None | Some(Value::Null) => 0.0,
        Some(v) => v.as_f64().ok_or("\"turn\" must be a number of degrees")?,
    };
    let spec = LocateSpec { part: part.to_owned(), fixture: steel.to_owned(), body, mates, offset: LocateOffset { dx, dy, dz, turn_deg } };
    let cmd = Locate::new(&env.world, &spec, mover).map_err(cmd_err)?;
    let warning = cmd.warning().map(str::to_owned);
    env.history.execute(&env.world, cmd);
    let moved = if mover == Mover::Part { part } else { steel };
    let placed = model::find_by_name(&env.world, moved).and_then(|e| model::transform_of(&env.world, &e));
    let mut out = loc_view(&placed.unwrap_or_default());
    out["moved"] = json!(moved);
    out["warning"] = json!(warning);
    Ok(out)
}

fn bounds(min: [f64; 3], max: [f64; 3]) -> Value {
    json!({ "min": min.map(fnum), "max": max.map(fnum) })
}

/// Every setup context's steel: the tagged models, then the table.
fn workholding_view(env: &Env) -> Vec<Value> {
    let w = &env.world;
    let mut contexts: Vec<Option<String>> = setup::names(w).into_iter().map(Some).collect();
    if contexts.is_empty() {
        contexts.push(None);
    }
    contexts
        .iter()
        .map(|s| {
            let d = s.as_deref();
            let fixtures: Vec<Value> = fixture::tagged(w, d)
                .iter()
                .map(|(n, e)| {
                    let b = model::entity_world_aabb(w, e);
                    json!({
                        "name": n,
                        "fixed": fixture::def_of(w, e).is_some_and(|f| f.fixed),
                        "bounds": b.map(|b| bounds([b.min.x, b.min.y, b.min.z], [b.max.x, b.max.y, b.max.z])),
                    })
                })
                .collect();
            let table = fixture::table_box(w, d).map(|b| bounds([b.min.x, b.min.y, b.min.z], [b.max.x, b.max.y, b.max.z]));
            let machine = crate::machine::of_setup(w, d).1.map(|m| m.model);
            json!({ "setup": s, "machine": machine, "fixtures": fixtures, "table": table })
        })
        .collect()
}

fn get_fixture_findings(env: &mut Env, _a: &Map<String, Value>) -> Result<Value, String> {
    let w = &env.world;
    let mut out = json!({
        "workholding": workholding_view(env),
        // The active setup's machine (kept for older callers); each
        // workholding entry names its own setup's machine (#388).
        "machine": crate::machine::of(w).1.map(|m| m.model),
    });
    match crate::posting::fixture_findings(w) {
        Ok(found) => {
            let findings: Vec<Value> = found
                .hits
                .iter()
                .map(|(key, hit)| {
                    let label = model::find_by_name(w, key).map_or_else(|| key.clone(), |e| model::operation_label(w, key, &e));
                    let mut f = super::job::finding(&label, hit);
                    f["key"] = json!(key);
                    if let Warning::FixtureHit { fixture_part, .. } = hit {
                        f["part"] = serde_json::to_value(fixture_part).unwrap_or(Value::Null);
                    }
                    f
                })
                .collect();
            out["judged"] = json!(true);
            out["ok"] = json!(findings.is_empty() && found.unjudged.is_empty());
            out["findings"] = Value::Array(findings);
            // #368: an op the walk could not judge is a problem, by name.
            out["unjudged"] = json!(found.unjudged);
        }
        // No verdict over stale paths: said, never read as "clear".
        Err(reason) => {
            out["judged"] = json!(false);
            out["ok"] = json!(false);
            out["findings"] = json!([]);
            out["skipped"] = json!(reason);
        }
    }
    Ok(out)
}

// ---- input schemas ----------------------------------------------------

fn s_empty() -> Value {
    obj(json!({}), &[])
}

fn s_add_fixture() -> Value {
    obj(
        json!({
            "model": { "type": "string", "description": "An imported model's name." },
            "setup": { "type": "string", "description": "The declared setup it holds the part in; defaults to the active one." }
        }),
        &["model"],
    )
}

fn s_fixture_name() -> Value {
    obj(json!({ "name": { "type": "string", "description": "A fixture's name." } }), &["name"])
}

fn s_locate_part() -> Value {
    obj(
        json!({
            "part": { "type": "string", "description": "The part (an imported model) to locate." },
            "fixture": { "type": "string", "description": "The fixture it sits in." },
            "mates": { "type": "array", "minItems": 1, "maxItems": 3,
                "items": { "type": "object", "properties": {
                    "part": { "type": "string", "description": "A face or vertex id (\"face:0x…\", \"vertex:0x…\") on the held body: the part, or its stock with body \"stock\"." },
                    "fixture": { "type": "string", "description": "The fixture face or vertex id it meets." } },
                    "required": ["part", "fixture"] },
                "description": "In order: the floor, the fixed jaw, the stop." },
            "offset": { "type": "array", "items": {"type": "number"}, "minItems": 3, "maxItems": 3,
                "description": "A nudge [dx, dy, dz] mm after the mates (the part relative to the fixture)." },
            "turn": { "type": "number", "description": "Degrees about the first face mate's normal, after the mates." },
            "move": { "type": "string", "enum": ["part", "fixture"], "description": "Which body moves; the part by default." },
            "body": { "type": "string", "enum": ["part", "stock"], "description": "Which body the mates' first ids are on: the part (default), or the stock of the fixture's setup. With \"stock\" the part moves and its stock follows it onto the mates." }
        }),
        &["part", "fixture", "mates"],
    )
}

fn s_move_fixture() -> Value {
    obj(
        json!({
            "name": { "type": "string", "description": "A fixture's name." },
            "position": { "type": "array", "items": {"type": "number"}, "minItems": 3, "maxItems": 3,
                "description": "World position [x, y, z] in mm." },
            "rotation": { "type": "array", "items": {"type": "number"}, "minItems": 4, "maxItems": 4,
                "description": "Unit quaternion [x, y, z, w]; omitted keeps the current rotation." }
        }),
        &["name", "position"],
    )
}
