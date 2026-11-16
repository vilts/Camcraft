//! The mounted machine's work envelope as the viewport draws it (#1567):
//! the active setup's machine and the setup's work origin, read by the same
//! law the travel check uses (`posting::wcs_context`). Display only.
//! Provenance: our own glue over `camcraft_post::machine::envelope`.

use flecs_ecs::prelude::World;

use super::envelope::{envelope, EnvelopeShape};
use crate::posting;

/// The active setup's envelope, or the reason there is none ("no machine
/// is mounted", or the missing travel figure). Never a guessed volume.
pub fn work_envelope(world: &World) -> Result<EnvelopeShape, String> {
    let (_, machine) = super::of(world);
    let Some(machine) = machine else {
        return Err("no machine is mounted".into());
    };
    let (_, origin) = posting::wcs_context(world, crate::setup::active(world).as_deref());
    envelope(&machine, origin)
}
