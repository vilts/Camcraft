//! The file mirror of a Pause / Home entry of the operation list (#155).
//! Provenance: our own project format.
//!
//! An entry is an entity row of kind `entry` carrying this payload. No
//! version rung: a document without entries writes no such row, so its
//! bytes are unchanged, and a reader from before #155 meets a kind word
//! it does not know and refuses the file by name (serde's "unknown
//! variant `entry`") - never a silent load without the stops.

use serde::{Deserialize, Serialize};

use super::*;
use crate::operation::AfterOperation;
use crate::schema::ProgramEntry;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct ProgramEntryDto {
    /// `optional-stop` / `program-stop` / `go-home` - never `none`.
    pub action: AfterOperation,
    /// Position in the setup's list, the `OperationDto::order` law.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub order: Option<u32>,
    /// The setup it stands in; absent - the first.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub setup: Option<String>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub suppressed: bool,
}

/// The payload for an entry row, `None` on every other entity.
pub(super) fn collect(world: &World, entity: &flecs_ecs::prelude::Entity) -> Option<ProgramEntryDto> {
    crate::program_entry::entry_of(world, entity).map(|e| ProgramEntryDto {
        action: e.action,
        order: model::operation_meta_of(world, entity).order,
        setup: e.setup,
        suppressed: e.suppressed,
    })
}

/// The entry rows' structural law, run before any world write: the
/// payload rides an `entry` row and nothing else, and an entry row is
/// named, carries nothing but it, and asks something (not `none`).
pub(super) fn validate(dto: &EntityDto, kind: EntityKind) -> Result<(), ProjectError> {
    let is_entry = matches!(kind, EntityKind::Entry);
    if dto.entry.is_some() != is_entry {
        return Err(corrupt(format!(
            "'{}' {}",
            dto.name,
            if is_entry { "is an entry without its action" } else { "carries an entry's action but is not an entry" }
        )));
    }
    if !is_entry {
        return Ok(());
    }
    if dto.name.is_empty() {
        return Err(corrupt("an entry must be named"));
    }
    if dto.facets.is_some() || dto.stock.is_some() || dto.tool.is_some() || dto.operation.is_some() || dto.setup.is_some() {
        return Err(corrupt(format!("'{}' is an entry, not a solid, tool, operation or setup", dto.name)));
    }
    if let Some(e) = &dto.entry {
        if e.action == AfterOperation::None {
            return Err(corrupt(format!("entry '{}' asks nothing (continue)", dto.name)));
        }
        if e.setup.as_deref().is_some_and(str::is_empty) {
            return Err(corrupt(format!("entry '{}' names an empty setup", dto.name)));
        }
    }
    Ok(())
}

/// Land an entry row (validated already).
pub(super) fn apply(world: &World, entity: &flecs_ecs::prelude::Entity, dto: Option<ProgramEntryDto>) -> Result<(), ProjectError> {
    let e = dto.ok_or_else(|| corrupt("entry entity without its action"))?;
    entity.entity_view(world).set(ProgramEntry {
        action: e.action,
        setup: e.setup,
        suppressed: e.suppressed,
    });
    if e.order.is_some() {
        entity.entity_view(world).set(OperationMeta {
            order: e.order,
            ..Default::default()
        });
    }
    Ok(())
}
