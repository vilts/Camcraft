//! The read-only `get_selection` MCP tool (#1942): returns currently
//! selected entities (`SelectEntity`) and, when called from a live UI
//! session, the active viewport pick mode (`state.geo_pick`) and
//! geometry picks (`state.op_ui.geo`) passed via [`SelectionContext`].
//!
//! Provenance: our own read-only MCP view over `model::selected_names`,
//! `setup::active`, and UI selection snapshots; clean-room, no external
//! sources.

use serde_json::{Map, Value, json};

use super::pick::picks_view;
use crate::model;
use crate::operation::GeoSel;
use crate::script::Env;
use crate::setup;

/// Active viewport pick mode mirrored from `crates/ui/src/state.rs`'s `GeoPick`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PickMode {
    Model,
    Face,
    Edge,
    Vertex,
}

impl PickMode {
    /// Wire name returned by `get_selection`.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Model => "model",
            Self::Face => "face",
            Self::Edge => "edge",
            Self::Vertex => "vertex",
        }
    }
}

/// UI selection snapshot passed into [`super::answer_live_with`] without
/// inverting the `ui -> app` dependency direction.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SelectionContext {
    pub pick_mode: Option<PickMode>,
    pub geometry_picks: Vec<GeoSel>,
    pub active_operation: Option<String>,
}

/// Read-only `get_selection` MCP handler.
pub(super) fn get_selection(env: &mut Env, _a: &Map<String, Value>) -> Result<Value, String> {
    let w = &env.world;
    let selected_entities = model::selected_names(w);
    let selected_op_in_world = selected_entities
        .iter()
        .find(|name| model::find_by_name(w, name).is_some_and(|e| model::is_operation(w, &e)));
    let (pick_mode, geometry_picks, selected_operation) = match &env.selection {
        Some(ctx) => (
            ctx.pick_mode.map(PickMode::as_str),
            picks_view(w, &ctx.geometry_picks),
            ctx.active_operation
                .clone()
                .or_else(|| selected_op_in_world.cloned()),
        ),
        None => (None, Vec::new(), selected_op_in_world.cloned()),
    };
    Ok(json!({
        "selected_entities": selected_entities,
        "active_setup": setup::active(w),
        "selected_operation": selected_operation,
        "pick_mode": pick_mode,
        "geometry_picks": geometry_picks,
    }))
}
