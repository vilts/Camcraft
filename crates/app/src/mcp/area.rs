//! `set_operation_area` (#180): an operation's area controls - tool
//! containment, offset, corner style, extension, extra include / exclude
//! boundaries and the open boundary - written through
//! `EditOperationParams::with_area`, one undoable step whose door is the
//! same one the operation page's Apply goes through.
//!
//! Provenance: our own JSON surface over `operation::AreaControls`
//! (this repo's M11.5 design); no vendor material consulted.

use super::edit::find_op;
use super::pick::{picks_of, picks_view, pick_list_schema};
use super::tools::req_str;
use crate::commands::EditOperationParams;
use crate::model;
use crate::operation::{
    AreaControls, BoundaryMode, BoundaryShape, ChainSide, Containment, CornerStyle, ExtraBoundary, OpenBoundary,
};
use crate::script::{fnum, Env};

use flecs_ecs::prelude::World;
use serde_json::{json, Map, Value};

/// The controls as the tools show and take them.
pub(super) fn area_view(w: &World, a: &AreaControls) -> Value {
    json!({
        "containment": a.containment.map(|c| match c {
            Containment::Inside => "inside",
            Containment::On => "on",
            Containment::Outside => "outside",
        }),
        "offset": fnum(a.offset),
        "corner": match a.corner { CornerStyle::Round => "round", CornerStyle::Sharp => "sharp" },
        "extension": fnum(a.extension),
        "extra": a.extra.iter().map(|b| {
            let mode = match b.mode { BoundaryMode::Include => "include", BoundaryMode::Exclude => "exclude" };
            match &b.shape {
                BoundaryShape::Picks(p) => json!({ "mode": mode, "from": "picks", "picks": picks_view(w, p) }),
                BoundaryShape::Stock => json!({ "mode": mode, "from": "stock" }),
                BoundaryShape::Silhouette => json!({ "mode": mode, "from": "silhouette" }),
            }
        }).collect::<Vec<_>>(),
        "open": a.open.as_ref().map(|o| json!({
            "picks": picks_view(w, &o.picks),
            "side": match o.side { ChainSide::Inside => "inside", ChainSide::Outside => "outside" },
        })),
    })
}

/// Refuse a key the shape does not name: a typo must not read as done.
fn only_keys(m: &Map<String, Value>, known: &[&str], what: &str) -> Result<(), String> {
    match m.keys().find(|k| !known.contains(&k.as_str())) {
        Some(k) => Err(format!("{what} has no field \"{k}\" - fields: {}", known.join(", "))),
        None => Ok(()),
    }
}

fn word<'a>(m: &'a Map<String, Value>, key: &str, words: &[&str]) -> Result<Option<&'a str>, String> {
    match m.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(s)) if words.contains(&s.as_str()) => Ok(Some(s)),
        Some(other) => Err(format!("\"{key}\" is one of {}, got {other}", words.join(", "))),
    }
}

fn number(m: &Map<String, Value>, key: &str) -> Result<f64, String> {
    match m.get(key) {
        None | Some(Value::Null) => Ok(0.0),
        Some(v) => v.as_f64().ok_or_else(|| format!("\"{key}\" must be a number in mm, got {v}")),
    }
}

fn extra_of(env: &Env, v: &Value) -> Result<ExtraBoundary, String> {
    let Value::Object(m) = v else {
        return Err(format!("an extra boundary is an object, got {v}"));
    };
    only_keys(m, &["mode", "from", "picks"], "an extra boundary")?;
    let mode = match word(m, "mode", &["include", "exclude"])? {
        Some("exclude") => BoundaryMode::Exclude,
        Some(_) => BoundaryMode::Include,
        None => return Err("an extra boundary needs \"mode\": include or exclude".into()),
    };
    let shape = match word(m, "from", &["picks", "stock", "silhouette"])?.unwrap_or("picks") {
        "stock" | "silhouette" if m.get("picks").is_some_and(|p| !p.is_null()) => {
            return Err("\"picks\" belongs to a boundary from picks only".into());
        }
        "stock" => BoundaryShape::Stock,
        "silhouette" => BoundaryShape::Silhouette,
        _ => BoundaryShape::Picks(picks_of(env, m.get("picks"), "picks")?),
    };
    Ok(ExtraBoundary { mode, shape })
}

/// The whole controls from the tool's object; omitted keys take their
/// defaults, so `{}` is the identity (every control off).
fn controls_of(env: &Env, v: &Value) -> Result<AreaControls, String> {
    let Value::Object(m) = v else {
        return Err(format!("\"area\" must be an object, got {v}"));
    };
    only_keys(m, &["containment", "offset", "corner", "extension", "extra", "open"], "\"area\"")?;
    let containment = word(m, "containment", &["inside", "on", "outside"])?.map(|c| match c {
        "inside" => Containment::Inside,
        "on" => Containment::On,
        _ => Containment::Outside,
    });
    let corner = match word(m, "corner", &["round", "sharp"])? {
        Some("sharp") => CornerStyle::Sharp,
        _ => CornerStyle::Round,
    };
    let extra = match m.get("extra") {
        None | Some(Value::Null) => Vec::new(),
        Some(Value::Array(items)) => items.iter().map(|b| extra_of(env, b)).collect::<Result<_, _>>()?,
        Some(other) => return Err(format!("\"extra\" must be an array of boundaries, got {other}")),
    };
    let open = match m.get("open") {
        None | Some(Value::Null) => None,
        Some(Value::Object(o)) => {
            only_keys(o, &["picks", "side"], "\"open\"")?;
            let side = match word(o, "side", &["inside", "outside"])? {
                Some("outside") => ChainSide::Outside,
                Some(_) => ChainSide::Inside,
                None => return Err("\"open\" needs \"side\": inside or outside".into()),
            };
            Some(OpenBoundary { picks: picks_of(env, o.get("picks"), "open.picks")?, side })
        }
        Some(other) => return Err(format!("\"open\" must be an object, got {other}")),
    };
    Ok(AreaControls {
        containment,
        offset: number(m, "offset")?,
        corner,
        extension: number(m, "extension")?,
        extra,
        open,
    })
}

/// `set_operation_area`: replace the op's area controls, its parameters
/// as they stand - one undo step, refused by the command's own door
/// (a kind without an area, bad distances, an open chain spelled as a
/// closed boundary) with its text.
pub(super) fn set_operation_area(env: &mut Env, a: &Map<String, Value>) -> Result<Value, String> {
    let key = find_op(env, req_str(a, "operation")?)?;
    let area = controls_of(env, a.get("area").ok_or("\"area\" is required")?)?;
    let e = model::find_by_name(&env.world, &key).ok_or_else(|| format!("no operation named '{key}'"))?;
    let op = model::operation_of(&env.world, &e).ok_or_else(|| format!("'{key}' is not an operation"))?;
    let cmd = EditOperationParams::with_area(&env.world, &key, op.params, area).map_err(|e| e.to_string())?;
    env.history.execute(&env.world, cmd);
    super::pick::geometry_view(env, &key)
}

pub(super) fn s_set_area() -> Value {
    json!({
        "type": "object",
        "properties": {
            "operation": { "type": "string", "description": "Operation name (key or label)." },
            "area": {
                "type": "object",
                "description": "The whole area controls; omitted fields take their defaults ({} turns every control off).",
                "properties": {
                    "containment": { "type": "string", "enum": ["inside", "on", "outside"], "description": "Tool against the boundary; omit for the kind's own rule." },
                    "offset": { "type": "number", "description": "Grow (+) or shrink (-) the boundary, mm." },
                    "corner": { "type": "string", "enum": ["round", "sharp"] },
                    "extension": { "type": "number", "minimum": 0, "description": "Extend past exterior edges, mm; 0 is off." },
                    "extra": { "type": "array", "description": "Extra include / exclude boundaries.", "items": {
                        "type": "object",
                        "properties": {
                            "mode": { "type": "string", "enum": ["include", "exclude"] },
                            "from": { "type": "string", "enum": ["picks", "stock", "silhouette"], "description": "Default picks." },
                            "picks": pick_list_schema("The boundary's features (from: picks).")
                        },
                        "required": ["mode"],
                        "additionalProperties": false
                    } },
                    "open": { "type": "object", "description": "An open (or closed) chain boundary with the side to machine.",
                        "properties": {
                            "picks": pick_list_schema("The boundary's edge chains."),
                            "side": { "type": "string", "enum": ["inside", "outside"] }
                        },
                        "required": ["picks", "side"],
                        "additionalProperties": false }
                },
                "additionalProperties": false
            }
        },
        "required": ["operation", "area"],
        "additionalProperties": false
    })
}
