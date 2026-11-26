//! Rest machining on the 2D Pocket (#1489): the parameter door's
//! refusals, said in the generator's own words (`machining2d::rest2d`,
//! #1446), so an edit is refused when it is made, never first at
//! Generate.
//!
//! #1446: the 2D Adaptive carries the same group. It reads the previous
//! operations only (`machining2d::rest_adaptive_previous`): the entered
//! previous tool, the Trochoidal method, arc turns, corner relief and a
//! tilt are refused by name beside it (pre-drilled entry is the
//! Pocket's alone already).
//!
//! Provenance: our own operation laws over this project's rest
//! generator; no other CAM product's code, text or output consulted.

use crate::machining2d::{AdaptiveMethod, GenError, REST_ADAPTIVE_ENTERED, RestParams, RestSource, rest_strategy};
use crate::model;
use flecs_ecs::prelude::World;

use super::{OpParams, OpStatus};

/// Previous-operations rest mode on a tilted plane (#2033): the stock
/// grid is read along the setup's Z only, so the plane's own depths are
/// not proved by it.
pub const REST_PREVIOUS_TILTED: &str = "rest machining from the previous operations is not available on a tilted Pocket: the stock of the previous operations is read along the setup's Z only - clear the tilt, or use the entered previous tool";

/// The same refusal on a 2D Adaptive (#1446), which has no entered tool.
pub const REST_ADAPTIVE_TILTED: &str = "rest machining is not available on a tilted 2D Adaptive: the stock of the previous operations is read along the setup's Z only - clear the tilt, or turn rest machining off";

/// Rest machining beside the Trochoidal method (#1446): the rest passes
/// are planned on the remaining stock, not along the region's medial line.
pub const REST_ADAPTIVE_TROCHOIDAL: &str = "rest machining is not available with the Trochoidal method: the rest passes are planned on the remaining stock - choose the Spiral method, or turn rest machining off";

/// Rest machining beside arc turns (#1446): the rest planner rolls its
/// own arcs off the walls and does not read the option.
pub const REST_ADAPTIVE_ARC_TURNS: &str = "arc turns at corners are not available with rest machining on a 2D Adaptive - turn off one of the two";

/// Rest machining beside corner relief (#1446, #2400): the rest
/// Adaptive planner cuts the material left by the previous operations
/// and does not cut inside-corner reliefs.
pub const REST_ADAPTIVE_CORNER_RELIEF: &str = "corner relief is not available with rest machining on a 2D Adaptive - turn off one of the two";

/// The tilt refusal of previous-operations rest mode, in the words of
/// the kind of `p`.
pub(super) fn tilted_refusal(p: &OpParams) -> &'static str {
    if matches!(p, OpParams::Adaptive2D { .. }) { REST_ADAPTIVE_TILTED } else { REST_PREVIOUS_TILTED }
}

/// Is `p` a 2D Pocket or a 2D Adaptive whose rest area is read from the
/// previous operations (#2033, #1446)?
pub fn reads_previous_ops(p: &OpParams) -> bool {
    rest_of(p).is_some_and(|r| r.source == RestSource::PreviousOperations)
}

/// Why the stock ahead of `name` cannot be established for
/// previous-operations rest mode (#2033): an earlier operation of its
/// setup has no current toolpath, or is suppressed with a cut on record.
/// The stock is then not guessed: generation refuses by this name.
pub(super) fn previous_ops_refusal(world: &World, name: &str, setup: Option<&str>) -> Option<String> {
    let consumer = crate::setup::effective(world, setup);
    let head = "rest machining from the previous operations";
    for (n, e) in crate::setup::operation_order(world) {
        if n == name {
            break;
        }
        let Some(op) = model::operation_of(world, &e) else {
            continue;
        };
        if crate::setup::effective(world, op.setup.as_deref()) != consumer {
            continue;
        }
        if op.suppressed {
            // A suppressed cut is never made: it proves nothing.
            if model::toolpath_nonempty(world, &e) {
                return Some(format!(
                    "{head}: operation '{n}' ahead is suppressed, so its cut is not made - unsuppress it, or use the entered previous tool"
                ));
            }
            continue;
        }
        match super::status_of(world, &e) {
            OpStatus::Ok => {}
            OpStatus::Never => {
                return Some(format!("{head}: operation '{n}' ahead has no toolpath - generate the program in order"));
            }
            OpStatus::Stale => {
                return Some(format!(
                    "{head}: the toolpath of operation '{n}' ahead is stale - regenerate the program in order"
                ));
            }
        }
    }
    None
}

/// Rest machining beside pre-drilled entry: the rest pass enters its
/// own areas and does not read the holes yet, so one of them is turned
/// off rather than one silently dropped.
pub const REST_PREDRILLED: &str =
    "rest machining does not use pre-drilled entry yet - turn off one of the two on this Pocket";

/// The generator's refusal text, without its "invalid machining
/// parameters" heading.
fn said(e: GenError) -> String {
    match e {
        GenError::Invalid(s) | GenError::Empty(s) => s,
        other => other.to_string(),
    }
}

/// The rest group of `p`, when it is a 2D Pocket or a 2D Adaptive with
/// rest machining on.
pub fn rest_of(p: &OpParams) -> Option<&RestParams> {
    match p {
        OpParams::Pocket { rest: Some(r), .. } | OpParams::Adaptive2D { rest: Some(r), .. } => Some(r),
        _ => None,
    }
}

/// The refusal of a 2D Adaptive's rest group (#1446): the entered tool
/// (it proves nothing), values the generator refuses, and each option
/// the rest planner does not read - refused, never silently dropped.
fn adaptive_rest_refusal(p: &OpParams) -> Option<String> {
    let OpParams::Adaptive2D { rest: Some(r), method, blend_turns, corner_relief, .. } = p else {
        return None;
    };
    if r.source != RestSource::PreviousOperations {
        return Some(REST_ADAPTIVE_ENTERED.to_string());
    }
    if let Err(e) = r.validate_alone() {
        return Some(said(e));
    }
    let why = if *method == AdaptiveMethod::Trochoidal {
        REST_ADAPTIVE_TROCHOIDAL
    } else if *blend_turns {
        REST_ADAPTIVE_ARC_TURNS
    } else if !corner_relief.is_off() {
        REST_ADAPTIVE_CORNER_RELIEF
    } else if p.tilt().is_some() {
        REST_ADAPTIVE_TILTED
    } else {
        return None;
    };
    Some(why.to_string())
}

/// The refusal of a Pocket's rest group that needs no tool: values the
/// generator refuses, a strategy it does not run, pre-drilled entry.
pub fn rest_refusal(p: &OpParams) -> Option<String> {
    let OpParams::Pocket { rest: Some(r), strategy, links, .. } = p else {
        return adaptive_rest_refusal(p);
    };
    if let Err(e) = r.validate_alone() {
        return Some(said(e));
    }
    if let Err(e) = rest_strategy(*strategy) {
        return Some(said(e));
    }
    if r.source == RestSource::PreviousOperations && p.tilt().is_some() {
        return Some(REST_PREVIOUS_TILTED.to_string());
    }
    links.approach.predrilled.then(|| REST_PREDRILLED.to_string())
}

/// The refusal of a Pocket's rest group against the tool it mounts: the
/// previous tool must be larger than `tool_diameter`.
pub fn rest_tool_refusal(p: &OpParams, tool_diameter: f64) -> Option<String> {
    rest_of(p).and_then(|r| r.validate(tool_diameter).err()).map(said)
}
