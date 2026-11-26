//! Detect holes on a mill names its radial holes instead of storing them (#3448).
//! Provenance: our own reach rule - a mill drills along the setup's Z only; cross holes need a turret.

use flecs_ecs::prelude::World;

use crate::drilling::CrossHole;

/// The radial holes a Drill op may store from detection, and one sentence
/// for each one left out. A mill has no turret to plunge a cross hole
/// from, and Generate refuses a cross list there, so on a mill every
/// radial hole goes to `skipped` and none to the op's `cross` list.
pub(super) fn keep_reachable(world: &World, setup: Option<&str>, radial: Vec<CrossHole>) -> (Vec<CrossHole>, Vec<String>) {
    if crate::machine::kind_of_setup(world, setup) != crate::machine::Kind::Mill {
        return (radial, Vec::new());
    }
    let skipped = radial
        .iter()
        .map(|h| {
            format!(
                "the {:.3} mm radial hole at ({:.3}, {:.3}, {:.3}) along ({:.3}, {:.3}, {:.3}) was skipped: \
                 the drill reaches it only from the side, and the machine is a mill with no turret - \
                 drill it in a setup that faces it, or type it into a hole list by hand",
                h.radius * 2.0,
                tidy(h.origin.x),
                tidy(h.origin.y),
                tidy(h.origin.z),
                tidy(h.dir.x),
                tidy(h.dir.y),
                tidy(h.dir.z)
            )
        })
        .collect();
    (Vec::new(), skipped)
}

/// A printed coordinate without a "-0.000".
fn tidy(v: f64) -> f64 {
    if v.abs() < 5e-4 { 0.0 } else { v }
}
