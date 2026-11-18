//! The MCP post-options tools (#1534): read and set the document's
//! (`SetPostOptions`) or a named setup's own (`SetSetupPostOptions`,
//! #389) post options without implicit active-setup routing (no
//! #1526-style bleed). One undoable History step per mutation; `follow:
//! true` returns a setup to following the document.
//!
//! Provenance: our own tool surface over this repo's `SetPostOptions`
//! and `SetSetupPostOptions` commands.

use std::collections::BTreeMap;

use flecs_ecs::prelude::World;
use serde_json::{Map, Value, json};

use super::setup_tools::{cmd_err, obj};
use super::tools::{Tool, opt_str};
use crate::commands::{PostSettingsEdit, SetPostSettings};
use crate::post::options::{self, DeclaredOption, OptionValue};
use crate::schema::{BetweenSetups, EndUnits, PostOptions, ThreadingOutput, WcsRotationOutput};
use crate::script::Env;
use crate::{posting, setup, setup_mount, setup_post_options};

pub(super) const TOOLS: &[Tool] = &[
    Tool {
        name: "get_post_options",
        description: "Read post options (skip_stops, per_op, threading, end_units, smoothing, between_setups, wcs_rotation, explain), resolved per-post script_options, and the target post's script_schema. With setup, reads that declared setup's resolved options and whether it follows the document; without setup, reads the document's.",
        schema: s_get_post_options,
        run: get_post_options,
    },
    Tool {
        name: "set_post_options",
        description: "Set post options and/or per-post script_options as one undoable step. Target is explicit: with setup, sets that setup's own options or follow: true returns both built-in and script options to following the document; without setup, sets the document's. Pass null inside script_options to reset one key to the script default. Omitted fields keep their current values.",
        schema: s_set_post_options,
        run: set_post_options,
    },
];

fn get_post_options(env: &mut Env, a: &Map<String, Value>) -> Result<Value, String> {
    let target = target_setup(&env.world, a)?;
    Ok(view(&env.world, target.as_deref()))
}

fn opt_script_patch(
    a: &Map<String, Value>,
    schema: &[DeclaredOption],
) -> Result<Option<BTreeMap<String, Option<OptionValue>>>, String> {
    let Some(raw) = a.get("script_options") else {
        return Ok(None);
    };
    let map = raw
        .as_object()
        .ok_or_else(|| "\"script_options\" must be an object".to_owned())?;
    if map.is_empty() {
        return Err("\"script_options\" must contain at least one option key".to_owned());
    }
    let mut patch = BTreeMap::new();
    for (k, v) in map {
        if !schema.iter().any(|d| &d.key == k) {
            return Err(format!("unknown post option '{k}'"));
        }
        if v.is_null() {
            patch.insert(k.clone(), None);
        } else {
            let parsed: OptionValue = serde_json::from_value(v.clone())
                .map_err(|_| format!("invalid value for post option '{k}'"))?;
            patch.insert(k.clone(), Some(parsed));
        }
    }
    Ok(Some(patch))
}

fn set_post_options(env: &mut Env, a: &Map<String, Value>) -> Result<Value, String> {
    let target = target_setup(&env.world, a)?;
    let follow = opt_bool(a, "follow")?;
    if follow.is_some() && target.is_none() {
        return Err("\"follow\" requires \"setup\" (only a declared setup can follow or override the document)".into());
    }
    let base = match target.as_deref() {
        Some(s) => setup_post_options::of(&env.world, Some(s)),
        None => setup_post_options::document(&env.world),
    };
    let post = match target.as_deref() {
        Some(s) => setup_mount::post_of(&env.world, Some(s)),
        None => posting::document_post_of(&env.world),
    };
    let (next, any_field) = apply_patch(base, a)?;
    let has_script_arg = a.contains_key("script_options");
    if follow == Some(true) && (any_field || has_script_arg) {
        return Err("cannot pass post option fields together with \"follow\": true".into());
    }
    if follow.is_none() && !any_field && !has_script_arg {
        return Err("give at least one post option field to change, or \"follow\": true with \"setup\"".into());
    }
    let script_overrides = if let Some(patch) = match a.get("script_options") {
        Some(_) => {
            let schema = posting::declared_options_for_post(&post)?;
            opt_script_patch(a, &schema)?
        }
        None => None,
    } {
        let mut current = setup_post_options::script_for_post(&env.world, target.as_deref(), &post);
        for (k, val) in patch {
            match val {
                Some(v) => {
                    current.insert(k, v);
                }
                None => {
                    current.remove(&k);
                }
            }
        }
        Some(current)
    } else {
        None
    };

    let follow_doc = follow == Some(true);
    let edit = PostSettingsEdit {
        setup: target.clone(),
        program_info: None,
        post_options: if follow_doc {
            None
        } else if target.is_some() {
            (any_field || follow == Some(false)).then_some(next)
        } else {
            Some(next)
        },
        follow_document: follow_doc,
        script_post: script_overrides.as_ref().map(|_| post),
        script_overrides,
        script_replace: None,
    };
    let cmd = SetPostSettings::new(&env.world, edit).map_err(cmd_err)?;
    if cmd.is_empty() {
        return Err(if follow_doc {
            format!(
                "setup '{}' already follows the document's post options",
                target.as_deref().unwrap_or_default()
            )
        } else {
            "those post options are already set".into()
        });
    }
    env.history.execute(&env.world, cmd);
    Ok(view(&env.world, target.as_deref()))
}

fn target_setup(world: &World, a: &Map<String, Value>) -> Result<Option<String>, String> {
    match opt_str(a, "setup")? {
        None => Ok(None),
        Some("") => Err("\"setup\" must not be empty".into()),
        Some(s) if !setup::exists(world, s) => Err(format!("no setup named '{s}'")),
        Some(s) => Ok(Some(s.to_owned())),
    }
}

fn opt_bool(a: &Map<String, Value>, key: &str) -> Result<Option<bool>, String> {
    match a.get(key) {
        None => Ok(None),
        Some(Value::Bool(b)) => Ok(Some(*b)),
        Some(other) => Err(format!("\"{key}\" must be true or false, got {other}")),
    }
}

fn opt_word<'a>(a: &'a Map<String, Value>, key: &str) -> Result<Option<&'a str>, String> {
    match a.get(key) {
        None => Ok(None),
        Some(Value::String(s)) => Ok(Some(s.as_str())),
        Some(other) => Err(format!("\"{key}\" must be a string, got {other}")),
    }
}

fn apply_patch(mut next: PostOptions, a: &Map<String, Value>) -> Result<(PostOptions, bool), String> {
    let mut any = false;
    if let Some(v) = opt_bool(a, "skip_stops")? {
        next.skip_stops = v;
        any = true;
    }
    if let Some(v) = opt_bool(a, "per_op")? {
        next.per_op = v;
        any = true;
    }
    if let Some(v) = opt_bool(a, "smoothing")? {
        next.smoothing = v;
        any = true;
    }
    if let Some(v) = opt_bool(a, "explain")? {
        next.explain = v;
        any = true;
    }
    if let Some(w) = opt_word(a, "threading")? {
        next.threading = parse_threading(w)?;
        any = true;
    }
    if let Some(w) = opt_word(a, "end_units")? {
        next.end_units = parse_end_units(w)?;
        any = true;
    }
    if let Some(w) = opt_word(a, "between_setups")? {
        next.between_setups = BetweenSetups::from_word(w).ok_or_else(|| {
            let words: Vec<&str> = BetweenSetups::ALL.iter().map(|b| b.word()).collect();
            format!("\"between_setups\" must be one of {}, not {w:?}", words.join(", "))
        })?;
        any = true;
    }
    if let Some(w) = opt_word(a, "wcs_rotation")? {
        next.wcs_rotation = WcsRotationOutput::from_word(w).ok_or_else(|| {
            let words: Vec<&str> = WcsRotationOutput::ALL.iter().map(|v| v.word()).collect();
            format!("\"wcs_rotation\" must be one of {}, not {w:?}", words.join(", "))
        })?;
        any = true;
    }
    Ok((next, any))
}

fn parse_threading(word: &str) -> Result<ThreadingOutput, String> {
    match word {
        "g32" => Ok(ThreadingOutput::G32Passes),
        "g76" => Ok(ThreadingOutput::G76Cycle),
        other => Err(format!("\"threading\" must be \"g32\" or \"g76\", not {other:?}")),
    }
}

fn parse_end_units(word: &str) -> Result<EndUnits, String> {
    match word {
        "leave" => Ok(EndUnits::Leave),
        "inch" => Ok(EndUnits::Inch),
        "mm" => Ok(EndUnits::Millimeters),
        other => Err(format!("\"end_units\" must be \"leave\", \"inch\" or \"mm\", not {other:?}")),
    }
}

fn view(world: &World, setup: Option<&str>) -> Value {
    let own = setup.and_then(|s| setup_post_options::own(world, Some(s)));
    let script_own = setup.and_then(|s| setup_post_options::script_own(world, Some(s)));
    let resolved = match setup {
        Some(s) => setup_post_options::of(world, Some(s)),
        None => setup_post_options::document(world),
    };
    let post = match setup {
        Some(s) => setup_mount::post_of(world, Some(s)),
        None => posting::document_post_of(world),
    };
    let schema = posting::declared_options_for_post(&post).unwrap_or_default();
    let stored = setup_post_options::script_for_post(world, setup, &post);
    let resolved_script = options::resolve_options(&schema, &stored).unwrap_or_default();
    json!({
        "setup": setup,
        "follows_document": setup.is_some() && own.is_none() && script_own.is_none(),
        "own": own.as_ref().map(options_json),
        "options": options_json(&resolved),
        "post": posting::post_ref_label(&post),
        "script_schema": schema,
        "script_options": resolved_script,
    })
}

fn options_json(o: &PostOptions) -> Value {
    let threading = match o.threading {
        ThreadingOutput::G32Passes => "g32",
        ThreadingOutput::G76Cycle => "g76",
    };
    let end_units = match o.end_units {
        EndUnits::Leave => "leave",
        EndUnits::Inch => "inch",
        EndUnits::Millimeters => "mm",
    };
    json!({
        "skip_stops": o.skip_stops,
        "per_op": o.per_op,
        "threading": threading,
        "end_units": end_units,
        "smoothing": o.smoothing,
        "between_setups": o.between_setups.word(),
        "wcs_rotation": o.wcs_rotation.word(),
        "explain": o.explain,
    })
}

fn s_get_post_options() -> Value {
    obj(
        json!({
            "setup": { "type": ["string", "null"], "description": "Declared setup name; omitted or null reads the document's post options." }
        }),
        &[],
    )
}

fn s_set_post_options() -> Value {
    let between: Vec<&str> = BetweenSetups::ALL.iter().map(|b| b.word()).collect();
    let wcs_rot: Vec<&str> = WcsRotationOutput::ALL.iter().map(|v| v.word()).collect();
    obj(
        json!({
            "setup": { "type": ["string", "null"], "description": "Declared setup name to set its own options; omitted or null sets the document's." },
            "follow": { "type": "boolean", "description": "With setup, true returns that setup to following the document's post options." },
            "skip_stops": { "type": "boolean", "description": "Post without any operation's stop or trip home." },
            "per_op": { "type": "boolean", "description": "Write one standalone program file per operation." },
            "threading": { "type": "string", "enum": ["g32", "g76"], "description": "Lathe threading output: g32 passes or g76 cycle." },
            "end_units": { "type": "string", "enum": ["leave", "inch", "mm"], "description": "Length unit word to leave the control in after the program." },
            "smoothing": { "type": "boolean", "description": "Write per-operation smoothing mode words." },
            "between_setups": { "type": "string", "enum": between, "description": "What the post writes where a program moves to the next setup." },
            "wcs_rotation": { "type": "string", "enum": wcs_rot, "description": "How a Z-rotated work offset posts: bake into XY or emit control G68/ROT." },
            "explain": { "type": "boolean", "description": "Append an explanation comment to each block when the post supports it." },
            "script_options": {
                "type": "object",
                "additionalProperties": { "type": ["boolean", "integer", "number", "string", "null"] },
                "description": "Per-post script option overrides keyed by declared option name; pass null to reset a key back to the script default."
            }
        }),
        &[],
    )
}
