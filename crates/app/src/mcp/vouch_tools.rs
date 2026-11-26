//! The MCP vouch tool (#1339): clear the "fidelity unknown" mark of one
//! model through the same undoable `VouchModelFidelity` the model page
//! queues. `get_job` then shows `fidelity_vouched` on the model. Scripts
//! reach it through `cam.call("vouch_model_fidelity", ...)`.
//!
//! Provenance: our own tool surface over this repo's `VouchModelFidelity`;
//! no vendor material consulted.

use super::setup_tools::{cmd_err, obj};
use super::tools::{req_str, Tool};
use crate::commands::VouchModelFidelity;
use crate::script::Env;

use serde_json::{json, Map, Value};

pub(super) const TOOLS: &[Tool] = &[Tool {
    name: "vouch_model_fidelity",
    description: "Vouch for the mesh of a model marked \"fidelity unknown\" (a model of a project saved before v59, whose reduced_fidelity says fidelity_unknown), so that its operations can post again without a re-import. Use it only when a person states that the mesh is exact or fine enough: gouge checks then read the mesh as exact. \"by\" names that person; the history and the model page show it. The mark also leaves the stocks copied from the model. One undo step; undo puts the mark back. Refused by name: an unknown model, a name that is not a model, an empty \"by\", a model that is not marked, a model with a recorded reduced fidelity (import it again with a larger facet_budget), and an incomplete model (walls missing).",
    schema: s_vouch,
    run: vouch,
}];

fn vouch(env: &mut Env, a: &Map<String, Value>) -> Result<Value, String> {
    let name = req_str(a, "model")?;
    let by = req_str(a, "by")?;
    // Every refusal is at construction, so none reaches the history.
    let cmd = VouchModelFidelity::new(&env.world, name, by).map_err(cmd_err)?;
    let out = json!({
        "model": name,
        "by": cmd.by(),
        "stock_copies": cmd.copies(),
        "summary": cmd.label(),
    });
    cmd.run(&mut env.history, &env.world);
    Ok(out)
}

fn s_vouch() -> Value {
    obj(
        json!({
            "model": { "type": "string", "description": "The model marked fidelity unknown." },
            "by": { "type": "string", "description": "Who vouches for the mesh: the name of the person who states that it is exact." }
        }),
        &["model", "by"],
    )
}
