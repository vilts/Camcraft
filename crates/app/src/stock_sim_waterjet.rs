//! Provenance: our own refusal to judge waterjet removal with a milling stock model (#3637).

use crate::{model, operation::Family, schema::Operation};

/// Waterjet removal has no validated stock model or waterjet laws yet.
#[must_use]
pub fn skips(op: &Operation) -> bool {
    op.params.kind().info().family() == Family::Waterjet
}

pub(crate) fn refuse_waterjet_cuts(world: &flecs_ecs::prelude::World) -> Result<(), String> {
    for (name, entity) in crate::posting::program_ops(world)? {
        if model::operation_of(world, &entity).is_some_and(|op| skips(&op)) {
            return Err(format!("{name}: waterjet stock removal is not judged: the stock sim has no waterjet model"));
        }
    }
    Ok(())
}
