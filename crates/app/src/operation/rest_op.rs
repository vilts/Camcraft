//! Rest machining on the 2D Pocket (#1489): the parameter door's
//! refusals, said in the generator's own words (`machining2d::rest2d`,
//! #1446), so an edit is refused when it is made, never first at
//! Generate.
//!
//! Provenance: our own operation laws over this project's rest
//! generator; no other CAM product's code, text or output consulted.

use crate::machining2d::{GenError, RestParams, rest_strategy};

use super::OpParams;

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

/// The rest group of `p`, when it is a 2D Pocket with rest machining on.
pub fn rest_of(p: &OpParams) -> Option<&RestParams> {
    match p {
        OpParams::Pocket { rest: Some(r), .. } => Some(r),
        _ => None,
    }
}

/// The refusal of a Pocket's rest group that needs no tool: values the
/// generator refuses, a strategy it does not run, pre-drilled entry.
pub fn rest_refusal(p: &OpParams) -> Option<String> {
    let OpParams::Pocket { rest: Some(r), strategy, links, .. } = p else {
        return None;
    };
    if let Err(e) = r.validate_alone() {
        return Some(said(e));
    }
    if let Err(e) = rest_strategy(*strategy) {
        return Some(said(e));
    }
    links.approach.predrilled.then(|| REST_PREDRILLED.to_string())
}

/// The refusal of a Pocket's rest group against the tool it mounts: the
/// previous tool must be larger than `tool_diameter`.
pub fn rest_tool_refusal(p: &OpParams, tool_diameter: f64) -> Option<String> {
    rest_of(p).and_then(|r| r.validate(tool_diameter).err()).map(said)
}
