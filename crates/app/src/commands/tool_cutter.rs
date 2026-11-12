//! SetToolCutter (#93 slice 2): set or clear a milling tool's drawn
//! cutter, neck and shank outline as one undoable command. The outline
//! is validated against the tool's numbers at construction
//! (`cutter_profile::check`) and refused by name; `apply`/`invert` only
//! land the snapshot. Our own command model, the snapshot-and-restore
//! inverse every sibling uses. #93 slice 3: `ReshapeTool` lands the
//! tool's numbers and its drawn cutter together, one undo step.

use flecs_ecs::prelude::World;

use camcraft_tools::cutter_profile;
use camcraft_tools::spun::{SpunPoint, SpunProfile};

use crate::commands::{ChangeTool, Command, CommandError};
use crate::tools::Tool;
use crate::model;
use crate::tool_cutter;

/// Set (`Some`) or clear (`None`, back to the outline the numbers draw)
/// a tool's drawn cutter.
#[derive(Debug)]
pub struct SetToolCutter {
    name: String,
    prev: Option<SpunProfile>,
    next: Option<SpunProfile>,
}

impl SetToolCutter {
    /// Validate `points` against tool `name` now. Refused by name: no
    /// such tool, not a tool, a turning tool, and every
    /// `cutter_profile::check` refusal ("tool 'T1': cutter profile: ...").
    pub fn new(world: &World, name: &str, points: Option<Vec<SpunPoint>>) -> Result<Self, CommandError> {
        let Some(entity) = model::find_by_name(world, name) else {
            return Err(CommandError::NotFound(name.to_string()));
        };
        let tool = model::tool_of(world, &entity)
            .ok_or_else(|| CommandError::Invalid(format!("'{name}' is not a tool")))?;
        let next = match points {
            None => None,
            Some(p) => Some(
                cutter_profile::check(&tool, p)
                    .map_err(|e| CommandError::Invalid(format!("tool '{name}': cutter profile: {e}")))?,
            ),
        };
        Ok(SetToolCutter { name: name.to_string(), prev: tool_cutter::cutter_of(world, &entity), next })
    }

    /// The outline this command lands (`None`: a clear).
    #[must_use]
    pub fn next(&self) -> Option<&SpunProfile> {
        self.next.as_ref()
    }
}

impl Command for SetToolCutter {
    fn apply(&self, world: &World) {
        if let Some(entity) = model::find_by_name(world, &self.name) {
            tool_cutter::set_cutter(world, &entity, self.next.as_ref());
        }
    }

    fn invert(&self, world: &World) {
        if let Some(entity) = model::find_by_name(world, &self.name) {
            tool_cutter::set_cutter(world, &entity, self.prev.as_ref());
        }
    }
}

/// ReshapeTool (#93 slice 3): a milling tool's numbers and its drawn
/// cutter landed together as one undoable command - the outline editor
/// edits the tool by its parameters, and a drawn cutter is carried onto
/// the new numbers (`cutter_fit::refit`) instead of the number edit
/// being refused. Every `ChangeTool` law holds for the numbers; the
/// outline (`Some`) is judged against the NEW numbers by
/// `cutter_profile::check`, `None` clears it back to the numbers' own.
pub struct ReshapeTool {
    numbers: ChangeTool,
    name: String,
    prev: Option<SpunProfile>,
    next: Option<SpunProfile>,
}

impl ReshapeTool {
    /// Refused by name: every `ChangeTool` refusal, and every
    /// `cutter_profile::check` refusal against `tool`.
    pub fn new(world: &World, name: &str, tool: Tool, points: Option<Vec<SpunPoint>>) -> Result<Self, CommandError> {
        let next = match points {
            None => None,
            Some(p) => Some(
                cutter_profile::check(&tool, p)
                    .map_err(|e| CommandError::Invalid(format!("tool '{name}': cutter profile: {e}")))?,
            ),
        };
        let numbers = ChangeTool::judged(world, name, tool, false)?;
        let prev = model::find_by_name(world, name).and_then(|e| tool_cutter::cutter_of(world, &e));
        Ok(ReshapeTool { numbers, name: name.to_string(), prev, next })
    }

    /// The outline this command lands (`None`: the numbers' own).
    #[must_use]
    pub fn next(&self) -> Option<&SpunProfile> {
        self.next.as_ref()
    }
}

impl Command for ReshapeTool {
    fn apply(&self, world: &World) {
        self.numbers.apply(world);
        if let Some(entity) = model::find_by_name(world, &self.name) {
            tool_cutter::set_cutter(world, &entity, self.next.as_ref());
        }
    }

    fn invert(&self, world: &World) {
        if let Some(entity) = model::find_by_name(world, &self.name) {
            tool_cutter::set_cutter(world, &entity, self.prev.as_ref());
        }
        self.numbers.invert(world);
    }
}
