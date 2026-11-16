//! The per-material cutting-data seam (#890): which row of a tool's
//! table the document's workpiece material picks, and the seeds that
//! carry it into the numbers the program runs - the tool's running
//! `rpm` / `feed` (the post reads `Tool::rpm`) and a new operation's
//! milling feed.
//!
//! Provenance: our own seam over `camcraft_tools::cutting_rows`.
//!
//! Safety law: nothing here runs on load. A row moves posted numbers
//! only inside a command the user (or an agent) issued - `AddTool`,
//! `ChangeTool` when the table itself is edited, `SetWorkMaterial`,
//! `EditOperationLinks` retargeting a tool, and the quick-add of a new
//! operation - each one undo step. Where no row names the material, the
//! numbers stay what they are (today's behaviour).

use flecs_ecs::prelude::World;

use crate::cutting;
use crate::operation::OpParams;
use crate::tools::{Tool, WorkMaterial};
use camcraft_tools::cutting_rows::{self as rows, CuttingRow};

/// The row of `tool`'s table for `material`, if it states one.
#[must_use]
pub fn row_in(tool: &Tool, material: Option<WorkMaterial>) -> Option<CuttingRow> {
    rows::row_for(&tool.cutting, material).copied()
}

/// The row for the document's stated material, if the tool has one.
#[must_use]
pub fn row_of(world: &World, tool: &Tool) -> Option<CuttingRow> {
    row_in(tool, cutting::material_of(world))
}

/// `tool` with its running rpm / feed taken from the row for
/// `material`, or `None` when no row names it or the numbers already
/// match (no no-op edits).
#[must_use]
pub fn seeded_for(tool: &Tool, material: Option<WorkMaterial>) -> Option<Tool> {
    let r = row_in(tool, material)?;
    (tool.rpm != r.rpm || tool.feed != r.feed).then(|| Tool { rpm: r.rpm, feed: r.feed, ..tool.clone() })
}

/// A new operation's milling feed from its tool's row for the
/// document's material. Called where an op is created from a seed with
/// a known tool (the ribbon's quick-add, MCP `add_operation`, the
/// feature recognizer), beside `seed_helix_for_tool`; explicit params,
/// saved files and retypes keep their own feed. Kinds with no milling
/// feed (drilling, turning, plasma) are left alone.
pub fn seed_feed_for_tool(world: &World, params: &mut OpParams, tool: &Tool) {
    if let (Some(r), Some(feed)) = (row_of(world, tool), cutting::feed_mut(params)) {
        *feed = r.feed;
    }
}

/// `params` with the milling feed of `tool`'s row for `material`, or
/// `None` when no row names it, the kind has no milling feed, or the
/// feed already matches.
#[must_use]
pub fn reseeded_params(params: &OpParams, tool: &Tool, material: Option<WorkMaterial>) -> Option<OpParams> {
    let r = row_in(tool, material)?;
    let mut next = params.clone();
    let feed = cutting::feed_mut(&mut next)?;
    if *feed == r.feed {
        return None;
    }
    *feed = r.feed;
    Some(next)
}
