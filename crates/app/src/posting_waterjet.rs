//! Provenance: our own safety refusal for incomplete waterjet output (#3637).

use flecs_ecs::prelude::World;

pub(super) fn problems(world: &World) -> Vec<String> {
    // A program that cannot be listed is refused by the sim clause with
    // its own reason; repeating it here would name the fault twice.
    let Ok(ops) = super::program_ops(world) else {
        return Vec::new();
    };
    ops.into_iter()
        .filter(|(_, entity)| {
            crate::model::operation_of(world, entity)
                .is_some_and(|op| op.params.kind().info().family() == crate::operation::Family::Waterjet)
        })
        .map(|(name, _)| format!(
            "{name}: waterjet posting is unavailable: standoff height, jet on/off records, pierce dwell records, and waterjet laws are not implemented"
        ))
        .collect()
}
