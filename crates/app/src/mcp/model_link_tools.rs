//! The MCP model link tool (#2463): link a model to its source file, or
//! unlink it, through the same undoable `SetModelSource` the Browser's
//! Unlink item queues. `get_job` shows each linked model's `source`. The
//! check of a linked file, and the import again when it changes, run in
//! the app's window; a headless session only records the link.
//!
//! Provenance: our own tool surface over this repo's `SetModelSource`; no
//! vendor material consulted.

use std::path::Path;

use super::setup_tools::{cmd_err, obj};
use super::tools::{req_str, Tool};
use crate::commands::SetModelSource;
use crate::model;
use crate::script::Env;

use serde_json::{json, Map, Value};

pub(super) const TOOLS: &[Tool] = &[Tool {
    name: "set_model_link",
    description: "Link a model to a file, or unlink it. A linked model is imported again under the same name when its file changes (a new SHA-256), while the project is open in the app window: one undo step, and its operations go Stale. Nothing is regenerated or posted by itself. Give \"path\" as a file to link to (its digest is recorded now; the file is not imported), or as null to unlink. An import from a file links the model already. One undo step. Refused by name: an unknown model, a name that is not a model, a file that cannot be read, unlinking a model that is not linked.",
    schema: s_set_model_link,
    run: set_model_link,
}];

fn set_model_link(env: &mut Env, a: &Map<String, Value>) -> Result<Value, String> {
    let name = req_str(a, "model")?;
    let cmd = match a.get("path") {
        Some(Value::String(p)) => SetModelSource::link(&env.world, name, Path::new(p)),
        Some(Value::Null) => SetModelSource::unlink(&env.world, name),
        None => return Err("give \"path\": a file to link to, or null to unlink".into()),
        Some(other) => return Err(format!("\"path\" must be a string or null, got {other}")),
    }
    .map_err(cmd_err)?;
    env.history.execute(&env.world, cmd);
    let source = model::find_by_name(&env.world, name).and_then(|e| crate::model_source::of(&env.world, &e));
    Ok(json!({
        "model": name,
        "source": source.map(|s| json!({ "path": s.path, "sha256": s.sha256 })),
    }))
}

fn s_set_model_link() -> Value {
    obj(
        json!({
            "model": { "type": "string", "description": "The model to link or unlink." },
            "path": { "type": ["string", "null"], "description": "The file to link the model to, or null to unlink it." }
        }),
        &["model", "path"],
    )
}
