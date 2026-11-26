//! The flute-reach law (#3525): a cut deeper below the stock top than the
//! tool's flute length puts the tool body above the flutes into the cut -
//! the shop rule that the depth of cut stays within the flute length
//! (machine-tool handbooks, end mill selection). Our own reading.
//!
//! The law names the CAUSE that the shank and holder laws report one move
//! at a time: the operation asks for a depth that the flutes cannot reach.
//! It reads the path's lowest cutting tip against the stock top (the
//! stock's highest point, the conservative reading for a box or flat
//! stock) and compares that depth with the flute length.
//!
//! The finding is advisory (#3525 decision). The body above the flutes in
//! the cut CAN meet the walls, but need not: a contour's shank can pass
//! beside the wall. The shank and holder laws judge each move against the
//! real body, and their findings hold the post when it does meet material.

use super::types::{ToolGeom, Warning};
use crate::cl::{MoveKind, Toolpath};

/// The slack under which a depth equal to the flute length is no finding
/// (mm).
const SLACK: f64 = 1e-3;

/// The flute-reach finding of `path` against a stock whose top stands at
/// `stock_top_z`, or `None` when every cutting tip stays within the flute
/// length (or the tool or the top is not finite: there is nothing to read).
#[must_use]
pub fn flute_reach(path: &Toolpath, tool: &ToolGeom, stock_top_z: f64) -> Option<Warning> {
    let flute = tool.flute_len;
    if !(flute.is_finite() && flute > 0.0 && stock_top_z.is_finite()) {
        return None;
    }
    let reach_z = stock_top_z - flute;
    let mut first: Option<usize> = None;
    let mut low = f64::INFINITY;
    for (i, m) in path.moves.iter().enumerate() {
        if m.kind == MoveKind::Rapid {
            continue;
        }
        let z = m.from.z.min(m.to.z);
        if !z.is_finite() {
            continue;
        }
        if z < reach_z - SLACK && first.is_none() {
            first = Some(i);
        }
        low = low.min(z);
    }
    let at = first?;
    Some(Warning::FluteShort { at, flute_mm: flute, depth_mm: stock_top_z - low })
}

/// The finding as the note that names the tool (the warning itself is
/// `Copy` and carries no name): `None` for any other warning. It states
/// the facts and what MAY follow; it claims no contact.
#[must_use]
pub fn flute_note(tool_name: &str, w: &Warning) -> Option<String> {
    let Warning::FluteShort { flute_mm, depth_mm, .. } = w else {
        return None;
    };
    Some(format!(
        "{tool_name} has a flute length of {flute_mm:.3} mm; this operation cuts {depth_mm:.3} mm below the stock top; \
         above the flutes, the shank or the holder goes into the cut and can rub or collide - use a tool with a flute \
         length of at least {depth_mm:.3} mm, or cut less deep"
    ))
}
