//! Tool commands: add, edit and delete tool entities. Validation is all
//! at construction (AGENTS.md: malformed input never enters the history);
//! `apply` is a pure world write so redo replays identically.
//!
//! Provenance: our own command model; identity by name, snapshot-and-
//! restore inverses, like every sibling command.

use flecs_ecs::prelude::World;

use crate::commands::{Command, CommandError};
use crate::model;
use crate::schema::{Location, STOCK_NAME, WCS_SLOTS, ToolBody, ToolDef};
use crate::tools::Tool;

pub(super) fn reserved(world: &World, name: &str) -> Option<CommandError> {
    if name.is_empty() {
        return Some(CommandError::Invalid("tool name must not be empty".into()));
    }
    // Setup furniture names (M3.2 A): the legacy `Stock`/`G54`..`G59`,
    // every declared setup's own names, and the `SetupN/...` convention
    // so a tool cannot squat on the entity a future setup will lazily
    // spawn.
    let taken = name == STOCK_NAME
        || WCS_SLOTS.contains(&name)
        || crate::setup::is_furniture_shape(name)
        || crate::setup::names(world).iter().any(|s| {
                crate::setup::stock_entity_name(world, Some(s)) == name
                    || crate::setup::wcs_entity_name(world, Some(s)) == name
            });
    if taken {
        return Some(CommandError::Invalid(format!(
            "'{name}' is a name reserved for a setup's stock and work offsets"
        )));
    }
    None
}

/// Which holder a new tool starts with (#828).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HolderPick {
    /// The active setup's machine's `tooling.default_holder`, when it
    /// states one and the holder takes the tool; none otherwise.
    MachineDefault,
    /// This built-in library holder, stated by the caller - it wins
    /// over the machine's default, and a refusal refuses the add.
    Named(String),
    /// No holder body (the tool's own holder cylinder), as before #828.
    Bare,
}

/// Create a tool entity with the given definition - and (#828) the
/// holder body it starts with, landed in the same undo step.
pub struct AddTool {
    name: String,
    tool: Tool,
    /// The library holder's name and its drawn body.
    holder: Option<(String, ToolBody)>,
    /// Why the machine's default holder was not applied, when it was not.
    skipped: Option<String>,
}

impl AddTool {
    pub fn new(world: &World, name: &str, tool: Tool) -> Result<Self, CommandError> {
        if let Some(err) = reserved(world, name) {
            return Err(err);
        }
        // #670: a torch stating a spindle speed is refused by name.
        if let Some(why) = tool.torch_refusal() {
            return Err(CommandError::Invalid(format!("tool '{name}': {why}")));
        }
        if !tool.is_sane() {
            return Err(CommandError::Invalid(format!(
                "tool parameters must be finite and positive: {tool:?}"
            )));
        }
        if model::find_by_name(world, name).is_some() {
            return Err(CommandError::NameConflict(name.to_string()));
        }
        Ok(AddTool {
            name: name.to_string(),
            tool,
            holder: None,
            skipped: None,
        })
    }

    /// #828: add the tool with the holder `pick` names. A stated
    /// holder that cannot take the tool refuses the add by name; the
    /// machine's default that cannot (a shank its collet does not take,
    /// a lathe-held insert) is left off and said so ([`Self::note`]) -
    /// the tool lands as it would without the key, never with another
    /// holder in its place.
    pub fn with_holder(world: &World, name: &str, tool: Tool, pick: HolderPick) -> Result<Self, CommandError> {
        let mut cmd = Self::new(world, name, tool)?;
        match pick {
            HolderPick::Bare => {}
            HolderPick::Named(entry) => {
                let body = super::tool_body::library_body(&cmd.tool, name, &entry, None, None)?;
                let canonical = camcraft_tools::holder_library::find(&entry).map(|e| e.name()).unwrap_or(entry);
                cmd.holder = Some((canonical, body));
            }
            HolderPick::MachineDefault => {
                if let Some(entry) = crate::machine::of(world).1.and_then(|m| m.default_holder) {
                    match super::tool_body::library_body(&cmd.tool, name, &entry, None, None) {
                        Ok(mut body) => {
                            // Assumed, not picked: judged floored (#828).
                            body.machine_default = true;
                            cmd.holder = Some((entry, body));
                        }
                        Err(e) => cmd.skipped = Some(format!("the machine's default holder {entry} was not applied - {e}")),
                    }
                }
            }
        }
        Ok(cmd)
    }

    /// The library holder the tool lands with, if any.
    #[must_use]
    pub fn holder(&self) -> Option<&str> {
        self.holder.as_ref().map(|(n, _)| n.as_str())
    }

    /// The add's holder line for the status bar and MCP: the holder it
    /// lands with, or why the machine's default was left off.
    #[must_use]
    pub fn note(&self) -> Option<String> {
        match (&self.holder, &self.skipped) {
            (Some((n, b)), _) if b.machine_default => {
                Some(format!("holder {n} (machine default - judged no smaller than the tool's holder cylinder)"))
            }
            (Some((n, _)), _) => Some(format!("holder {n}")),
            (None, Some(why)) => Some(why.clone()),
            (None, None) => None,
        }
    }
}

impl Command for AddTool {
    fn apply(&self, world: &World) {
        let entity = match model::find_by_name(world, &self.name) {
            Some(entity) => entity,
            None => model::spawn(world, &self.name),
        };
        entity
            .entity_view(world)
            .set(ToolDef { tool: self.tool.clone(), number: None });
        if let Some((_, body)) = &self.holder {
            crate::tool_body::set_body(world, &entity, Some(body));
        }
    }

    fn invert(&self, world: &World) {
        if let Some(entity) = model::find_by_name(world, &self.name) {
            model::remove(world, &entity);
        }
    }
}

/// Replace a tool's definition (the tool library's edit path). Operations
/// that reference the tool read the new numbers and derive Stale from the
/// stamp mismatch - no cascade writes here.
pub struct ChangeTool {
    name: String,
    prev: Tool,
    next: Tool,
}

impl ChangeTool {
    pub fn new(world: &World, name: &str, next: Tool) -> Result<Self, CommandError> {
        Self::judged(world, name, next, true)
    }

    /// Every `new` law; `judge_cutter: false` skips the drawn cutter's
    /// agreement - only for `ReshapeTool` (#93 slice 3), which lands a
    /// cutter checked against `next` in the same undo step.
    pub(crate) fn judged(world: &World, name: &str, next: Tool, judge_cutter: bool) -> Result<Self, CommandError> {
        if let Some(why) = next.torch_refusal() {
            return Err(CommandError::Invalid(format!("tool '{name}': {why}")));
        }
        if !next.is_sane() {
            return Err(CommandError::Invalid(format!(
                "tool parameters must be finite and positive: {next:?}"
            )));
        }
        let Some(entity) = model::find_by_name(world, name) else {
            return Err(CommandError::NotFound(name.to_string()));
        };
        let prev = model::tool_of(world, &entity)
            .ok_or_else(|| CommandError::Invalid(format!("'{name}' is not a tool")))?;
        // The tool law (M10.1 B): a retype that would leave an operation
        // holding a tool its family cannot cut with is refused by name.
        if let Some(msg) =
            crate::operation::tool_gate::retype_refusal(world, name, prev.kind, next.kind)
        {
            return Err(CommandError::Invalid(msg));
        }
        // #368: an edit that would stand the tool's body over its flutes
        // (or retype it to a lathe insert) is refused by name.
        if let Some(body) = crate::tool_body::body_of(world, &entity) {
            crate::tool_body::fits(&next, &body)
                .map_err(|e| CommandError::Invalid(format!("tool '{name}': {e}")))?;
        }
        // #93 slice 2: a drawn cutter states the numbers it was drawn
        // to; an edit that leaves it disagreeing is refused by name
        // (clear it back to the numbers' outline first).
        if let Some(c) = crate::tool_cutter::cutter_of(world, &entity).filter(|_| judge_cutter) {
            camcraft_tools::cutter_profile::agrees(&next, &c).map_err(|e| {
                CommandError::Invalid(format!(
                    "tool '{name}': the drawn cutter profile no longer fits: {e} - reset the cutter profile to Parametric first"
                ))
            })?;
        }
        Ok(ChangeTool {
            name: name.to_string(),
            prev,
            next,
        })
    }
}

impl Command for ChangeTool {
    // #221: the number is the tool table's, not the geometry's - a
    // ChangeTool (either way) keeps whatever number the tool has now.
    fn apply(&self, world: &World) {
        if let Some(entity) = model::find_by_name(world, &self.name) {
            let number = model::tool_number_of(world, &entity);
            entity
                .entity_view(world)
                .set(ToolDef { tool: self.next.clone(), number });
        }
    }

    fn invert(&self, world: &World) {
        if let Some(entity) = model::find_by_name(world, &self.name) {
            let number = model::tool_number_of(world, &entity);
            entity
                .entity_view(world)
                .set(ToolDef { tool: self.prev.clone(), number });
        }
    }
}

/// Delete a tool entity; the snapshot restores it identically, undoable
/// like every other persistent object.
pub struct DeleteTool {
    name: String,
    tool: Tool,
    /// #221: the tool's number, restored with it on undo.
    number: Option<u32>,
    /// #368: its modelled body, restored with it on undo.
    body: Option<ToolBody>,
    /// #93: its drawn cutter, restored with it on undo.
    cutter: Option<camcraft_tools::spun::SpunProfile>,
    location: Location,
}

impl DeleteTool {
    pub fn new(world: &World, name: &str) -> Result<Self, CommandError> {
        let Some(entity) = model::find_by_name(world, name) else {
            return Err(CommandError::NotFound(name.to_string()));
        };
        let tool = model::tool_of(world, &entity)
            .ok_or_else(|| CommandError::Invalid(format!("'{name}' is not a tool")))?;
        Ok(DeleteTool {
            name: name.to_string(),
            tool,
            number: model::tool_number_of(world, &entity),
            body: crate::tool_body::body_of(world, &entity),
            cutter: crate::tool_cutter::cutter_of(world, &entity),
            location: model::transform_of(world, &entity).unwrap_or_default(),
        })
    }
}

impl Command for DeleteTool {
    fn apply(&self, world: &World) {
        if let Some(entity) = model::find_by_name(world, &self.name) {
            model::remove(world, &entity);
        }
    }

    fn invert(&self, world: &World) {
        if model::find_by_name(world, &self.name).is_some() {
            return; // already restored (idempotent redo path)
        }
        let entity = model::spawn(world, &self.name);
        model::set_transform(world, &entity, self.location);
        entity
            .entity_view(world)
            .set(ToolDef { tool: self.tool.clone(), number: self.number });
        crate::tool_body::set_body(world, &entity, self.body.as_ref());
        crate::tool_cutter::set_cutter(world, &entity, self.cutter.as_ref());
    }
}
