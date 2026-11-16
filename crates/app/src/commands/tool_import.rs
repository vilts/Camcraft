//! #300 / #899: import a tool library file as ONE undo unit. The library
//! was already checked whole by `tool_library::parse_tool_library`; here
//! each tool meets the document, under an explicit [`ImportMode`] keyed by
//! tool name (#898's part number is the intended future key):
//!
//! - **Add** (default): a name the document already uses is never
//!   overwritten - an identical tool of that name is skipped, a different
//!   one comes in renamed with the first free ` (2)`, ` (3)`... suffix, so
//!   an operation holding the old tool keeps exactly the tool it had.
//! - **Update**: a same-named tool takes the imported definition in place
//!   (same entity, same controller number, so operations keep pointing at
//!   it and read Stale from the stamp); an identical one is skipped; a
//!   name the document lacks is added.
//! - **Replace**: Update, and the library becomes the document's whole tool
//!   set: a document tool the library lacks is removed - unless an
//!   operation uses it, which refuses the import by name, nothing applied.
//!
//! #889: holder bodies ride on top, as a choice of the same command
//! ([`ImportTools::with_options`], `holders`). On, a tool lands with the
//! body its file carries (judged again here against the tool); off, the
//! file's bodies are left out. Per mode:
//!
//! - **Add**: a tool lands with its file body (on) or bare (off). The
//!   skip compares the body only when one lands: record AND body (as the
//!   DTO a save writes) must match, else the tool arrives renamed; a
//!   bare landing matches on the record alone, as before.
//! - **Update / Replace**: a file body that differs from the document
//!   tool's is a change like any other - it takes the `ChangeTool` gates
//!   (`change_refusal`, which fits the body that will stand) and moves
//!   the operation stamp (the body digest), so operations using the tool
//!   go Stale. A tool whose file carries no body, or any tool with
//!   holders off, keeps the body the document has - never cleared
//!   silently - and that body must fit the incoming record.
//!
//! Provenance: our own command model (`AddTool` / `ChangeTool` /
//! `DeleteTool`, batched into one undo unit); no vendor material consulted.

use flecs_ecs::prelude::World;

use crate::commands::tool::reserved;
use crate::commands::{Command, CommandError, DeleteTool};
use crate::model;
use crate::project::ToolBodyDto;
use crate::schema::{ToolBody, ToolDef};
use crate::tool_body;
use crate::tool_library::LibraryTool;
use crate::tools::Tool;

/// What an import does with a library tool whose name the document has.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ImportMode {
    /// Skip an identical tool, rename a different one (the original law).
    #[default]
    Add,
    /// Take the imported definition in place; add the unmatched.
    Update,
    /// Update, then remove document tools the library lacks (refused if used).
    Replace,
}

impl ImportMode {
    /// `add` / `update` / `replace`, case-blind, for scripts and MCP.
    pub fn parse(text: &str) -> Result<Self, String> {
        match text.trim().to_ascii_lowercase().as_str() {
            "add" => Ok(Self::Add),
            "update" => Ok(Self::Update),
            "replace" => Ok(Self::Replace),
            other => Err(format!("unknown import mode '{other}' (use add, update or replace)")),
        }
    }

    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Add => "add",
            Self::Update => "update",
            Self::Replace => "replace",
        }
    }
}

/// What an import did with each library tool, for the status line.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ImportReport {
    /// Tools added under their own name.
    pub added: Vec<String>,
    /// `(library name, name it was added as)` for a name already taken.
    pub renamed: Vec<(String, String)>,
    /// Library names skipped: the document holds that exact tool already.
    pub skipped: Vec<String>,
    /// Update mode: tools that took the imported definition in place.
    pub updated: Vec<String>,
    /// Replace mode: tools that took the imported definition in place.
    pub replaced: Vec<String>,
    /// Replace mode: document tools the library lacked, removed.
    pub removed: Vec<String>,
    /// #889: tools that landed (added or changed) with their file's holder body.
    pub holders: usize,
    /// #889: bodies the file carried that the import left out (holders off).
    pub holders_left_out: usize,
}

fn list(names: &[String]) -> String {
    names.join(", ")
}

impl ImportReport {
    /// One line: "imported 3 tools; 1 renamed (A -> A (2)); 2 already in the document (B, C)".
    #[must_use]
    pub fn summary(&self) -> String {
        let n = self.added.len().saturating_add(self.renamed.len());
        let mut parts: Vec<String> = Vec::new();
        let in_place = self.updated.len().saturating_add(self.replaced.len()).saturating_add(self.removed.len());
        if n > 0 || in_place == 0 {
            parts.push(format!("imported {n} tool{}", if n == 1 { "" } else { "s" }));
        }
        if !self.renamed.is_empty() {
            let l: Vec<String> = self.renamed.iter().map(|(a, b)| format!("{a} -> {b}")).collect();
            parts.push(format!("{} renamed ({})", self.renamed.len(), l.join(", ")));
        }
        for (what, names) in [("updated", &self.updated), ("replaced", &self.replaced), ("removed", &self.removed)] {
            if !names.is_empty() {
                parts.push(format!("{} {what} ({})", names.len(), list(names)));
            }
        }
        if !self.skipped.is_empty() {
            parts.push(format!("{} already in the document ({})", self.skipped.len(), list(&self.skipped)));
        }
        if self.holders > 0 {
            parts.push(format!("{} with {} holder", self.holders, if self.holders == 1 { "its" } else { "their" }));
        }
        if self.holders_left_out > 0 {
            let s = if self.holders_left_out == 1 { "" } else { "s" };
            parts.push(format!("{} holder{s} in the file not imported", self.holders_left_out));
        }
        parts.join("; ")
    }
}

/// One planned change of an import, applied in order, inverted in reverse.
enum Step {
    /// A new tool, with the body it lands with (#889).
    Add(String, Tool, Option<ToolBody>),
    /// Same entity, new definition; the controller number stays. `body`
    /// is `(document body before, file body)` when the file's lands.
    Change { name: String, prev: Tool, next: Tool, body: Option<Box<(Option<ToolBody>, ToolBody)>> },
    Remove(DeleteTool),
}

/// The body as a save writes it - the identity the skip compares (#889).
fn body_key(b: Option<&ToolBody>) -> Option<ToolBodyDto> {
    b.map(ToolBodyDto::from_body)
}

/// Import a library in one step; undo restores every changed tool exactly.
pub struct ImportTools {
    steps: Vec<Step>,
    report: ImportReport,
}

/// Why `name` cannot take `next` in place (the `ChangeTool` gates), by
/// name. `landing` is the file body that lands with it (#889); without
/// one the document's body stays and must fit `next`.
fn change_refusal(world: &World, name: &str, prev: &Tool, next: &Tool, landing: Option<&ToolBody>) -> Option<String> {
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
    if let Some(c) = crate::tool_cutter::cutter_of(world, &entity)
        && let Err(e) = camcraft_tools::cutter_profile::agrees(next, &c)
    {
        return Some(format!("tool '{name}': the drawn cutter profile no longer fits: {e}"));
    }
    None
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

    /// Plan the import against the document. Refuses (nothing applied)
    /// a library naming setup furniture, a tool `is_sane` refuses, a
    /// library that changes nothing, and - Update / Replace - a tool the
    /// edit gates refuse or a name that belongs to a non-tool; Replace
    /// also refuses to remove a tool an operation uses. #889: `holders`
    /// takes each tool's file body (judged against its tool; a body that
    /// does not fit refuses the file by name) or leaves them out.
    pub fn with_options(
        world: &World,
        library: Vec<LibraryTool>,
        mode: ImportMode,
        holders: bool,
    ) -> Result<Self, CommandError> {
        let mut steps: Vec<Step> = Vec::with_capacity(library.len());
        let mut added: Vec<(String, Tool)> = Vec::new();
        let mut report = ImportReport::default();
        let names: Vec<String> = library.iter().map(|l| l.name.clone()).collect();
        for LibraryTool { name, tool, body } in library {
            if let Some(err) = reserved(world, &name) {
                return Err(err);
            }
            if !tool.is_sane() {
                return Err(CommandError::Invalid(format!(
                    "tool '{name}': parameters must be finite and positive"
                )));
            }
            if steps.iter().any(|s| matches!(s, Step::Add(n, ..) if *n == name)) || report_has(&report, &name) {
                return Err(CommandError::Invalid(format!("tool '{name}' appears twice in the library")));
            }
            let body = match body {
                Some(b) if holders => {
                    // Judged again at the command, as SetToolBody does.
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
                // Counted now; a skipped tool takes it back below.
                report.holders = report.holders.saturating_add(1);
            }
            let taken = |n: &str| model::find_by_name(world, n).is_some() || added.iter().any(|(t, _)| t == n);
            let existing = model::find_by_name(world, &name);
            let current = existing.as_ref().and_then(|e| model::tool_of(world, e));
            let doc_body = existing.as_ref().and_then(|e| tool_body::body_of(world, e));
            // A landing body is part of identity; a bare one leaves the
            // document's body alone, so only the record is compared.
            let body_same = body.is_none() || body_key(doc_body.as_ref()) == body_key(body.as_ref());
            match (existing, current) {
                (None, _) => {
                    report.added.push(name.clone());
                    added.push((name.clone(), tool.clone()));
                    steps.push(Step::Add(name, tool, body));
                }
                (Some(_), Some(prev)) if prev == tool && body_same => {
                    if body.is_some() {
                        report.holders = report.holders.saturating_sub(1);
                    }
                    report.skipped.push(name);
                }
                (Some(_), Some(prev)) if mode != ImportMode::Add => {
                    if let Some(why) = change_refusal(world, &name, &prev, &tool, body.as_ref()) {
                        return Err(CommandError::Invalid(why));
                    }
                    match mode {
                        ImportMode::Replace => report.replaced.push(name.clone()),
                        _ => report.updated.push(name.clone()),
                    }
                    let body = body.map(|b| Box::new((doc_body, b)));
                    steps.push(Step::Change { name, prev, next: tool, body });
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
                    report.renamed.push((name, free.clone()));
                    added.push((free.clone(), tool.clone()));
                    steps.push(Step::Add(free, tool, body));
                }
            }
        }
        if mode == ImportMode::Replace {
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
                steps.push(Step::Remove(DeleteTool::new(world, &name)?));
                report.removed.push(name);
            }
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
    [&report.skipped, &report.updated, &report.replaced].iter().any(|v| v.iter().any(|n| n == name))
        || report.renamed.iter().any(|(from, _)| from == name)
}

fn put(world: &World, name: &str, tool: &Tool) {
    if let Some(entity) = model::find_by_name(world, name) {
        let number = model::tool_number_of(world, &entity);
        entity.entity_view(world).set(ToolDef { tool: tool.clone(), number });
    }
}

fn put_body(world: &World, name: &str, body: Option<&ToolBody>) {
    if let Some(entity) = model::find_by_name(world, name) {
        tool_body::set_body(world, &entity, body);
    }
}

impl Command for ImportTools {
    fn apply(&self, world: &World) {
        for step in &self.steps {
            match step {
                Step::Add(name, tool, body) => {
                    let entity = match model::find_by_name(world, name) {
                        Some(entity) => entity,
                        None => model::spawn(world, name),
                    };
                    entity.entity_view(world).set(ToolDef { tool: tool.clone(), number: None });
                    tool_body::set_body(world, &entity, body.as_ref());
                }
                Step::Change { name, next, body, .. } => {
                    put(world, name, next);
                    if let Some((_, b)) = body.as_deref() {
                        put_body(world, name, Some(b));
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
                Step::Change { name, prev, body, .. } => {
                    put(world, name, prev);
                    if let Some((before, _)) = body.as_deref() {
                        put_body(world, name, before.as_ref());
                    }
                }
                Step::Remove(del) => del.invert(world),
            }
        }
    }
}
