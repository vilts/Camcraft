//! The MCP WCS-from-features tool (#937): place the active setup's work
//! offset from `list_features` picks, each tagged with the role it sets
//! (origin, x, y or z). The frame is the GUI's own law -
//! `pick::align_roles` (#701) - and lands through the same undoable
//! `SetWcsOrigin` the GUI's `CommandKind::SetWcs` builds, rotation taken
//! from the law's roll/pitch/yaw exactly as the GUI turns them into a
//! quaternion. No frame maths lives here; every refusal is the law's or
//! the command's own text, returned as a tool error.
//!
//! Provenance: our own tool surface over this repo's WCS-from-features
//! law; datum alignment as taught in coordinate metrology, no vendor
//! material consulted.

use super::pick::{parse_feature_id, req_model};
use super::setup_tools::{cmd_err, loc_view, obj, point};
use super::tools::{Tool, opt_str, req_str};
use crate::commands::SetWcsOrigin;
use crate::geometry::quat_from_euler;
use crate::model;
use crate::operation::GeoSel;
use crate::pick::{Alignment, Role, align_roles, check_pickable};
use crate::schema::Location;
use crate::script::{Env, fnum};
use crate::setup;

use serde_json::{Map, Value, json};

pub(super) const TOOLS: &[Tool] = &[Tool {
    name: "set_wcs_from_features",
    description: "Place the active setup's work coordinate system on a frame fitted to picked features - the GUI's \"Set WCS from features\". Each pick is a list_features id plus the role it sets: origin, x, y or z. Axes read in Z, X, Y priority: the first axis pick is exact, the second is squared to it, the third axis is derived right-handed (a third axis pick is ignored). A flat face sets its outward normal, a straight edge its direction (sided away from the origin pick), a curved wall (face or patch) its fitted axis; the origin is a vertex, a face's centroid or a wall's centre, else the second axis pick's anchor, projected onto the first pick's plane when that pick is a face. One undo step. Refused by name: fewer than two picks, a role given twice, fewer than two axes, a pick that cannot set its role (a vertex for an axis, an edge for the origin), a curved or zero-length edge, near-parallel axes (under 5 degrees apart), an unfittable wall, a stale feature id, a setup that is not the active one.",
    schema: s_set_wcs_from_features,
    run: set_wcs_from_features,
}];

fn role_of(word: &str) -> Result<Role, String> {
    match word.trim().to_ascii_lowercase().as_str() {
        "origin" => Ok(Role::Origin),
        "x" => Ok(Role::X),
        "y" => Ok(Role::Y),
        "z" => Ok(Role::Z),
        _ => Err(format!("\"role\" must be \"origin\", \"x\", \"y\" or \"z\", got \"{word}\"")),
    }
}

fn role_word(r: Role) -> &'static str {
    match r {
        Role::Origin => "origin",
        Role::X => "x",
        Role::Y => "y",
        Role::Z => "z",
    }
}

/// One `{model, id, role}` pick: the model checked to be a part, the id
/// held to the one pick rule (#692) like every other MCP pick.
fn role_pick(env: &Env, v: &Value) -> Result<(GeoSel, Role), String> {
    let Value::Object(m) = v else {
        return Err(format!("a pick is an object {{\"model\", \"id\", \"role\"}}, got {v}"));
    };
    if let Some(k) = m.keys().find(|k| !["model", "id", "role"].contains(&k.as_str())) {
        return Err(format!("a pick has \"model\", \"id\" and \"role\" only, not \"{k}\""));
    }
    let entity = req_str(m, "model")?;
    req_model(env, entity)?;
    let feature = parse_feature_id(req_str(m, "id")?)?;
    let role = role_of(req_str(m, "role")?)?;
    let pick = GeoSel { entity: entity.to_owned(), feature };
    check_pickable(&env.world, &pick)?;
    Ok((pick, role))
}

/// The WCS placement the GUI's `CommandKind::SetWcs` builds from an
/// `Alignment`: its position, and its roll/pitch/yaw degrees turned into
/// a quaternion by the same `quat_from_euler` call - so an agent's pick
/// set lands on the frame a click on Apply would, to the last digit.
fn wcs_location(a: &Alignment) -> Result<Location, String> {
    if ![a.roll_deg, a.pitch_deg, a.yaw_deg].iter().all(|v| v.is_finite()) {
        return Err("WCS not set: rotation angles must be finite".into());
    }
    Ok(Location {
        pos: point(a.pos),
        rot: quat_from_euler(a.roll_deg.to_radians(), a.pitch_deg.to_radians(), a.yaw_deg.to_radians()),
    })
}

fn set_wcs_from_features(env: &mut Env, a: &Map<String, Value>) -> Result<Value, String> {
    let active = setup::active(&env.world);
    // `SetWcsOrigin` places the ACTIVE setup's WCS; naming another setup
    // is refused, never a silent move of the wrong work zero (the GUI
    // greys Apply on an inactive setup's row the same way).
    if let Some(s) = opt_str(a, "setup")? {
        if !setup::exists(&env.world, s) {
            return Err(format!("no setup named '{s}'"));
        }
        if active.as_deref() != Some(s) {
            return Err(format!(
                "setup '{s}' is not the active setup - activate_setup it first to set its WCS from features"
            ));
        }
    }
    let picks: Vec<(GeoSel, Role)> = match a.get("picks") {
        Some(Value::Array(items)) => items.iter().map(|p| role_pick(env, p)).collect::<Result<_, _>>()?,
        None | Some(Value::Null) => return Err("\"picks\" is required".into()),
        Some(other) => return Err(format!("\"picks\" must be an array of picks, got {other}")),
    };
    let fit = align_roles(&env.world, &picks)?;
    let loc = wcs_location(&fit)?;
    let cmd = SetWcsOrigin::new(&env.world, loc).map_err(cmd_err)?;
    env.history.execute(&env.world, cmd);
    let w = &env.world;
    let placed = setup::wcs_entity(w, active.as_deref()).and_then(|e| model::transform_of(w, &e)).unwrap_or_default();
    let dir = |v: [f64; 3]| json!([fnum(v[0]), fnum(v[1]), fnum(v[2])]);
    let mut out = loc_view(&placed);
    out["setup"] = json!(active);
    out["label"] = json!(setup::wcs_label(w, active.as_deref()));
    out["index"] = json!(crate::setup_offsets::index(w, active.as_deref()));
    out["name"] = json!(setup::wcs_name(w, active.as_deref()));
    out["word"] = json!(setup::wcs_name(w, active.as_deref()));
    out["axes"] = json!({ "x": dir(fit.x), "y": dir(fit.y), "z": dir(fit.z) });
    out["angles_deg"] = json!({ "roll": fnum(fit.roll_deg), "pitch": fnum(fit.pitch_deg), "yaw": fnum(fit.yaw_deg) });
    out["picks"] = picks
        .iter()
        .map(|(g, r)| json!({ "model": g.entity, "id": super::pick::feature_id(g.feature), "role": role_word(*r) }))
        .collect();
    Ok(out)
}

fn s_set_wcs_from_features() -> Value {
    obj(
        json!({
            "picks": {
                "type": "array",
                "description": "Two to four picks, each setting one role: two axes, or an axis and the origin, plus at most one more.",
                "items": {
                    "type": "object",
                    "properties": {
                        "model": { "type": "string", "description": "The model the feature is on." },
                        "id": { "type": "string", "description": "A feature id from list_features: face:0x…, chain:0x…, patch:0x… or vertex:0x…." },
                        "role": { "type": "string", "enum": ["origin", "x", "y", "z"], "description": "What the pick sets." }
                    },
                    "required": ["model", "id", "role"],
                    "additionalProperties": false
                }
            },
            "setup": { "type": "string", "description": "Must name the active setup if given (the WCS placed is always the active setup's)." }
        }),
        &["picks"],
    )
}
