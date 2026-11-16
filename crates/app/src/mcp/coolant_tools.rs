//! `set_operation_coolant` (#954): an operation's coolant mode written
//! through `EditOperationParams::with_coolant`, the undoable command the
//! operation page's coolant picker runs - one undo step, the same door.
//!
//! Provenance: our own JSON surface over `operation::CoolantMode`
//! (#112, #869); no vendor material consulted.

use super::edit::find_op;
use super::tools::{req_str, Tool};
use crate::commands::EditOperationParams;
use crate::model;
use crate::operation::CoolantMode;
use crate::script::Env;

use serde_json::{json, Map, Value};

pub(super) const TOOLS: &[Tool] = &[Tool {
    name: "set_operation_coolant",
    description: "Set an operation's coolant: off, flood, mist, air or through-tool. One undo step, the same command the operation page's coolant picker runs. A mode the operation's machine has no coded entry for is still set, with a \"warning\" (the post refuses it at post time).",
    schema: s_set_coolant,
    run: set_operation_coolant,
}];

/// The wire word of a mode (the project file's kebab-case).
fn word(m: CoolantMode) -> &'static str {
    match m {
        CoolantMode::Off => "off",
        CoolantMode::Flood => "flood",
        CoolantMode::Mist => "mist",
        CoolantMode::Air => "air",
        CoolantMode::ThroughTool => "through-tool",
    }
}

/// Read a mode word: case-blind, `_` and spaces as `-`, so
/// `through_tool` and "Through tool" both serve.
fn parse(s: &str) -> Result<CoolantMode, String> {
    let n = s.trim().to_ascii_lowercase().replace(['_', ' '], "-");
    CoolantMode::ALL.into_iter().find(|m| word(*m) == n).ok_or_else(|| {
        let all: Vec<&str> = CoolantMode::ALL.iter().map(|m| word(*m)).collect();
        format!("unknown coolant '{s}' - one of {}", all.join(", "))
    })
}

/// A warning when the op's machine states coded coolants and none of
/// them serves `mode` (a shop's own coded name serves flood, as the
/// posts read it). Off, and a machine stating no coded coolant, never warn.
fn machine_warning(env: &Env, setup: Option<&str>, mode: CoolantMode) -> Option<String> {
    if mode == CoolantMode::Off {
        return None;
    }
    let (_, machine) = crate::setup_mount::machine_of(&env.world, setup);
    let machine = machine?;
    let coded: Vec<Option<CoolantMode>> = machine
        .coolants
        .iter()
        .filter(|c| !c.m_code.trim().is_empty())
        .map(|c| CoolantMode::of_machine_name(&c.name))
        .collect();
    let serves = coded.iter().any(|m| match m {
        Some(m) => *m == mode,
        None => mode == CoolantMode::Flood,
    });
    (!coded.is_empty() && !serves).then(|| {
        format!("the machine has no coolant entry for '{}'; the post will refuse it", word(mode))
    })
}

fn set_operation_coolant(env: &mut Env, a: &Map<String, Value>) -> Result<Value, String> {
    let key = find_op(env, req_str(a, "name")?)?;
    let mode = parse(req_str(a, "coolant")?)?;
    let cmd = EditOperationParams::with_coolant(&env.world, &key, mode).map_err(|e| e.to_string())?;
    env.history.execute(&env.world, cmd);
    let setup = model::find_by_name(&env.world, &key)
        .and_then(|e| model::operation_of(&env.world, &e))
        .and_then(|o| o.setup);
    let mut out = json!({ "operation": key, "coolant": word(mode) });
    if let Some(w) = machine_warning(env, setup.as_deref(), mode) {
        out["warning"] = json!(w);
    }
    Ok(out)
}

fn s_set_coolant() -> Value {
    json!({
        "type": "object",
        "properties": {
            "name": { "type": "string", "description": "Operation name (key or label)." },
            "coolant": { "type": "string", "enum": ["off", "flood", "mist", "air", "through-tool"], "description": "The coolant mode; through_tool is read as through-tool." }
        },
        "required": ["name", "coolant"],
        "additionalProperties": false
    })
}
