//! Cutting-data commands (M13.2): `SetWorkMaterial` states the
//! document's workpiece material (and, #890, seeds the tools' rows for
//! it); `ApplyCuttingData` lands an
//! operation's feeds-and-speeds suggestion as ONE undo unit - the
//! feed through `EditOperationParams`, the spindle speed through
//! `ChangeTool` (the tool's rpm IS the program's spindle word). A
//! turning op (#891) also takes its plunge feed, and its tool the
//! surface speed and feed per rev the suggestion read.
//!
//! Provenance: our own commands, the theme's snapshot-and-restore law
//! for the singleton and a composite of our own two edit commands for
//! the suggestion; specced from ROADMAP M13.2. Never silent: the
//! suggestion is computed and validated at construction, and a
//! suggestion equal to what the op already runs is refused (no no-ops
//! in the history), so a click is the only way numbers move.

use flecs_ecs::prelude::World;

use crate::commands::{ChangeTool, Command, CommandError, EditOperationParams};
use crate::cutting::{self, Suggestion};
use crate::cutting_rows;
use crate::model;
use crate::tools::{TurningData, WorkMaterial};

/// State the document's workpiece material - and (#890) seed it: every
/// tool whose per-material table names the new material runs that row
/// (`ChangeTool`), and every operation on such a tool takes the row's
/// milling feed (`EditOperationParams`), all in this one undo step.
/// Tools with no row for it, and their operations, keep their numbers.
pub struct SetWorkMaterial {
    prev: Option<WorkMaterial>,
    next: Option<WorkMaterial>,
    tools: Vec<ChangeTool>,
    ops: Vec<EditOperationParams>,
}

impl SetWorkMaterial {
    pub fn new(world: &World, next: Option<WorkMaterial>) -> Result<Self, CommandError> {
        let prev = cutting::material_of(world);
        if prev == next {
            return Err(CommandError::Invalid("that workpiece material is already stated".to_owned()));
        }
        let mut tools = Vec::new();
        let mut ops = Vec::new();
        let named = model::named_entities(world);
        for (name, e) in &named {
            if let Some(t) = model::tool_of(world, e)
                && let Some(seeded) = cutting_rows::seeded_for(&t, next)
            {
                tools.push(ChangeTool::new(world, name, seeded)?);
            }
        }
        for (name, e) in &named {
            let Some(op) = model::operation_of(world, e) else { continue };
            let Some(tool) = model::find_by_name(world, &op.tool).and_then(|t| model::tool_of(world, &t)) else {
                continue;
            };
            if let Some(params) = cutting_rows::reseeded_params(&op.params, &tool, next) {
                ops.push(EditOperationParams::new(world, name, params)?);
            }
        }
        Ok(Self { prev, next, tools, ops })
    }

    /// How many tools and operations the material's rows re-seeded.
    #[must_use]
    pub fn seeded(&self) -> (usize, usize) {
        (self.tools.len(), self.ops.len())
    }
}

impl Command for SetWorkMaterial {
    fn apply(&self, world: &World) {
        cutting::material_set(world, self.next);
        for t in &self.tools {
            t.apply(world);
        }
        for o in &self.ops {
            o.apply(world);
        }
    }

    fn invert(&self, world: &World) {
        for o in self.ops.iter().rev() {
            o.invert(world);
        }
        for t in self.tools.iter().rev() {
            t.invert(world);
        }
        cutting::material_set(world, self.prev);
    }
}

/// The suggestion, applied: either half may be absent when that number
/// already matches, never both.
pub struct ApplyCuttingData {
    edit: Option<EditOperationParams>,
    tool: Option<ChangeTool>,
    suggestion: Suggestion,
}

impl ApplyCuttingData {
    pub fn new(world: &World, name: &str) -> Result<Self, CommandError> {
        let s = cutting::suggest(world, name).map_err(CommandError::Invalid)?;
        let op = model::find_by_name(world, name)
            .and_then(|e| model::operation_of(world, &e))
            .ok_or_else(|| CommandError::NotFound(name.to_owned()))?;
        let tool = model::find_by_name(world, &op.tool)
            .and_then(|t| model::tool_of(world, &t))
            .ok_or_else(|| CommandError::NotFound(op.tool.clone()))?;
        let mut params = op.params.clone();
        let mut next = crate::tools::Tool { rpm: s.rpm, ..tool.clone() };
        let edit = if let Some(plan) = s.turning {
            // #891: a turning op takes the feed AND the plunge at the
            // suggested rpm, and the tool keeps the Vc and feed per rev
            // it now runs (its own max-rpm clamp stays as stated).
            let (feed, plunge) = crate::cutting_turn::turn_feeds_mut(&mut params).ok_or_else(|| {
                CommandError::Invalid(format!("a {} operation has no turning feed to suggest", op.params.kind()))
            })?;
            *feed = s.feed;
            *plunge = plan.plunge_feed;
            next.turning =
                Some(TurningData { vc: s.vc, feed_per_rev: s.fz, max_rpm: plan.max_rpm });
            (params != op.params).then_some(params)
        } else {
            let feed = cutting::feed_mut(&mut params).ok_or_else(|| {
                CommandError::Invalid(format!("a {} operation has no milling feed to suggest", op.params.kind()))
            })?;
            let moved = *feed != s.feed;
            *feed = s.feed;
            moved.then_some(params)
        };
        let edit = edit.map(|p| EditOperationParams::new(world, name, p)).transpose()?;
        let change = if next == tool { None } else { Some(ChangeTool::new(world, &op.tool, next)?) };
        if edit.is_none() && change.is_none() {
            let u = crate::units::of(world);
            return Err(CommandError::Invalid(format!(
                "{name} already runs the suggestion ({:.0} rpm, {} {})",
                s.rpm,
                crate::units::feed(s.feed, u),
                u.feed_suffix()
            )));
        }
        Ok(Self { edit, tool: change, suggestion: s })
    }

    /// What was applied, for the status line.
    #[must_use]
    pub fn suggestion(&self) -> &Suggestion {
        &self.suggestion
    }
}

impl Command for ApplyCuttingData {
    fn apply(&self, world: &World) {
        if let Some(e) = &self.edit {
            e.apply(world);
        }
        if let Some(t) = &self.tool {
            t.apply(world);
        }
    }

    fn invert(&self, world: &World) {
        if let Some(t) = &self.tool {
            t.invert(world);
        }
        if let Some(e) = &self.edit {
            e.invert(world);
        }
    }
}
