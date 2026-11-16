//! The MCP circle-centre anchor (#739): place the active setup's WCS on
//! the centre of a picked circular edge (a hole rim, a boss edge) through
//! the same undoable `SetWcsAtCircleCentre` the WCS page's "Circle centre"
//! anchor queues. A straight edge, a curve that is not a circle, a face or
//! a vertex is refused by name, never guessed.
//!
//! Provenance: our own tool surface over this repo's pick primitive and
//! WCS command; work-offset semantics per RS-274 / ISO 6983.

use super::pick::pick_of;
use super::setup_tools::{cmd_err, loc_view, obj, opt_nums};
use super::tools::Tool;
use crate::commands::SetWcsAtCircleCentre;
use crate::geometry::IDENTITY_Q;
use crate::model;
use crate::script::{Env, fnum};
use crate::setup;

use serde_json::{Map, Value, json};

pub(super) const TOOLS: &[Tool] = &[Tool {
    name: "set_wcs_circle_centre",
    description: "Place the active setup's work coordinate system on the centre of a picked circular edge (a hole rim or a boss edge; a full circle or an arc): one list_features edge chain id. The origin goes to the circle's centre at the edge's height; the result also states the circle's axis and diameter. Rotation is a unit quaternion [x, y, z, w]; omitted keeps the current one. One undo step. Refused by name: a straight edge, a curve that is not circular within tolerance, a run of fewer than three segments, a face or a vertex.",
    schema: s_set_wcs_circle_centre,
    run: set_wcs_circle_centre,
}];

fn set_wcs_circle_centre(env: &mut Env, a: &Map<String, Value>) -> Result<Value, String> {
    let edge = pick_of(env, a.get("edge").ok_or("\"edge\" is required")?)?;
    let active = setup::active(&env.world);
    let current =
        setup::wcs_entity(&env.world, active.as_deref()).and_then(|e| model::transform_of(&env.world, &e));
    let rot = match opt_nums::<4>(a, "rotation")? {
        Some(q) => q,
        None => current.map_or(IDENTITY_Q, |l| l.rot),
    };
    let cmd = SetWcsAtCircleCentre::new(&env.world, &edge, rot).map_err(cmd_err)?;
    let circle = cmd.circle().clone();
    env.history.execute(&env.world, cmd);
    let w = &env.world;
    let placed = setup::wcs_entity(w, active.as_deref()).and_then(|e| model::transform_of(w, &e)).unwrap_or_default();
    let mut out = loc_view(&placed);
    out["setup"] = json!(active);
    out["label"] = json!(setup::wcs_label(w, active.as_deref()));
    out["circle"] = json!({
        "centre": circle.centre.iter().map(|v| fnum(*v)).collect::<Vec<_>>(),
        "axis": circle.axis.iter().map(|v| fnum(*v)).collect::<Vec<_>>(),
        "diameter": fnum(circle.diameter()),
        "closed": circle.closed,
    });
    Ok(out)
}

fn s_set_wcs_circle_centre() -> Value {
    obj(
        json!({
            "edge": {
                "type": "object",
                "description": "The circular edge: a list_features edge chain on a model.",
                "properties": {
                    "model": { "type": "string", "description": "The model the edge is on." },
                    "id": { "type": "string", "description": "An edge chain id from list_features (chain:0x…)." }
                },
                "required": ["model", "id"],
                "additionalProperties": false
            },
            "rotation": { "type": "array", "items": { "type": "number" }, "minItems": 4, "maxItems": 4, "description": "Unit quaternion [x, y, z, w]; omitted keeps the WCS's current rotation." }
        }),
        &["edge"],
    )
}
