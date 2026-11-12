//! The family door at post (#530): an op made on one machine and then
//! remounted onto steel that cannot run it (an EDM, a lathe under a
//! milling op) is refused by name - never posted in a dialect its
//! machine does not speak. The same law as the create-time
//! `family_gate`, read again over the program the post is about to emit.
//!
//! Provenance: our own rule (the umbrella #290's design notes D1-D3:
//! an EDM runs no milling or turning op); split out of `posting` so the
//! posting module does not grow.

use flecs_ecs::prelude::*;

use crate::model;

/// Refuse the first program op whose family its setup's machine does
/// not admit, naming the op, its kind and the machine kind.
///
/// # Errors
/// `"<op>: a <kind> operation cannot run on its setup's <machine> machine - ..."`.
pub(crate) fn family_at_post(world: &World, ops: &[(String, Entity)]) -> Result<(), String> {
    for (name, e) in ops {
        if let Some(op) = model::operation_of(world, e) {
            let machine = crate::machine::kind_of_setup(world, op.setup.as_deref());
            if !op.params.kind().info().admits(machine) {
                return Err(format!(
                    "{name}: a {} operation cannot run on its setup's {} machine - mount a machine that runs it, or suppress the operation",
                    op.params.kind(),
                    machine.label()
                ));
            }
        }
    }
    Ok(())
}
