//! #1720: a WCS axis may run reversed (a "flip": X+ becomes X-), stored
//! as `schema::WcsFlip` on the setup's WCS entity. Some machines have a
//! reversed axis direction. Carrying a flip into the posted program is
//! new frame work (a left-handed frame: coordinate signs, `G2`/`G3`,
//! `G41`/`G42`, the tool axis for Z - the part 2 of issue 1649), so until
//! it lands the post AND toolpath generation refuse a flipped WCS by
//! name, the way the post refuses a rotated one (#315). No program is
//! ever posted or generated with a flip set.
//!
//! Provenance: our own design notes (RS-274 / ISO 6983 work offsets are
//! translation only; a mirrored frame is not a rotation); no vendor
//! material consulted.

use flecs_ecs::prelude::*;

use crate::model;
use crate::schema::{Operation, WcsFlip};

const AXES: [&str; 3] = ["X", "Y", "Z"];

/// The flip flags (X, Y, Z) of a WCS entity; all false when it has none.
#[must_use]
pub fn of_entity(world: &World, entity: &Entity) -> [bool; 3] {
    entity.entity_view(world).try_get::<&WcsFlip>(|f| f.0).unwrap_or_default()
}

/// Write the flags on a WCS entity: all false removes the component, so
/// "no flip" is stored as absent (what every older file holds).
pub fn set(world: &World, entity: &Entity, flip: [bool; 3]) {
    let view = entity.entity_view(world);
    if flip.iter().any(|f| *f) {
        view.set(WcsFlip(flip));
    } else {
        view.remove(world.component::<WcsFlip>());
    }
}

/// The flags of a setup's WCS as declared (`None`: the active setup);
/// all false when the WCS is not placed or has no flip.
#[must_use]
pub fn of_setup(world: &World, declared: Option<&str>) -> [bool; 3] {
    crate::setup::wcs_entity(world, declared).map(|e| of_entity(world, &e)).unwrap_or_default()
}

/// The flipped axes as a user reads them: "X", "X and Z", "X, Y and Z".
#[must_use]
pub fn axes_text(flip: [bool; 3]) -> String {
    let names: Vec<&str> = AXES.iter().zip(flip).filter(|(_, f)| *f).map(|(n, _)| *n).collect();
    match names.as_slice() {
        [] => String::new(),
        [one] => (*one).to_owned(),
        [init @ .., last] => format!("{} and {last}", init.join(", ")),
    }
}

/// The refusal for a setup as declared, `None` when no axis is flipped.
#[must_use]
pub fn setup_problem(world: &World, declared: Option<&str>) -> Option<String> {
    let flip = of_setup(world, declared);
    if !flip.iter().any(|f| *f) {
        return None;
    }
    let word = crate::setup::wcs_name(world, declared);
    let owner = match crate::setup::effective(world, declared) {
        Some(s) => format!("setup '{s}': its WCS ({word})"),
        None => format!("the WCS ({word})"),
    };
    let n = flip.iter().filter(|f| **f).count();
    let axis = if n == 1 { "axis" } else { "axes" };
    Some(format!(
        "{owner} has its {} {axis} flipped; posting or generating with a flipped WCS is not \
         available yet - clear the flip on the WCS page",
        axes_text(flip)
    ))
}

/// Why `op` may not be generated: its setup's WCS is flipped. Read at
/// the door of every generation (`collect_job`), beside #1510's tilt.
#[must_use]
pub fn generation_problem(world: &World, op: &Operation) -> Option<String> {
    setup_problem(world, op.setup.as_deref())
}

/// The post clause: one line per flipped setup a posted (not suppressed)
/// operation uses, with the operations, in first-use program order.
/// Empty when no posted operation runs in a flipped setup.
#[must_use]
pub fn post_problems(world: &World) -> Vec<String> {
    let mut hits: Vec<(Option<String>, String, Vec<String>)> = Vec::new();
    for (name, e) in crate::setup::operation_order(world) {
        let Some(op) = model::operation_of(world, &e) else {
            continue;
        };
        if op.suppressed {
            continue; // not in the program (#110)
        }
        let setup = crate::setup::effective(world, op.setup.as_deref());
        let label = model::operation_label(world, &name, &e);
        if let Some(hit) = hits.iter_mut().find(|h| h.0 == setup) {
            hit.2.push(label);
            continue;
        }
        if let Some(why) = setup_problem(world, op.setup.as_deref()) {
            hits.push((setup, why, vec![label]));
        }
    }
    hits.into_iter()
        .map(|(_, why, ops)| format!("{why} (operations: {})", ops.join(", ")))
        .collect()
}
