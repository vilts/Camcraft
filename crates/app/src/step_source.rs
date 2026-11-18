//! Raw STEP source tracking for imported solids (#744).
//! Provenance: clean-room ECS helper preserving original STEP bytes on entities.

use flecs_ecs::core::EntityViewGet;
use flecs_ecs::prelude::{Entity, World};

use crate::schema::StepSource;

/// Returns the stored raw STEP source for `entity`, if imported from STEP.
#[must_use]
pub fn of(world: &World, entity: &Entity) -> Option<String> {
    entity
        .entity_view(world)
        .try_get::<&StepSource>(|s| s.0.clone())
        .filter(|s| !s.is_empty())
}

/// Attach (`Some`) or remove (`None`) the raw STEP source on `entity`.
pub fn set(world: &World, entity: &Entity, src: Option<&str>) {
    let view = entity.entity_view(world);
    match src.filter(|s| !s.trim().is_empty()) {
        Some(s) => {
            view.set(StepSource(s.to_owned()));
        }
        None => {
            view.remove(world.component::<StepSource>());
        }
    }
}
