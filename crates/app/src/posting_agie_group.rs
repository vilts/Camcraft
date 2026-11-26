//! The modeled workpiece of each operation of a native wire EDM job: one work, a group of two to
//! nine independently captured punches (#2961, #2980), or two openings of one die plate (#2977).
//! Provenance: CAMCraft's Wire Contour workflow.

use flecs_ecs::prelude::World;

use super::AgieJob;
use crate::operation::{self, OpParams, SrcRef, WireSide};
use crate::{model, schema::Operation};

/// The nominal jobs of the active operations, in program order.
///
/// One operation needs exactly one modeled workpiece. A group needs one
/// complete modeled punch for each operation, a different model for each,
/// and no other model or placed drawing, so that no unowned part can meet a
/// controller path.
/// A die group needs one retained plate, with one through-opening for each
/// operation, as the only model and no placed drawing.
pub(super) fn jobs(
    world: &World,
    operations: &[(&str, Operation)],
) -> Result<Vec<AgieJob>, String> {
    let models = model::model_entities(world);
    if let [(name, op)] = operations {
        let [(source, _)] = models.as_slice() else {
            return Err("The native wire EDM job requires exactly one modeled workpiece; additional parts or fixture models can intersect the controller path".into());
        };
        return Ok(vec![operation::agie_job(world, op, name, source, 1)?]);
    }
    let is_die = |op: &Operation| matches!(&op.params, OpParams::WireContour(wire) if wire.side == WireSide::Die);
    if operations.iter().all(|(_, op)| is_die(op)) {
        return die_group(world, operations, &models);
    }
    let mut owners: Vec<&str> = Vec::with_capacity(operations.len());
    for (_, op) in operations {
        let SrcRef::Entity(owner) = &op.src else {
            return Err("Select one complete modeled punch for each work of a punch group".into());
        };
        if owners.contains(&owner.as_str()) {
            return Err("The works of a punch group must cut different modeled punches".into());
        }
        owners.push(owner);
    }
    if models.len() != owners.len()
        || models
            .iter()
            .any(|(model, _)| !owners.contains(&model.as_str()))
    {
        return Err("Each modeled workpiece of a punch group must be the punch of one work; additional parts or fixture models can intersect the controller path".into());
    }
    let setup = operations.first().and_then(|(_, op)| op.setup.as_deref());
    if !crate::sketch::placed_in(world, setup).is_empty() {
        return Err("The modeled punches of a punch group must be the only workpieces; remove the placed drawing".into());
    }
    operations
        .iter()
        .zip(owners)
        .map(|((name, op), owner)| operation::agie_job(world, op, name, owner, 1))
        .collect()
}

/// The jobs of a die group: each operation selects one through-opening of the
/// same retained plate. Each selected rim must match one opening. The post
/// refuses two works whose cutting areas meet, so a group cannot select the
/// same opening twice and leave the other opening uncut.
fn die_group(
    world: &World,
    operations: &[(&str, Operation)],
    models: &[(String, flecs_ecs::prelude::Entity)],
) -> Result<Vec<AgieJob>, String> {
    if operations.len() != 2 {
        return Err("A die group holds two openings of one plate. The control checked die groups of two openings only".into());
    }
    let [(plate, _)] = models else {
        return Err("The operations of a die group must select through-openings of the same retained plate, with no other model".into());
    };
    let setup = operations.first().and_then(|(_, op)| op.setup.as_deref());
    if !crate::sketch::placed_in(world, setup).is_empty() {
        return Err("The retained plate of a die group must be the only workpiece; remove the placed drawing".into());
    }
    operations
        .iter()
        .map(|(name, op)| operation::agie_job(world, op, name, plate, operations.len()))
        .collect()
}
