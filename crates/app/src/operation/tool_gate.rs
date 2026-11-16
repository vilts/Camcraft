//! The tool law's gate half (M10.1 B): the world-reading questions the
//! commands and the ribbon ask of the `tool_kinds` table - does the
//! active setup hold a tool that can cut this kind, does this named tool
//! fit it, and would a tool retype strand an operation that uses it.
//!
//! Provenance: our own command-layer design (validate early, AGENTS.md
//! Prime Directive), read against the table in `tool_law`; no vendor
//! material consulted.
//!
//! Every function here is a pure read returning `None` when the door is
//! open and the refusal text when it is shut, so `CreateOperation`,
//! `EditOperationLinks`, `ChangeOperationKind`, `ChangeTool` and the
//! ribbon's darkening all speak one sentence for one fact. Tools are a
//! document-wide library; "the active setup's tools" is that library as
//! seen from the setup new work joins, and the refusal names the setup.
//! Opening or viewing an existing operation is never refused - viewing
//! is not cutting. What an existing op's tool has become since it was
//! made is M10.1 C's after-state: [`tool_problem`] names it, generation
//! and the post refuse on it, the tree and the page show it.

use flecs_ecs::prelude::{Entity, World};

use crate::model;
use crate::schema::Operation;
use crate::tools::{Tool, ToolKind};

use super::KindInfo;

/// The kinds as one phrase: "drill", "flat end mill or bull-nose",
/// "drill, tap or boring bar".
#[must_use]
pub fn kinds_phrase(kinds: &[ToolKind]) -> String {
    let words: Vec<String> = kinds.iter().map(ToString::to_string).collect();
    match words.split_last() {
        None => String::from("tool"),
        Some((last, [])) => last.clone(),
        Some((last, rest)) => format!("{} or {last}", rest.join(", ")),
    }
}

/// The setup's name as a refusal speaks it: the active setup, or - in a
/// document that declares none (the implicit-setup law) - "the setup",
/// the name the browser's single Setup row wears.
fn setup_words(world: &World) -> String {
    match crate::setup::active(world) {
        Some(name) => format!("{name}'s"),
        None => String::from("the setup's"),
    }
}

/// The tool library, name-sorted, with each tool's kind.
fn library(world: &World) -> Vec<(String, ToolKind)> {
    model::named_entities(world)
        .into_iter()
        .filter_map(|(name, e)| model::tool_of(world, &e).map(|t| (name, t.kind)))
        .collect()
}

/// The tools (by name, sorted) the kind's family accepts.
#[must_use]
pub fn fitting_tools(world: &World, info: &KindInfo) -> Vec<String> {
    library(world)
        .into_iter()
        .filter(|(_, k)| info.accepts(*k))
        .map(|(n, _)| n)
        .collect()
}

/// Why a kind cannot be created right now: no tool in the active
/// setup's list belongs to its family. `None` when one does.
#[must_use]
pub fn create_refusal(world: &World, info: &KindInfo) -> Option<String> {
    // #532: a kind that mounts no library tool needs none to exist.
    if !info.mounts_tool() || library(world).iter().any(|(_, k)| info.accepts(*k)) {
        return None;
    }
    let hint = catalog_suggestion(&crate::tools::catalog(), info.tool_kinds)
        .map(|s| format!(" - {s}"))
        .unwrap_or_default();
    Some(format!(
        "No {} in {} tools - a {} operation cuts with one; add it to the tool library{hint}",
        kinds_phrase(info.tool_kinds),
        setup_words(world),
        info.label
    ))
}

/// The everyday size a suggestion aims at when the refusal knows no
/// hole or region size (M13.4): 6 mm, the middle of the starter
/// catalog's drill and end-mill ladders - a common shop size, not the
/// smallest (a 2 mm drill is the breakable one, a poor default).
const SUGGEST_SIZE_MM: f64 = 6.0;

/// The size a tool is bought by: a blade's or insert's ground width
/// (every catalog blade shares one 16 mm toolholder body, so the body
/// says nothing), everything else its cutting diameter.
fn bought_size(t: &Tool) -> f64 {
    match t.kind {
        ToolKind::Groove | ToolKind::Thread | ToolKind::Form if t.width > 0.0 => t.width,
        _ => t.diameter,
    }
}

/// M13.4, the tool law's catalog hint: the starter-catalog entry the
/// refusal suggests - of the family's first kind the catalog stocks
/// (the family lists its primary kind first), the one nearest the
/// everyday size, a tie to the smaller. `None` when the catalog holds
/// no tool of any fitting kind: then the refusal says nothing extra
/// rather than suggest a tool that cannot cut the op. Non-finite sizes
/// never win (total ordering on the distance, NaN last).
#[must_use]
pub fn catalog_suggestion(catalog: &[(&str, Tool)], kinds: &[ToolKind]) -> Option<String> {
    let (name, tool) = kinds.iter().find_map(|kind| {
        catalog
            .iter()
            .filter(|(_, t)| t.kind == *kind && bought_size(t).is_finite())
            .min_by(|(_, a), (_, b)| {
                let (sa, sb) = (bought_size(a), bought_size(b));
                (sa - SUGGEST_SIZE_MM)
                    .abs()
                    .total_cmp(&(sb - SUGGEST_SIZE_MM).abs())
                    .then(sa.total_cmp(&sb))
            })
    })?;
    Some(format!(
        "add '{name}' ({} mm {}) from the catalog?",
        bought_size(tool),
        tool.kind
    ))
}

/// Why the named tool cannot cut this kind. `None` when it can, and
/// also when the name is not a tool - existence is `check_tool`'s door,
/// not this one. When the whole setup holds nothing that fits, the
/// setup-level sentence answers instead (it names the real fix).
#[must_use]
pub fn tool_refusal(world: &World, info: &KindInfo, tool: &str) -> Option<String> {
    let kind = model::find_by_name(world, tool).and_then(|e| model::tool_of(world, &e))?.kind;
    if info.accepts(kind) {
        return None;
    }
    if let Some(none_fit) = create_refusal(world, info) {
        return Some(none_fit);
    }
    Some(format!(
        "'{tool}' is a {kind}; a {} operation cuts with a {}",
        info.label,
        kinds_phrase(info.tool_kinds)
    ))
}

/// Why a tool's definition cannot change kind from `prev` to `next`:
/// an operation that names it would be left holding a tool its family
/// cannot cut with. A kind-keeping edit (diameter, lengths) is never
/// refused here - a mismatch that predates this law is C's after-state,
/// not a lock on the library.
#[must_use]
pub fn retype_refusal(world: &World, tool: &str, prev: ToolKind, next: ToolKind) -> Option<String> {
    if prev == next {
        return None;
    }
    let stranded: Vec<(String, &'static KindInfo)> = model::named_entities(world)
        .into_iter()
        .filter_map(|(name, e)| model::operation_of(world, &e).map(|op| (name, op)))
        .filter(|(_, op)| op.tool == tool)
        .map(|(name, op)| (name, op.params.kind().info()))
        .filter(|(_, info)| !info.accepts(next))
        .collect();
    let (first, info) = stranded.first()?;
    let more = match stranded.len().saturating_sub(1) {
        0 => String::new(),
        1 => String::from(" (and 1 more operation)"),
        n => format!(" (and {n} more operations)"),
    };
    Some(format!(
        "'{tool}' cannot become a {next}: {first}{more} uses it, and a {} operation cuts with a {} - give it another tool first",
        info.label,
        kinds_phrase(info.tool_kinds)
    ))
}

/// M10.1 C, the honest after-state: why an existing operation cannot be
/// cut with the tool it names - the tool was deleted (or its name now
/// belongs to something that is not a tool), or it is a kind the op's
/// family cannot cut with (an old file, written before the law). `None`
/// when the tool fits. A pure read of the live world, so the warning
/// surfaces the moment the op is read and clears the moment the world
/// is mended (an undo of the delete, a reassignment, a retype back) -
/// nothing is stored, nothing in the project file changes.
#[must_use]
pub fn tool_problem(world: &World, op_name: &str, op: &Operation) -> Option<String> {
    let info = op.params.kind().info();
    let wants = format!(
        "a {} operation cuts with a {}",
        info.label,
        kinds_phrase(info.tool_kinds)
    );
    let tool = &op.tool;
    // #532: a wire EDM kind names no tool - and must not.
    if !info.mounts_tool() {
        return (!tool.is_empty()).then(|| {
            format!("{op_name} names tool '{tool}'; a {} operation mounts none - the wire is the machine's EDM condition", info.label)
        });
    }
    let Some(entity) = model::find_by_name(world, tool) else {
        return Some(format!("{op_name}'s tool '{tool}' is gone; {wants}"));
    };
    let Some(def) = model::tool_of(world, &entity) else {
        return Some(format!("{op_name}'s tool '{tool}' is no longer a tool; {wants}"));
    };
    if info.accepts(def.kind) {
        return None;
    }
    Some(format!("{op_name}'s tool '{tool}' is a {}; {wants}", def.kind))
}

/// [`tool_problem`] for an operation entity, as the tree, the page and
/// the post read it. `None` for an entity that is not an operation.
#[must_use]
pub fn tool_problem_of(world: &World, entity: &Entity) -> Option<String> {
    let op = model::operation_of(world, entity)?;
    let name = model::name_of(world, entity).unwrap_or_default();
    tool_problem(world, &name, &op)
}

/// A generation's tool: the tool, its body's envelope, its lathe body,
/// its drawn neck (#852; empty = none).
pub(super) type JobTool = (
    Tool,
    Option<camcraft_tools::holder_profile::HolderProfile>,
    Option<std::sync::Arc<camcraft_tools::lathe_holder::LatheClearance>>,
    Vec<camcraft_tools::holder_profile::HolderBand>,
);

/// The library tool a generation cuts with, as `collect_job` reads it:
/// the tool, its modelled body's envelope (#368), its lathe body's
/// clearance (#369) and its drawn neck (#852) - or the refusal by name.
/// Kinds that mount no library tool (#532) never come here.
pub(super) fn job_tool(world: &World, op: &Operation) -> Result<JobTool, crate::machining2d::GenError> {
    use crate::machining2d::GenError;
    let Some(tool_entity) = model::find_by_name(world, &op.tool) else {
        return Err(GenError::Invalid(format!("no tool named '{}'", op.tool)));
    };
    let Some(tool) = model::tool_of(world, &tool_entity) else {
        return Err(GenError::Invalid(format!("'{}' is not a tool", op.tool)));
    };
    let neck = crate::tool_cutter::neck_of(world, &tool_entity, &tool);
    let (tool, body) = crate::tool_body::judged(world, &tool_entity, tool)
        .map_err(|e| GenError::Invalid(format!("tool '{}': {e}", op.tool)))?;
    // #369: a lathe body is judged only by the turning family's
    // revolved-stock law; on any other op no law could read it, so the
    // op is refused by name rather than cut with it unjudged.
    let lathe_body = crate::tool_body_lathe::clearance_of(world, &tool_entity);
    if lathe_body.is_some() && op.params.kind().info().family() != super::Family::Turn {
        return Err(GenError::Invalid(format!(
            "tool '{}' carries a lathe-frame body, which only a turning operation judges - clear it or place a spindle body for this op",
            op.tool
        )));
    }
    Ok((tool, body, lathe_body, neck))
}
