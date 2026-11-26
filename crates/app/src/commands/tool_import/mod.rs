//! #300 / #899 / #2566: import a tool library or controller tool table as
//! ONE undo unit. The file was already checked whole by `tool_library` /
//! `tool_table`; here each tool meets the document under an explicit
//! [`ImportMode`] keyed by tool name.
//!
//! Provenance: our own command model (`AddTool` / `ChangeTool` /
//! `DeleteTool`, batched into one undo unit); no vendor material consulted.
//! #3904: a library tool's drawn cutter lands with it and undoes with it.

use flecs_ecs::prelude::World;

use crate::commands::tool::reserved;
use crate::commands::{Command, CommandError, DeleteTool};
use crate::model;
use crate::project::ToolBodyDto;
use crate::schema::{ToolBody, ToolDef};
use crate::spun::SpunProfile;
use crate::tool_body;
use crate::tool_cutter;
use crate::tool_library::LibraryTool;
use crate::tools::Tool;

pub use report::{ImportMode, ImportReport};
mod report;

/// One planned change of an import, applied in order, inverted in reverse.
enum Step {
    /// A new tool, its body (#889), its assigned T/H number (#2442,
    /// #2566) and its drawn cutter (#3904).
    Add(String, Tool, Option<ToolBody>, u32, Option<SpunProfile>),
    /// Same entity, new definition and optional number change.
    Change {
        name: String,
        prev: Tool,
        next: Tool,
        prev_number: Option<u32>,
        next_number: Option<u32>,
        body: Option<Box<(Option<ToolBody>, ToolBody)>>,
        /// #3904: (the cutter before, the library's cutter).
        cutter: Option<Box<(Option<SpunProfile>, SpunProfile)>>,
    },
    Remove(DeleteTool),
}

fn body_key(b: Option<&ToolBody>) -> Option<ToolBodyDto> {
    b.map(ToolBodyDto::from_body)
}

/// Import a library in one step; undo restores every changed tool exactly.
pub struct ImportTools {
    steps: Vec<Step>,
    report: ImportReport,
}

fn change_refusal(
    world: &World,
    name: &str,
    prev: &Tool,
    next: &Tool,
    landing: Option<&ToolBody>,
    landing_cutter: Option<&SpunProfile>,
) -> Option<String> {
    if let Some(why) = next.torch_refusal() {
        return Some(format!("tool '{name}': {why}"));
    }
    if let Some(msg) = crate::operation::tool_gate::retype_refusal(world, name, prev.kind, next.kind) {
        return Some(msg);
    }
    let entity = model::find_by_name(world, name)?;
    if let Some(body) = landing.cloned().or_else(|| tool_body::body_of(world, &entity))
        && let Err(e) = tool_body::fits(next, &body)
    {
        return Some(format!("tool '{name}': {e}"));
    }
    if let Some(c) = landing_cutter.cloned().or_else(|| tool_cutter::cutter_of(world, &entity))
        && let Err(e) = camcraft_tools::cutter_profile::agrees(next, &c)
    {
        return Some(format!("tool '{name}': the drawn cutter profile no longer fits: {e}"));
    }
    None
}

fn validate_library_numbers(library: &[LibraryTool]) -> Result<(), CommandError> {
    let mut seen: Vec<(&str, u32)> = Vec::new();
    for entry in library {
        let Some(n) = entry.number else { continue };
        if !crate::tool_numbers::valid(n) {
            return Err(CommandError::Invalid(format!(
                "tool '{}': tool number {n} is out of range - tool numbers run {}",
                entry.name,
                crate::tool_numbers::RANGE_TEXT
            )));
        }
        if let Some((other, _)) = seen.iter().find(|(_, num)| *num == n) {
            return Err(CommandError::Invalid(format!(
                "tools '{other}' and '{}' both claim T{n}",
                entry.name
            )));
        }
        seen.push((entry.name.as_str(), n));
    }
    Ok(())
}

fn collect_replace_removals(
    world: &World,
    names: &[String],
) -> Result<(Vec<String>, Vec<DeleteTool>), CommandError> {
    let mut removed = Vec::new();
    let mut dels = Vec::new();
    for (name, entity) in model::named_entities(world) {
        if model::tool_of(world, &entity).is_none() || names.contains(&name) {
            continue;
        }
        let users: Vec<String> = model::named_entities(world)
            .into_iter()
            .filter(|(_, e)| model::operation_of(world, e).is_some_and(|op| op.tool == name))
            .map(|(n, _)| n)
            .collect();
        if let Some(first) = users.first() {
            let more = users.len().saturating_sub(1);
            let more = if more == 0 { String::new() } else { format!(" (and {more} more)") };
            return Err(CommandError::Invalid(format!(
                "replace would remove tool '{name}', but operation {first}{more} uses it - keep it in the library or give the operation another tool; nothing imported"
            )));
        }
        dels.push(DeleteTool::new(world, &name)?);
        removed.push(name);
    }
    Ok((removed, dels))
}

fn claim_number(
    world: &World,
    kind: crate::tools::ToolKind,
    owners: &mut Vec<(String, u32)>,
    name: &str,
    explicit: Option<u32>,
) -> Result<u32, CommandError> {
    if let Some(n) = explicit {
        crate::tool_reservations::for_tool(world, name, kind, n).map_err(CommandError::Invalid)?;
        if let Some((other, _)) = owners.iter().find(|(o, num)| *num == n && o != name) {
            return Err(CommandError::Invalid(format!(
                "tool '{name}': tool number T{n} is already '{other}''s"
            )));
        }
        if !owners.iter().any(|(o, _)| o == name) {
            owners.push((name.to_owned(), n));
        }
        return Ok(n);
    }
    let taken: Vec<u32> = owners.iter().map(|(_, n)| *n).collect();
    let n = crate::tool_reservations::allocate(world, name, kind, &taken).map_err(CommandError::Invalid)?;
    owners.push((name.to_owned(), n));
    Ok(n)
}

impl ImportTools {
    /// The original import: [`ImportMode::Add`], holders taken.
    pub fn new(world: &World, library: Vec<LibraryTool>) -> Result<Self, CommandError> {
        Self::with_options(world, library, ImportMode::Add, true)
    }

    /// `mode`, with the file's holders taken (#899's door).
    pub fn with_mode(world: &World, library: Vec<LibraryTool>, mode: ImportMode) -> Result<Self, CommandError> {
        Self::with_options(world, library, mode, true)
    }

    /// [`ImportMode::Add`] with the holders choice (#889's door).
    pub fn with_holders(world: &World, library: Vec<LibraryTool>, holders: bool) -> Result<Self, CommandError> {
        Self::with_options(world, library, ImportMode::Add, holders)
    }

    /// Plan the import against the document.
    pub fn with_options(
        world: &World,
        library: Vec<LibraryTool>,
        mode: ImportMode,
        holders: bool,
    ) -> Result<Self, CommandError> {
        validate_library_numbers(&library)?;
        let names: Vec<String> = library.iter().map(|l| l.name.clone()).collect();
        let (removed_names, remove_steps) = if mode == ImportMode::Replace {
            collect_replace_removals(world, &names)?
        } else {
            (Vec::new(), Vec::new())
        };
        // Track surviving tool-number owners; entries being renumbered in
        // Update/Replace release their old slot first so swaps succeed.
        let reassigned: Vec<&str> = if mode != ImportMode::Add {
            library
                .iter()
                .filter(|l| l.number.is_some())
                .map(|l| l.name.as_str())
                .collect()
        } else {
            Vec::new()
        };
        let mut number_owners: Vec<(String, u32)> = crate::tool_numbers::numbers(world)
            .map_err(CommandError::Invalid)?
            .into_iter()
            .filter(|(n, _)| !removed_names.contains(n) && !reassigned.contains(&n.as_str()))
            .collect();

        let mut steps: Vec<Step> = Vec::with_capacity(library.len() + remove_steps.len());
        let mut added: Vec<(String, Tool)> = Vec::new();
        let mut report = ImportReport::default();

        for LibraryTool { name, tool, body, number: explicit_num, cutter } in library {
            if let Some(err) = reserved(world, &name) {
                return Err(err);
            }
            if let Some(why) = tool.why_not_sane() {
                return Err(CommandError::Invalid(format!("tool '{name}': {why}")));
            }
            if let Some(c) = &cutter
                && let Err(e) = camcraft_tools::cutter_profile::agrees(&tool, c)
            {
                return Err(CommandError::Invalid(format!("tool '{name}': the drawn cutter profile does not fit: {e}")));
            }
            if steps.iter().any(|s| matches!(s, Step::Add(n, ..) if *n == name)) || report_has(&report, &name) {
                return Err(CommandError::Invalid(format!("tool '{name}' appears twice in the library")));
            }
            let body = match body {
                Some(b) if holders => {
                    tool_body::fits(&tool, &b).map_err(|e| CommandError::Invalid(format!("tool '{name}': {e}")))?;
                    Some(b)
                }
                Some(_) => {
                    report.holders_left_out = report.holders_left_out.saturating_add(1);
                    None
                }
                None => None,
            };
            if body.is_some() {
                report.holders = report.holders.saturating_add(1);
            }
            let taken = |n: &str| model::find_by_name(world, n).is_some() || added.iter().any(|(t, _)| t == n);
            let existing = model::find_by_name(world, &name);
            let current = existing.as_ref().and_then(|e| model::tool_of(world, e));
            let prev_number = existing.as_ref().and_then(|e| model::tool_number_of(world, e));
            if let Some(n) = explicit_num.or(prev_number) {
                crate::tool_reservations::for_tool(world, &name, tool.kind, n).map_err(CommandError::Invalid)?;
            }
            let doc_body = existing.as_ref().and_then(|e| tool_body::body_of(world, e));
            let body_same = body.is_none() || body_key(doc_body.as_ref()) == body_key(body.as_ref());
            let num_same = explicit_num.is_none() || explicit_num == prev_number;
            let doc_cutter = existing.as_ref().and_then(|e| tool_cutter::cutter_of(world, e));
            let cutter_same = cutter.is_none() || doc_cutter == cutter;
            match (existing, current) {
                (None, _) => {
                    let assigned = claim_number(world, tool.kind, &mut number_owners, &name, explicit_num)?;
                    report.added.push(name.clone());
                    added.push((name.clone(), tool.clone()));
                    steps.push(Step::Add(name, tool, body, assigned, cutter));
                }
                (Some(_), Some(prev)) if prev == tool && body_same && num_same && cutter_same => {
                    if body.is_some() {
                        report.holders = report.holders.saturating_sub(1);
                    }
                    report.skipped.push(name);
                }
                (Some(_), Some(prev)) if mode != ImportMode::Add => {
                    if let Some(why) = change_refusal(world, &name, &prev, &tool, body.as_ref(), cutter.as_ref()) {
                        return Err(CommandError::Invalid(why));
                    }
                    let next_number = match explicit_num {
                        Some(n) => Some(claim_number(world, tool.kind, &mut number_owners, &name, Some(n))?),
                        None => prev_number,
                    };
                    match mode {
                        ImportMode::Replace => report.replaced.push(name.clone()),
                        _ => report.updated.push(name.clone()),
                    }
                    let body = body.map(|b| Box::new((doc_body, b)));
                    let cutter = cutter.map(|c| Box::new((doc_cutter, c)));
                    steps.push(Step::Change { name, prev, next: tool, prev_number, next_number, body, cutter });
                }
                (Some(_), None) if mode != ImportMode::Add => {
                    return Err(CommandError::Invalid(format!(
                        "'{name}' in the library names something in the document that is not a tool; nothing imported"
                    )));
                }
                _ => {
                    let Some(free) = (2..=9999u32).map(|i| format!("{name} ({i})")).find(|c| !taken(c)) else {
                        return Err(CommandError::NameConflict(name));
                    };
                    let assigned = claim_number(world, tool.kind, &mut number_owners, &free, explicit_num)?;
                    report.renamed.push((name, free.clone()));
                    added.push((free.clone(), tool.clone()));
                    steps.push(Step::Add(free, tool, body, assigned, cutter));
                }
            }
        }
        for (name, del) in removed_names.into_iter().zip(remove_steps) {
            steps.push(Step::Remove(del));
            report.removed.push(name);
        }
        if steps.is_empty() {
            return Err(CommandError::Invalid(
                "nothing to import: every tool in the library is already in the document".into(),
            ));
        }
        Ok(ImportTools { steps, report })
    }

    #[must_use]
    pub fn report(&self) -> &ImportReport {
        &self.report
    }
}

fn report_has(report: &ImportReport, name: &str) -> bool {
    [&report.skipped, &report.updated, &report.replaced]
        .iter()
        .any(|v| v.iter().any(|n| n == name))
        || report.renamed.iter().any(|(from, _)| from == name)
}

fn put(world: &World, name: &str, tool: &Tool, number: Option<u32>) {
    if let Some(entity) = model::find_by_name(world, name) {
        let number = number.or_else(|| model::tool_number_of(world, &entity));
        entity.entity_view(world).set(ToolDef { tool: tool.clone(), number });
    }
}

fn put_body(world: &World, name: &str, body: Option<&ToolBody>) {
    if let Some(entity) = model::find_by_name(world, name) {
        tool_body::set_body(world, &entity, body);
    }
}

fn put_cutter(world: &World, name: &str, cutter: Option<&SpunProfile>) {
    if let Some(entity) = model::find_by_name(world, name) {
        tool_cutter::set_cutter(world, &entity, cutter);
    }
}

impl Command for ImportTools {
    fn apply(&self, world: &World) {
        for step in &self.steps {
            match step {
                Step::Add(name, tool, body, number, cutter) => {
                    let entity = match model::find_by_name(world, name) {
                        Some(entity) => entity,
                        None => model::spawn(world, name),
                    };
                    entity
                        .entity_view(world)
                        .set(ToolDef { tool: tool.clone(), number: Some(*number) });
                    tool_body::set_body(world, &entity, body.as_ref());
                    tool_cutter::set_cutter(world, &entity, cutter.as_ref());
                }
                Step::Change { name, next, next_number, body, cutter, .. } => {
                    put(world, name, next, *next_number);
                    if let Some((_, b)) = body.as_deref() {
                        put_body(world, name, Some(b));
                    }
                    if let Some((_, c)) = cutter.as_deref() {
                        put_cutter(world, name, Some(c));
                    }
                }
                Step::Remove(del) => del.apply(world),
            }
        }
    }

    fn invert(&self, world: &World) {
        for step in self.steps.iter().rev() {
            match step {
                Step::Add(name, ..) => {
                    if let Some(entity) = model::find_by_name(world, name) {
                        model::remove(world, &entity);
                    }
                }
                Step::Change { name, prev, prev_number, body, cutter, .. } => {
                    put(world, name, prev, *prev_number);
                    if let Some((before, _)) = body.as_deref() {
                        put_body(world, name, before.as_ref());
                    }
                    if let Some((before, _)) = cutter.as_deref() {
                        put_cutter(world, name, before.as_ref());
                    }
                }
                Step::Remove(del) => del.invert(world),
            }
        }
    }
}
