//! The wire clause of the program sim (#1110, part of #290): a wire EDM
//! cut mounts no library tool (`KindInfo::mounts_tool`, #532) and cuts
//! THROUGH the work along a kerf the dexel column model has no atom
//! for, so it stays out of the dexel plan the way a probe and a torch
//! do - and the verdict says so by name instead of reading the stock
//! as uncut. The wire's own verdicts are the wire laws' (#534).
//!
//! Provenance: our own rule over our own plan walk; no vendor material
//! consulted.

use crate::model;
use crate::schema::Operation;

/// Why the program sim does not judge a program holding a wire cut.
pub const WIRE_VERDICT_REFUSAL: &str = "a wire cut is verified by the wire laws, not the stock sim";

/// Whether the plan walk leaves `op` out: exactly the kinds that mount
/// no library tool (the wire family). A kind that mounts one is never
/// skipped here - a missing tool on it is still refused by name.
#[must_use]
pub fn skips(op: &Operation) -> bool {
    !op.params.kind().info().mounts_tool()
}

/// The verdict's refusal: the first program op the plan left out, by
/// name. Without it the column verdict would read the stock the wire
/// cut through as rest material - a wrong answer, not a judged one.
///
/// # Errors
/// The program holds a wire cut, or its operations cannot be read.
pub(crate) fn refuse_wire_cuts(world: &flecs_ecs::prelude::World) -> Result<(), String> {
    for (name, e) in crate::posting::program_ops(world)? {
        if model::operation_of(world, &e).is_some_and(|op| skips(&op)) {
            return Err(format!("{name}: {WIRE_VERDICT_REFUSAL}"));
        }
    }
    Ok(())
}
