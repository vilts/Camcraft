//! A recognised pocket or slot planned as a new operation (#511): the
//! name, kind, source and params `CreateOperation` takes - one plan for
//! the MCP `detect_pockets {create}` tool and the GUI's Detect pockets /
//! slots page, so a click and an agent create the same operation.
//!
//! Provenance: our own glue over `recognize::operation_for` and the
//! shared operation seed; no vendor material.

use flecs_ecs::prelude::World;

use super::{op_kind_of, operation_for, Pocket};
use crate::model;
use crate::operation::{self, KindInfo, OpParams, SrcRef};

/// The operation one recognised feature becomes.
#[derive(Clone, Debug, PartialEq)]
pub struct Planned {
    /// The operation's name: the caller's, else the kind's label and the
    /// first free number ("Pocket1", "Slot2").
    pub name: String,
    /// The kind's display label ("Pocket", "Slot").
    pub label: &'static str,
    pub src: SrcRef,
    pub params: OpParams,
}

/// Plan the operation that machines `p` with `tool`: the kind's shared
/// seed, its helix sized to the tool, then the feature's area and depths.
///
/// # Errors
/// A feature with no usable depth or width (`operation_for`).
pub fn plan_operation(world: &World, p: &Pocket, tool: &str, name: Option<&str>) -> Result<Planned, String> {
    let kind = op_kind_of(p) as usize;
    let mut base = operation::seed_params(kind);
    if let Some(t) = model::find_by_name(world, tool).and_then(|e| model::tool_of(world, &e)) {
        operation::seed_helix_for_tool(&mut base, t.diameter);
        crate::cutting_rows::seed_feed_for_tool(world, &mut base, &t);
    }
    let (src, params) = operation_for(p, &base)?;
    let label = KindInfo::ALL.get(kind).map_or("Operation", |k| k.label);
    let name = match name {
        Some(n) => n.to_owned(),
        None => (1..)
            .map(|n| format!("{label}{n}"))
            .find(|c| model::find_by_name(world, c).is_none())
            .unwrap_or_else(|| label.to_owned()),
    };
    Ok(Planned { name, label, src, params })
}
