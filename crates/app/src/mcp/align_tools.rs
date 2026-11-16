//! The MCP part-alignment tools: align the part from axis picks - a
//! straight edge, or two points, for +Y and/or +X - through the same
//! undoable `AlignPartToSetup::align_from_axes` the Setups page's "Part
//! axes" rows queue: one axis turns it in the work plane (#557), both
//! define the full 3D frame (#769, #1329); and lay it on a plane through
//! three picked points (#555), through `AlignPartToSetup::lay_on_points`
//! like the "Lay on a plane - pick three points" row; or on one picked flat
//! face (#729), through `AlignPartToSetup::lay_on_plane`. Every refusal is
//! the command's own text, returned as a tool error.
//!
//! Provenance: our own tool surface over this repo's setup alignment
//! command; no vendor material consulted.

use super::pick::{pick_list_schema, pick_of, picks_of};
use super::setup_tools::{cmd_err, loc_view, obj, setup_or_active};
use super::tools::{Tool, opt_str, req_str};
use crate::commands::{AlignPartToSetup, BoxPoint, OriginAt, SetWcsOrigin};
use crate::cl::ArcPlane;
use crate::pick::{AxisPick, InPlaneAxes, InPlaneAxis, PlaneLay, PointsLay, TargetAxis, in_plane_turn_deg, plane_side, points_side};
use crate::script::{Env, fnum};
use crate::setup_align;

use serde_json::{Map, Value, json};

pub(super) const TOOLS: &[Tool] = &[
    Tool {
        name: "align_part_cylinder",
        description: "Align a picked cylinder or bore wall (a list_features patch id) to target x, y or z (default z), then turn angle_deg (default 0) about that axis. flip reverses the wall axis. Uses the GUI's checked alignment command, including its axis landing proof; one undo step. Refuses unknown setups or models, non-cylinder picks, unusable frames and unchanged poses by name.",
        schema: s_align_part_cylinder,
        run: align_part_cylinder,
    },
    Tool {
        name: "place_setup_origin",
        description: "Place a named setup's WCS origin (default active setup) from source model_box, stock_box or vertex. Boxes take point corner (default) or centre; vertex takes one list_features pick. Keeps the WCS rotation and uses the GUI's undoable command. Refuses missing geometry, invalid picks and unchanged origins by name; one undo step.",
        schema: s_place_setup_origin,
        run: place_setup_origin,
    },
    Tool {
        name: "set_part_axes",
        description: "Align the part from picked axes, as the Setups page's \"Part axes\" rows do. Picks are list_features ids: one edge chain, or two points (first to second) - a point is a vertex id, or the chain id of a circular edge (a hole rim, a boss edge) for its centre; flip_y / flip_x reverse a pick. One axis (+Y or +X): the part turns in the setup's work plane (its XY) so the pick, projected into the plane, runs along that axis (turn_deg reports the turn). Both axes: their 3D directions define the full frame (frame \"full_3d\") - primary (default \"y\") lands exactly, the other is made square to it, and +Z lands on the right-hand cross of the picks, so a left-handed pair is honoured literally (the part lands on its head), not refused. The part turns about its bounds' centre, composed onto the setup's current part alignment; the work offset is untouched; one undo step. Refused by name: a face, a curved edge as the one pick, an edge in a point pair that is not circular (straight, or no circle within tolerance), coincident points, a single pick along Z, X and Y picks running (nearly) along each other.",
        schema: s_set_part_axes,
        run: set_part_axes,
    },
    Tool {
        name: "lay_part_on_points",
        description: "Lay the part on a setup's G17 (XY), G18 (ZX) or G19 (YZ) plane through three picked points (list_features vertex ids, or the chain id of a circular edge - a hole rim, a boss edge - for its centre). The plane through them is turned so its outside (the side away from the part's material) faces the plane's axis (+Z for G17), or the opposite way with flip (G17 + flip: the points rest on the table, part above), and point 1 is set onto the plane's level (place, default true). frame (3-2-1) also turns the part so point 1 -> 2 runs along +X (+Y for G19) and moves point 1 to the setup origin. Absolute like the face pick; one undo step; the work offset is untouched. Refused by name: not three points, a face, an edge that is not circular (straight, or no circle within tolerance), points closer than 0.001 mm, (nearly) collinear points (under 1 degree of spread), a pick off the part, no model.",
        schema: s_lay_part_on_points,
        run: lay_part_on_points,
    },
    Tool {
        name: "lay_part_on_face",
        description: "Lay the part on a setup's G17 (XY), G18 (ZX) or G19 (YZ) plane by one picked flat face (a list_features face id, as {model, id}). The face's outward normal is turned to face the plane's axis (+Z for G17), or the opposite way with flip (G17 + flip: the face rests on the table, part above), and the face's centroid is set onto the plane's level (place, default true). Absolute like the face pick on the Setups page; one undo step; the work offset is untouched. Returns the setup, the face's outside, the stored alignment and its description. Refused by name: an edge, a vertex, a curved or degenerate face, more than one face, a pick off the part, no model, and a lay the setup already holds.",
        schema: s_lay_part_on_face,
        run: lay_part_on_face,
    },
];

fn align_part_cylinder(env: &mut Env, a: &Map<String, Value>) -> Result<Value, String> {
    let setup = setup_or_active(env, a)?.ok_or("no setup is declared - add_setup first")?;
    let pick = pick_of(env, a.get("pick").ok_or("\"pick\" is required")?)?;
    let target = match opt_str(a, "target")? {
        Some("x") => TargetAxis::X,
        Some("y") => TargetAxis::Y,
        None | Some("z") => TargetAxis::Z,
        Some(other) => return Err(format!("\"target\" must be x, y or z, got \"{other}\"")),
    };
    let angle = match a.get("angle_deg") {
        None => 0.0,
        Some(v) => v.as_f64().filter(|n| n.is_finite()).ok_or("\"angle_deg\" must be a finite number")?,
    };
    let cmd = AlignPartToSetup::align_from_cylinder(&env.world, &setup, &pick, target, angle, flag(a, "flip")?)
        .map_err(cmd_err)?;
    env.history.execute(&env.world, cmd);
    let held = setup_align::stored(&env.world, &setup);
    Ok(json!({ "setup": setup, "alignment": held.as_ref().map(loc_view), "description": setup_align::describe(held) }))
}

fn place_setup_origin(env: &mut Env, a: &Map<String, Value>) -> Result<Value, String> {
    let setup = setup_or_active(env, a)?.ok_or("no setup is declared - add_setup first")?;
    let at = match req_str(a, "source")? {
        "vertex" => {
            if a.contains_key("point") {
                return Err("\"point\" belongs to a box source, not vertex".into());
            }
            OriginAt::Vertex(pick_of(env, a.get("pick").ok_or("\"pick\" is required for vertex")?)?)
        }
        source @ ("model_box" | "stock_box") => {
            if a.contains_key("pick") {
                return Err("\"pick\" belongs to vertex, not a box source".into());
            }
            let point = match opt_str(a, "point")? {
                None | Some("corner") => BoxPoint::Corner,
                Some("centre") => BoxPoint::Centre,
                Some(other) => return Err(format!("\"point\" must be corner or centre, got \"{other}\"")),
            };
            if source == "model_box" { OriginAt::ModelBox(point) } else { OriginAt::StockBox(point) }
        }
        other => return Err(format!("\"source\" must be model_box, stock_box or vertex, got \"{other}\"")),
    };
    let location = crate::commands::place_setup_origin(&env.world, &setup, &at).map_err(cmd_err)?;
    let previous = crate::setup::wcs_entity(&env.world, Some(&setup))
        .and_then(|e| crate::model::transform_of(&env.world, &e));
    if previous == Some(location) {
        return Err(format!("setup '{setup}' already holds that origin"));
    }
    let cmd = SetWcsOrigin::for_setup(&env.world, Some(&setup), location).map_err(cmd_err)?;
    env.history.execute(&env.world, cmd);
    let mut out = loc_view(&location);
    out["setup"] = json!(setup);
    Ok(out)
}

fn s_align_part_cylinder() -> Value {
    obj(json!({
        "setup": { "type": "string", "description": "Declared setup; defaults to the active one." },
        "pick": pick_list_schema("One cylinder wall.")["items"].clone(),
        "target": { "type": "string", "enum": ["x", "y", "z"], "default": "z" },
        "angle_deg": { "type": "number", "default": 0 },
        "flip": { "type": "boolean", "default": false }
    }), &["pick"])
}

fn s_place_setup_origin() -> Value {
    obj(json!({
        "setup": { "type": "string", "description": "Declared setup; defaults to the active one." },
        "source": { "type": "string", "enum": ["model_box", "stock_box", "vertex"] },
        "point": { "type": "string", "enum": ["corner", "centre"], "default": "corner" },
        "pick": pick_list_schema("One vertex.")["items"].clone()
    }), &["source"])
}

fn flag(a: &Map<String, Value>, key: &str) -> Result<bool, String> {
    match a.get(key) {
        None | Some(Value::Null) => Ok(false),
        Some(Value::Bool(b)) => Ok(*b),
        Some(other) => Err(format!("\"{key}\" must be true or false, got {other}")),
    }
}

fn axis_of(env: &Env, a: &Map<String, Value>, key: &str, flip: &str) -> Result<Option<AxisPick>, String> {
    match a.get(key) {
        None | Some(Value::Null) => Ok(None),
        v => Ok(Some(AxisPick { picks: picks_of(env, v, key)?, flip: flag(a, flip)? })),
    }
}

fn set_part_axes(env: &mut Env, a: &Map<String, Value>) -> Result<Value, String> {
    let setup = setup_or_active(env, a)?.ok_or("no setup is declared - add_setup first")?;
    let primary = match opt_str(a, "primary")? {
        None | Some("y") => InPlaneAxis::Y,
        Some("x") => InPlaneAxis::X,
        Some(other) => return Err(format!("\"primary\" must be \"y\" or \"x\", got \"{other}\"")),
    };
    let axes = InPlaneAxes { y: axis_of(env, a, "y", "flip_y")?, x: axis_of(env, a, "x", "flip_x")?, primary };
    // Both axes: the full 3D frame (#769) - no in-plane turn angle, and
    // a left-handed pair is honoured, as the GUI does. One axis: the
    // in-plane turn (#557), whose angle is reported.
    let full_3d = axes.y.as_ref().is_some_and(|p| !p.picks.is_empty())
        && axes.x.as_ref().is_some_and(|p| !p.picks.is_empty());
    let turn = if full_3d { None } else { Some(in_plane_turn_deg(&env.world, &setup, &axes)?) };
    let cmd = AlignPartToSetup::align_from_axes(&env.world, &setup, &axes).map_err(cmd_err)?;
    env.history.execute(&env.world, cmd);
    let held = setup_align::stored(&env.world, &setup);
    Ok(json!({
        "setup": setup,
        "frame": if full_3d { "full_3d" } else { "in_plane" },
        "turn_deg": turn.map(fnum),
        "alignment": held.as_ref().map(loc_view),
        "description": setup_align::describe(held),
    }))
}

fn s_set_part_axes() -> Value {
    obj(
        json!({
            "setup": { "type": "string", "description": "A declared setup; defaults to the active one." },
            "y": pick_list_schema("+Y: one straight edge chain, or two points (first to second): vertex ids or circular edge chain ids (their centres)."),
            "x": pick_list_schema("+X: one straight edge chain, or two points (first to second): vertex ids or circular edge chain ids (their centres)."),
            "flip_y": { "type": "boolean", "description": "Take the +Y pick the other way." },
            "flip_x": { "type": "boolean", "description": "Take the +X pick the other way." },
            "primary": { "type": "string", "enum": ["y", "x"], "description": "With both axes given, the one landed exactly; the other is made square to it (default y)." }
        }),
        &[],
    )
}

/// The `plane` option shared by the lay tools: g17 (default), g18, g19.
fn plane_of(a: &Map<String, Value>) -> Result<ArcPlane, String> {
    match opt_str(a, "plane")? {
        None | Some("g17") => Ok(ArcPlane::Xy),
        Some("g18") => Ok(ArcPlane::Xz),
        Some("g19") => Ok(ArcPlane::Yz),
        Some(other) => Err(format!("\"plane\" must be \"g17\", \"g18\" or \"g19\", got \"{other}\"")),
    }
}

fn lay_part_on_face(env: &mut Env, a: &Map<String, Value>) -> Result<Value, String> {
    let setup = setup_or_active(env, a)?.ok_or("no setup is declared - add_setup first")?;
    let plane = plane_of(a)?;
    let place = match a.get("place") {
        None | Some(Value::Null) => true,
        _ => flag(a, "place")?,
    };
    // One pick object; an array is passed on so the command refuses more than one by name.
    let face = match a.get("face") {
        Some(v @ Value::Object(_)) => Value::Array(vec![v.clone()]),
        Some(v) => v.clone(),
        None => Value::Null,
    };
    let picks = picks_of(env, Some(&face), "face")?;
    let lay = PlaneLay { plane, flip: flag(a, "flip")?, place };
    let side = plane_side(&env.world, &picks)?;
    let cmd = AlignPartToSetup::lay_on_plane(&env.world, &setup, &picks, lay).map_err(cmd_err)?;
    env.history.execute(&env.world, cmd);
    let held = setup_align::stored(&env.world, &setup);
    Ok(json!({
        "setup": setup,
        "outside": side,
        "alignment": held.as_ref().map(loc_view),
        "description": setup_align::describe(held),
    }))
}

fn s_lay_part_on_face() -> Value {
    obj(
        json!({
            "setup": { "type": "string", "description": "A declared setup; defaults to the active one." },
            "face": {
                "type": "object",
                "description": "One flat face of the part: {\"model\", \"id\"} with a list_features face id.",
                "properties": { "model": { "type": "string" }, "id": { "type": "string" } },
                "required": ["model", "id"]
            },
            "plane": { "type": "string", "enum": ["g17", "g18", "g19"], "description": "The plane to lay on (default g17)." },
            "flip": { "type": "boolean", "description": "Face the outside the opposite way (-Z for G17: rest the face on the table)." },
            "place": { "type": "boolean", "description": "Set the face onto the plane's level (default true); false keeps the part where it is." }
        }),
        &["face"],
    )
}

fn lay_part_on_points(env: &mut Env, a: &Map<String, Value>) -> Result<Value, String> {
    let setup = setup_or_active(env, a)?.ok_or("no setup is declared - add_setup first")?;
    let plane = match opt_str(a, "plane")? {
        None | Some("g17") => ArcPlane::Xy,
        Some("g18") => ArcPlane::Xz,
        Some("g19") => ArcPlane::Yz,
        Some(other) => return Err(format!("\"plane\" must be \"g17\", \"g18\" or \"g19\", got \"{other}\"")),
    };
    let place = match a.get("place") {
        None | Some(Value::Null) => true,
        _ => flag(a, "place")?,
    };
    let picks = picks_of(env, a.get("points"), "points")?;
    let pl = PointsLay { lay: PlaneLay { plane, flip: flag(a, "flip")?, place }, frame: flag(a, "frame")? };
    let side = points_side(&env.world, &picks)?;
    let cmd = AlignPartToSetup::lay_on_points(&env.world, &setup, &picks, pl).map_err(cmd_err)?;
    env.history.execute(&env.world, cmd);
    let held = setup_align::stored(&env.world, &setup);
    Ok(json!({
        "setup": setup,
        "outside": side,
        "alignment": held.as_ref().map(loc_view),
        "description": setup_align::describe(held),
    }))
}

fn s_lay_part_on_points() -> Value {
    obj(
        json!({
            "setup": { "type": "string", "description": "A declared setup; defaults to the active one." },
            "points": pick_list_schema("Exactly three points (points 1, 2, 3): vertex ids, or circular edge chain ids for their centres."),
            "plane": { "type": "string", "enum": ["g17", "g18", "g19"], "description": "The plane to lay on (default g17)." },
            "flip": { "type": "boolean", "description": "Face the points' outside the opposite way (-Z for G17: rest the points on the table)." },
            "place": { "type": "boolean", "description": "Set point 1 onto the plane's level (default true); false keeps the part where it is." },
            "frame": { "type": "boolean", "description": "3-2-1: point 1 to the origin, point 1 -> 2 along +X (+Y for G19)." }
        }),
        &["points"],
    )
}
