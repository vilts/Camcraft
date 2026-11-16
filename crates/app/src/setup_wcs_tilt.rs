//! #1510: a milling setup whose WCS Z axis is tilted off the machine's
//! Z is refused at generation, by name. Every milling generator cuts
//! along the machine's Z (a 3+2 tilt is its own frame), and a rotated
//! work offset does not post (#315); a path generated there machines
//! the part as it lies, not as the tilted WCS shows - the wrong face,
//! reported as OK. A part is turned for a setup with its part alignment
//! (#326), never with the WCS. A WCS turned only about Z (a squared-up
//! 3-2-1 offset) still generates; the post's #315 clause refuses it.
//!
//! Provenance: our own design notes over the 3-axis tool axis (the
//! spindle is the machine's Z; RS-274 / ISO 6983 work offsets carry no
//! tool orientation); no vendor material consulted.

use flecs_ecs::prelude::World;

use crate::geometry::{Point3, transform_point};
use crate::model;
use crate::schema::Operation;

/// Tilt angle (rad) past which the WCS Z counts as off the machine's Z:
/// the post's identity tolerance (`ROT_TOL_RAD`, 1e-9 rad), so nothing
/// the gate would call square is refused here.
const TILT_TOL_RAD: f64 = 1e-9;

/// The angle (deg) between a WCS rotation's Z axis and the machine's
/// +Z, `None` for a non-finite rotation (refused elsewhere, #327).
fn tilt_deg(rot: [f64; 4]) -> Option<f64> {
    let z = transform_point(Point3::new(0.0, 0.0, 1.0), Point3::ZERO, rot);
    if !(z.x.is_finite() && z.y.is_finite() && z.z.is_finite()) {
        return None;
    }
    let angle = (z.x * z.x + z.y * z.y).sqrt().atan2(z.z);
    (angle > TILT_TOL_RAD).then(|| angle.to_degrees())
}

/// Why `op` may not be generated in its setup: `Some` when the op runs
/// on a milling machine and its setup's WCS Z is tilted off the
/// machine's Z. Read at the door of every generation (`collect_job`).
#[must_use]
pub fn generation_problem(world: &World, op: &Operation) -> Option<String> {
    setup_problem(world, op.setup.as_deref())
}

/// [`generation_problem`] for a setup as declared (`None`: the active
/// one) - also what a facing pick on a face the tilted WCS calls level
/// is refused with, ahead of "not horizontal".
#[must_use]
pub fn setup_problem(world: &World, declared: Option<&str>) -> Option<String> {
    if crate::machine::kind_of_setup(world, declared) != crate::machine::Kind::Mill {
        return None;
    }
    let wcs = crate::setup::wcs_entity(world, declared)?;
    let loc = model::transform_of(world, &wcs)?;
    let deg = tilt_deg(loc.rot)?;
    // #931: the setup's post names its offset (display only).
    let word = crate::setup::wcs_name(world, declared);
    let owner = match crate::setup::effective(world, declared) {
        Some(s) => format!("setup '{s}': its WCS ({word})"),
        None => format!("the WCS ({word})"),
    };
    Some(format!(
        "{owner} has its Z axis tilted {deg:.3} deg off the machine's Z - the tool always cuts along \
         the machine's Z, so this would machine the part as it lies, not as the WCS shows; turn the \
         part with the setup's Part alignment instead and keep the WCS Z pointing up"
    ))
}
