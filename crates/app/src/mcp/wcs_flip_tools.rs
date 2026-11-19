//! The MCP WCS flip tool (#1720): set which axes of a setup's one WCS run
//! reversed, through the same undoable `SetWcsFlip` the WCS page's Flip
//! X / Flip Y / Flip Z buttons queue. An axis not given keeps its flag.
//! The post mirrors the program on a flipped X or Y (#1649); a case it
//! does not carry is refused by name (`setup_wcs_flip`), and this tool
//! states that refusal in its reply.
//!
//! Provenance: our own tool surface over this repo's WCS model; no
//! vendor material consulted.

use super::setup_tools::{cmd_err, obj};
use super::tools::{Tool, opt_str};
use crate::commands::SetWcsFlip;
use crate::script::Env;
use crate::setup;
use crate::setup_wcs_flip;

use serde_json::{Map, Value, json};

pub(super) const TOOLS: &[Tool] = &[Tool {
    name: "set_wcs_flip",
    description: "Set which axes of a setup's work coordinate system run reversed (a flip: X+ becomes X-), as the WCS page's Flip X, Flip Y and Flip Z buttons do. Give x, y and/or z as true (flipped) or false (normal); an axis not given keeps its flag. The origin and rotation do not change. One undo step. Toolpaths do not change. The post mirrors the program on a flipped X or Y: coordinate signs, arc direction and cutter compensation side. The reply's refused field is null when the flip is carried, else it holds why the post refuses it: a flipped Z, a WCS that is also rotated, or a post not verified for a mirrored program (Fanuc and Haas are). Refused by name: no flag given, a flag that is not a boolean, an unknown setup.",
    schema: s_set_wcs_flip,
    run: set_wcs_flip,
}];

fn flag(a: &Map<String, Value>, key: &str) -> Result<Option<bool>, String> {
    match a.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::Bool(b)) => Ok(Some(*b)),
        Some(other) => Err(format!("\"{key}\" must be true or false, got {other}")),
    }
}

/// The flags as JSON: `{x, y, z}`.
pub(super) fn flip_view(flip: [bool; 3]) -> Value {
    json!({ "x": flip[0], "y": flip[1], "z": flip[2] })
}

fn set_wcs_flip(env: &mut Env, a: &Map<String, Value>) -> Result<Value, String> {
    let named = opt_str(a, "setup")?;
    if let Some(s) = named
        && !setup::exists(&env.world, s)
    {
        return Err(format!("no setup named '{s}'"));
    }
    let given = [flag(a, "x")?, flag(a, "y")?, flag(a, "z")?];
    if given.iter().all(Option::is_none) {
        return Err("give at least one of \"x\", \"y\" or \"z\"".into());
    }
    let target = match named {
        Some(s) => Some(s.to_owned()),
        None => setup::active(&env.world),
    };
    let now = setup_wcs_flip::of_setup(&env.world, target.as_deref());
    let next = [0, 1, 2].map(|i| given[i].unwrap_or(now[i]));
    let cmd = SetWcsFlip::for_setup(&env.world, target.as_deref(), next).map_err(cmd_err)?;
    env.history.execute(&env.world, cmd);
    let w = &env.world;
    let flip = setup_wcs_flip::of_setup(w, target.as_deref());
    Ok(json!({
        "setup": target,
        "label": setup::wcs_label(w, target.as_deref()),
        "flip": flip_view(flip),
        // #1649: the setup's own refusal, else each operation the post
        // refuses in it (probing, rotary, turning ...); null when it posts.
        "refused": setup_wcs_flip::setup_problem(w, target.as_deref()).or_else(|| {
            let ops = setup_wcs_flip::setup_post_problems(w, target.as_deref());
            (!ops.is_empty()).then(|| ops.join("; "))
        }),
    }))
}

fn s_set_wcs_flip() -> Value {
    obj(
        json!({
            "x": { "type": "boolean", "description": "true: the X axis runs reversed; false: normal." },
            "y": { "type": "boolean", "description": "true: the Y axis runs reversed; false: normal." },
            "z": { "type": "boolean", "description": "true: the Z axis runs reversed; false: normal." },
            "setup": { "type": "string", "description": "The setup whose WCS to flip (default: the active setup)." }
        }),
        &[],
    )
}
