//! #1720, #1649: a WCS axis may run reversed (a "flip": X+ becomes X-),
//! stored as `schema::WcsFlip` on the setup's WCS entity. Some machines
//! have a reversed axis direction.
//!
//! The meaning (#1649): the flipped axis reads the same physical point
//! with the other sign, from the same origin. The cut does not change,
//! so generation, verification and the stock sim stay in the setup
//! frame; the post mirrors the section into the controller's frame
//! (`post::wcs_flip`: coordinate signs, `G2`/`G3`, `G41`/`G42`). This
//! module reads and writes the flags and names, at the verification
//! gate, each case the post does not carry: a flipped Z, a flipped WCS
//! tilted off Z, a post script not verified for a mirrored
//! program, operation types outside 3-axis milling, and probing.
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

fn owner_of(world: &World, declared: Option<&str>) -> String {
    let word = crate::setup::wcs_name(world, declared);
    match crate::setup::effective(world, declared) {
        Some(s) => format!("setup '{s}': its WCS ({word})"),
        None => format!("the WCS ({word})"),
    }
}

/// Why the post does not carry a setup's flip, `None` when no axis is
/// flipped or the flip is carried (X and/or Y, square or Z-rotated, through a post
/// script verified for a mirrored program).
#[must_use]
pub fn setup_problem(world: &World, declared: Option<&str>) -> Option<String> {
    let flip = of_setup(world, declared);
    if !flip.iter().any(|f| *f) {
        return None;
    }
    let n = flip.iter().filter(|f| **f).count();
    let axis = if n == 1 { "axis" } else { "axes" };
    let what = format!("{} has its {} {axis} flipped", owner_of(world, declared), axes_text(flip));
    if flip[2] {
        // #2474: the exact reason, in the post engine's own words.
        let why = crate::post::wcs_flip::Z_REASON;
        return Some(format!("{what}; a flipped Z is not carried into the program yet: {why} - clear Flip Z on the WCS page"));
    }
    let rotated = crate::setup::wcs_entity(world, declared)
        .and_then(|e| model::transform_of(world, &e))
        .is_some_and(|loc| !matches!(
            crate::posting::offset_frame(&loc),
            crate::posting::OffsetFrame::Square | crate::posting::OffsetFrame::Yaw { .. }
        ));
    if rotated {
        return Some(format!(
            "{what} and has a rotated or invalid placement; only a finite Z rotation is supported with \
             flipped axes - clear the flip or correct the placement"
        ));
    }
    let post = crate::setup_mount::post_of(world, declared);
    if let Ok((name, text)) = crate::posting::script_of(&post)
        && matches!(crate::post::dialect::translates_inverted_axes(&text), Ok(false))
    {
        return Some(format!(
            "{what}, and its post ({name}) is not verified for a mirrored program - choose a post that is \
             (each built-in mill post is), or clear the flip on the WCS page"
        ));
    }
    None
}

/// Why one operation cannot post in a flipped WCS that is otherwise
/// carried: the mirror is verified for 3-axis milling only, and probing
/// is refused (#1649, the maintainer's decision of 2026-11-18): the
/// probe's work-offset write (`G10 L2`) is proven for a machine axis that
/// runs reversed (#392), not for a control that mirrors one work offset.
fn op_problem(op: &Operation, cache: Option<crate::schema::Toolpath>) -> Option<&'static str> {
    use crate::operation::{OpParams, Section};
    if matches!(op.params, OpParams::Probe { .. }) {
        return Some(
            "probing does not post in a flipped WCS: the probe's work-offset write is not verified for a \
             mirrored work offset - probe in a setup without a flip, or clear the flip on the WCS page",
        );
    }
    if matches!(
        op.params,
        OpParams::WireContour(_)
            | OpParams::SinkerBurn(_)
            | OpParams::SinkerSideBurn(_)
            // #1444: not verified mirrored (the climb side swaps).
            | OpParams::SlotUndercut(_)
            | OpParams::RotaryContour { .. }
            | OpParams::RotaryParallel { .. }
            | OpParams::RotaryOffset { .. }
            | OpParams::MultiAxisRough { .. }
            | OpParams::Flow5D { .. }
            | OpParams::MultiAxisContour { .. }
            | OpParams::MultiAxisDeburr { .. }
            // #2826: the band's cylinders and rotary words are not
            // verified mirrored.
            | OpParams::RotaryFlow { .. }
    ) || op.params.kind().info().section == Section::Turning
    {
        return Some("that operation type is not supported in a flipped WCS yet");
    }
    if op.params.tilt().is_some() {
        return Some("3+2 tilted toolpaths are not supported in a flipped WCS yet");
    }
    let turned = cache.is_some_and(|c| {
        c.path.moves.iter().any(|m| m.rotary.is_some() || m.axis.is_some_and(|a| !crate::cl::is_vertical(&a)))
    });
    turned.then_some("multiaxis or rotary moves are not supported in a flipped WCS yet")
}

/// The post clause: one line per refused case, with the operations, in
/// first-use program order. Empty when no posted (not suppressed)
/// operation runs in a flipped setup, or each flip is carried.
#[must_use]
pub fn post_problems(world: &World) -> Vec<String> {
    problems(world, None)
}

/// [`post_problems`] for the operations of one setup only (the name as
/// `setup::effective` reads it) - what the MCP flip tool reports back.
#[must_use]
pub fn setup_post_problems(world: &World, declared: Option<&str>) -> Vec<String> {
    let want = crate::setup::effective(world, declared);
    problems(world, Some(want.as_deref()))
}

fn problems(world: &World, only: Option<Option<&str>>) -> Vec<String> {
    let mut hits: Vec<(String, Vec<String>)> = Vec::new();
    for (name, e) in crate::setup::operation_order(world) {
        let Some(op) = model::operation_of(world, &e) else {
            continue;
        };
        if op.suppressed {
            continue; // not in the program (#110)
        }
        let declared = op.setup.as_deref();
        if only.is_some_and(|want| crate::setup::effective(world, declared).as_deref() != want) {
            continue;
        }
        let flip = of_setup(world, declared);
        if !flip.iter().any(|f| *f) {
            continue;
        }
        let why = match setup_problem(world, declared) {
            Some(why) => why,
            None => match op_problem(&op, model::toolpath_of(world, &e)) {
                Some(kind) => {
                    let axis = if flip.iter().filter(|f| **f).count() == 1 { "axis" } else { "axes" };
                    format!("{} has its {} {axis} flipped; {kind}", owner_of(world, declared), axes_text(flip))
                }
                None => continue,
            },
        };
        let label = model::operation_label(world, &name, &e);
        if let Some(hit) = hits.iter_mut().find(|h| h.0 == why) {
            hit.1.push(label);
        } else {
            hits.push((why, vec![label]));
        }
    }
    hits.into_iter().map(|(why, ops)| format!("{why} (operations: {})", ops.join(", "))).collect()
}
