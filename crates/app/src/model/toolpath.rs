//! Reads of an operation's cached toolpath that do not clone it (#2201).
//! Provenance: our own design notes; no vendor material consulted.

use flecs_ecs::core::EntityViewGet; // not re-exported in prelude (0.2.2)
use flecs_ecs::prelude::{Entity, World};

use crate::schema::Toolpath;

/// True when the operation has a cached toolpath with at least one move
/// (without cloning the move vector).
#[must_use]
pub fn toolpath_nonempty(world: &World, entity: &Entity) -> bool {
    entity
        .entity_view(world)
        .try_get::<&Toolpath>(|t| !t.path.is_empty())
        .unwrap_or(false)
}
