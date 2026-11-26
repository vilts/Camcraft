//! #3544: Detect holes on an operation whose source is the stock. A new
//! Drill, Bore or Thread operation reads the stock unless a model is
//! picked, and the stock has no holes or bosses. A refusal that said
//! "the source model" pointed the user at the wrong thing, so it names
//! the stock and the models the user can pick instead.
//! Provenance: our own wording; no vendor material consulted.

use flecs_ecs::prelude::World;

use crate::model;
use crate::operation::SrcRef;

/// What Detect holes read, for its refusals: "the stock" or "the source
/// model".
pub(super) fn searched(src: &SrcRef) -> &'static str {
    if matches!(src, SrcRef::Stock) { "the stock" } else { "the source model" }
}

/// The fix that a refusal on a stock source adds, or nothing for any other
/// source: `name` reads the stock, and each model of the document is a
/// source to pick (`source: "<model>"`, the MCP `add_operation` key).
pub(super) fn stock_hint(world: &World, name: &str, src: &SrcRef) -> String {
    if !matches!(src, SrcRef::Stock) {
        return String::new();
    }
    let models: Vec<String> =
        model::model_entities(world).into_iter().map(|(n, _)| format!("source: \"{n}\"")).collect();
    if models.is_empty() {
        format!(". '{name}' reads the stock, not a model: import a model and pick it as the source")
    } else {
        format!(". '{name}' reads the stock, not a model: pick a model as the source ({})", models.join(" or "))
    }
}
