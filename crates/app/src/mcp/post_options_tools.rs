//! The MCP post-options tools (#1534): read and set the document's
//! (`SetPostOptions`) or a named setup's own (`SetSetupPostOptions`,
//! #389) post options without implicit active-setup routing (no
//! #1526-style bleed). One undoable History step per mutation; `follow:
//! true` returns a setup to following the document.
//!
//! Provenance: our own tool surface over this repo's `SetPostOptions`
//! and `SetSetupPostOptions` commands.

use flecs_ecs::prelude::World;
use serde_json::{Map, Value, json};

use super::setup_tools::{cmd_err, obj};
use super::tools::{Tool, opt_str};
use crate::commands::{SetPostOptions, SetSetupPostOptions};
use crate::schema::{BetweenSetups, EndUnits, PostOptions, ThreadingOutput};
use crate::script::Env;
use crate::{setup, setup_post_options};

pub(super) const TOOLS: &[Tool] = &[
    Tool {
        name: "get_post_options",
        description: "Read post options (skip_stops, per_op, threading, end_units, smoothing, between_setups, explain). With setup, reads that declared setup's resolved options and whether it follows the document; without setup, reads the document's.",
        schema: s_get_post_options,
        run: get_post_options,
    },
    Tool {
        name: "set_post_options",
        description: "Set post options as one undoable step. Target is explicit: with setup, sets that setup's own options (SetSetupPostOptions) or follow: true returns it to following the document; without setup, sets the document's (SetPostOptions). Omitted option fields keep their current values.",
        schema: s_set_post_options,
        run: set_post_options,
    },
];

fn get_post_options(env: &mut Env, a: &Map<String, Value>) -> Result<Value, String> {
    let target = target_setup(&env.world, a)?;
    Ok(view(&env.world, target.as_deref()))
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
    let (next, any_field) = apply_patch(base, a)?;
    if follow == Some(true) && any_field {
        return Err("cannot pass post option fields together with \"follow\": true".into());
    }
    if follow.is_none() && !any_field {
        return Err("give at least one post option field to change, or \"follow\": true with \"setup\"".into());
    }
    match target.as_deref() {
        None => {
            let cmd = SetPostOptions::new(&env.world, next).map_err(cmd_err)?;
            env.history.execute(&env.world, cmd);
        }
        Some(s) => {
            let value = (follow != Some(true)).then_some(next);
            let cmd = SetSetupPostOptions::new(&env.world, s, value).map_err(cmd_err)?;
            env.history.execute(&env.world, cmd);
        }
    }
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
    let resolved = match setup {
        Some(s) => setup_post_options::of(world, Some(s)),
        None => setup_post_options::document(world),
    };
    json!({
        "setup": setup,
        "follows_document": setup.is_some() && own.is_none(),
        "own": own.as_ref().map(options_json),
        "options": options_json(&resolved),
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
            "explain": { "type": "boolean", "description": "Append an explanation comment to each block when the post supports it." }
        }),
        &[],
    )
}
