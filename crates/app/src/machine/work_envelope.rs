//! The mounted machine's work envelope as the viewport draws it (#1567):
//! the active setup's machine and the setup's work origin, read by the same
//! law the travel check uses (`posting::wcs_context`). Display only.
//! Provenance: our own glue over `camcraft_post::machine::envelope`.

use flecs_ecs::prelude::World;

use super::envelope::{envelope, EnvelopeShape};
use crate::posting;

/// The active setup's envelope, or the reason there is none ("no machine
/// is mounted", or the missing travel figure). Never a guessed volume.
///
/// #1649: on a flipped WCS axis the program's plus stroke lies on the
/// world's minus side of the origin, so the box mirrors about the origin
/// there: the same sign the travel check reads. A lathe's turned envelope
/// is symmetric in X, and a flipped Z never posts.
pub fn work_envelope(world: &World) -> Result<EnvelopeShape, String> {
    let (_, machine) = super::of(world);
    let Some(machine) = machine else {
        return Err("no machine is mounted".into());
    };
    let active = crate::setup::active(world);
    let (_, origin) = posting::wcs_context(world, active.as_deref());
    let flip = crate::setup_wcs_flip::of_setup(world, active.as_deref());
    Ok(match envelope(&machine, origin)? {
        EnvelopeShape::Box { mut min, mut max } => {
            for k in 0..3 {
                if flip.get(k).copied().unwrap_or(false)
                    && let (Some(lo), Some(hi), Some(o)) = (min.get_mut(k), max.get_mut(k), origin.get(k))
                {
                    (*lo, *hi) = (2.0 * o - *hi, 2.0 * o - *lo);
                }
            }
            EnvelopeShape::Box { min, max }
        }
        other => other,
    })
}
