//! The per-material cutting-data seam (#890): which row of a tool's
//! table the workpiece material picks, and the seeds that carry it into
//! the numbers the program runs - the tool's running `rpm` / `feed` (the
//! post reads `Tool::rpm`) and an operation's milling feed, stepdown and
//! stepover.
//!
//! #2015: the row in force is the one naming the material, else the
//! tool's `Unknown` row. For an operation the material is its setup
//! stock's, else the document's ([`crate::stock_material::in_force`]);
//! the tool's running numbers follow the document's material (a tool is
//! one entity, shared by every setup). A row's missing stepdown or
//! stepover falls back to the `Unknown` row's.
//!
//! Provenance: our own seam over `camcraft_tools::cutting_rows`.
//!
//! Safety law: nothing here runs on load. A row moves posted numbers
//! only inside a command the user (or an agent) issued - `AddTool`,
//! `ChangeTool` when the table itself is edited, `SetWorkMaterial`,
//! `SetStockMaterial`, `EditOperationLinks` retargeting a tool, and the
//! quick-add of a new operation - each one undo step. An operation's
//! milling FEED moves only for a row that names the material (#890's
//! rule): picking a material never overwrites a hand-set op feed with
//! the tool's own `Unknown` feed. Where no row says otherwise, the
//! numbers stay what they are. #2598: a Drill or Thread op takes its
//! tool's stated feed at the quick-add and at a retarget, and a feed
//! above it is refused at the post (`operation::feed_rating`).

use flecs_ecs::prelude::World;

use crate::cutting;
use crate::operation::OpParams;
use crate::tools::{Tool, WorkMaterial};
use camcraft_tools::cutting_rows::{self as rows, CuttingRow};

/// The row of `tool`'s table in force for `material`: the row naming
/// it, else the `Unknown` row.
#[must_use]
pub fn row_in(tool: &Tool, material: Option<WorkMaterial>) -> Option<CuttingRow> {
    rows::row_in_tool(tool, material)
}

/// The row in force for the document's stated material.
#[must_use]
pub fn row_of(world: &World, tool: &Tool) -> Option<CuttingRow> {
    row_in(tool, cutting::material_of(world))
}

/// The row that NAMES `material` (no `Unknown` fallback).
fn named_row(tool: &Tool, material: Option<WorkMaterial>) -> Option<CuttingRow> {
    row_in(tool, material).filter(|r| r.material.is_some())
}

/// `tool` with its running rpm / feed taken from the row in force for
/// `material`, or `None` when the tool has no such row or the numbers
/// already match (no no-op edits). Moving them away from a derived
/// `Unknown` row stores that row first, so the default is kept.
#[must_use]
pub fn seeded_for(tool: &Tool, material: Option<WorkMaterial>) -> Option<Tool> {
    let r = row_in(tool, material)?;
    if tool.rpm == r.rpm && tool.feed == r.feed {
        return None;
    }
    let mut next = tool.clone();
    rows::ensure_unknown_row(&mut next);
    next.rpm = r.rpm;
    next.feed = r.feed;
    rows::canonical(&mut next);
    Some(next)
}

/// #2015: an edit that moves the running rpm / feed but leaves the
/// table alone, while the `Unknown` row is the row in force, moves that
/// row with them - so the `Unknown` row stays what the tool runs when
/// no row names the material, and a later material change never puts
/// back numbers the user had replaced.
pub fn follow_running(prev: &Tool, next: &mut Tool, material: Option<WorkMaterial>) {
    // An editor shows the derived row and hands it back untouched: it
    // stays derived, so it follows the new numbers by itself.
    if rows::unknown_row(&prev.cutting).is_none()
        && let Some(i) = next.cutting.iter().position(|r| rows::is_derived_unknown(r, prev.rpm, prev.feed))
    {
        next.cutting.remove(i);
    }
    if next.cutting != prev.cutting || (next.rpm == prev.rpm && next.feed == prev.feed) {
        return;
    }
    if named_row(next, material).is_some() {
        return;
    }
    let (rpm, feed) = (next.rpm, next.feed);
    if let Some(r) = next.cutting.iter_mut().find(|r| r.is_unknown()) {
        r.rpm = rpm;
        r.feed = feed;
    }
}

/// The stepdown and stepover fields an operation kind states in mm, as
/// the cutting-data rows seed them: the pocket family, the slot, the
/// 3-D and multi-axis roughing ladders, the staged face (its stepdown
/// only when staged) and the 2-D contour's stepdown.
pub fn steps_mut(p: &mut OpParams) -> (Option<&mut f64>, Option<&mut f64>) {
    match p {
        OpParams::Pocket { stepdown, stepover, .. }
        | OpParams::Adaptive2D { stepdown, stepover, .. }
        | OpParams::Offset2D { stepdown, stepover, .. }
        | OpParams::Slot { stepdown, stepover, .. }
        | OpParams::Pocket3D { stepdown, stepover, .. }
        | OpParams::MultiAxisRough { stepdown, stepover, .. } => (Some(stepdown), Some(stepover)),
        OpParams::Face { stepdown, stepover, .. } => (stepdown.as_mut(), Some(stepover)),
        OpParams::Contour { stepdown, .. } => (Some(stepdown), None),
        _ => (None, None),
    }
}

/// The stepdown the verifier's plunge depth law reads (#3679): the kinds
/// that cut in vertical levels and state a stepdown in mm, as
/// [`steps_mut`] seeds them. `None` - every kind that cuts no levels
/// (drills, bores, threads, the turning, wire and plasma families) -
/// keeps the law silent.
#[must_use]
pub fn plunge_stepdown(p: &OpParams) -> Option<f64> {
    match p {
        OpParams::Contour { stepdown, .. }
        | OpParams::Pocket { stepdown, .. }
        | OpParams::Offset2D { stepdown, .. }
        | OpParams::Slot { stepdown, .. }
        | OpParams::TrochoidalSlot { stepdown, .. }
        | OpParams::Adaptive2D { stepdown, .. }
        | OpParams::Pocket3D { stepdown, .. }
        | OpParams::MultiAxisRough { stepdown, .. } => Some(*stepdown),
        // Staged facing only: `None` faces in one level at `cut_z`.
        OpParams::Face { stepdown, .. } => *stepdown,
        _ => None,
    }
}

/// Seed `params` from `tool`'s rows for `material`: the feed from a
/// row naming it, the stepdown / stepover from the row in force (each
/// missing one from the `Unknown` row). `true` when anything moved.
fn seed_into(params: &mut OpParams, tool: &Tool, material: Option<WorkMaterial>) -> bool {
    let mut moved = false;
    if let (Some(r), Some(feed)) = (named_row(tool, material), cutting::feed_mut(params))
        && *feed != r.feed
    {
        *feed = r.feed;
        moved = true;
    }
    let (down, over) = rows::steps_in_tool(tool, material);
    let (down_f, over_f) = steps_mut(params);
    for (want, field) in [(down, down_f), (over, over_f)] {
        if let (Some(v), Some(f)) = (want, field)
            && *f != v
        {
            *f = v;
            moved = true;
        }
    }
    moved
}

/// A new operation's milling feed, stepdown and stepover from its
/// tool's rows for the material in force on the active setup (where a
/// new operation lands). Called where an op is created from a seed with
/// a known tool (the ribbon's quick-add, MCP `add_operation`, the
/// feature recognizer), beside `seed_helix_for_tool`; explicit params,
/// saved files and retypes keep their own numbers. A Drill or Thread op
/// takes the tool's stated feed as its own (#2598,
/// [`crate::operation::feed_rating::seed_into`]); other kinds with no
/// milling feed or steps (turning, plasma) are left alone.
pub fn seed_feed_for_tool(world: &World, params: &mut OpParams, tool: &Tool) {
    let material = crate::stock_material::in_force(world, crate::setup::active(world).as_deref());
    seed_into(params, tool, material);
    crate::operation::feed_rating::seed_into(params, tool, material, cutting::material_of(world));
}

/// `params` with `tool` set in its place (`EditOperationLinks`): the
/// rows' seed of [`reseeded_params`], then a Drill or Thread op's feed
/// from the new tool's stated feed (#2598; `document` is the document's
/// material, which the running feed follows, #2724). `None` when nothing
/// moves.
#[must_use]
pub fn retargeted_params(
    params: &OpParams,
    tool: &Tool,
    material: Option<WorkMaterial>,
    document: Option<WorkMaterial>,
) -> Option<OpParams> {
    let mut next = params.clone();
    let rows = seed_into(&mut next, tool, material);
    let rated = crate::operation::feed_rating::seed_into(&mut next, tool, material, document);
    (rows || rated).then_some(next)
}

/// `params` seeded from `tool`'s rows for `material` (see
/// [`seed_feed_for_tool`]), or `None` when nothing would move.
#[must_use]
pub fn reseeded_params(params: &OpParams, tool: &Tool, material: Option<WorkMaterial>) -> Option<OpParams> {
    let mut next = params.clone();
    seed_into(&mut next, tool, material).then_some(next)
}
