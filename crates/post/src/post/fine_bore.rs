//! Fine boring in the post engine (#1445, #1479): the gate that refuses
//! the cycle for a script that cannot write it, and the shift a script
//! reads, taken from the cycle's own chain.
//!
//! Provenance: our own engine law. The cycle's shape is the published G76
//! fine boring canned cycle (Haas Mill Operator's Manual, G76, words I and
//! J): feed in, dwell, oriented spindle stop, shift, rapid out.

use super::cnc::{CycleRec, MoveRec, Record};
use crate::cl::MoveKind;
use serde_json::{json, Value};
use std::collections::BTreeMap;

/// Two stream numbers are the same point (the stream carries micrometres).
const SAME: f64 = 1e-6;

/// The shift of a fine-bore cycle, read from its chain: the lateral rapid
/// at the bottom that follows the feed in, followed by the rapid out at the
/// shifted XY. `rest` is the stream after the cycle record. `None` when the
/// chain does not have that shape - the cycle must then be refused, because
/// a fine bore with no shift drags the tip up the finished wall.
pub(super) fn shift_of(c: &CycleRec, rest: &[Record]) -> Option<(f64, f64)> {
    let group: Vec<&MoveRec> = rest
        .iter()
        .take_while(|r| !matches!(r, Record::Cycle(_) | Record::Section { begin: false }))
        .filter_map(|r| match r {
            Record::Move(m) => Some(m),
            _ => None,
        })
        .collect();
    let [cut, shift, out, ..] = group.as_slice() else { return None };
    let on_axis = (cut.x - c.x).abs() < SAME && (cut.y - c.y).abs() < SAME;
    let at_bottom = |m: &MoveRec| (m.z - c.bottom_z).abs() < SAME;
    if cut.kind == MoveKind::Rapid || !on_axis || !at_bottom(cut) {
        return None;
    }
    if shift.kind != MoveKind::Rapid || !at_bottom(shift) || shift.center.is_some() {
        return None;
    }
    let (dx, dy) = (shift.x - c.x, shift.y - c.y);
    if !(dx.is_finite() && dy.is_finite()) || dx.hypot(dy) < SAME {
        return None;
    }
    // The rapid out runs at the shifted XY, up to the R plane.
    let lifted = out.kind == MoveKind::Rapid
        && (out.x - shift.x).abs() < SAME
        && (out.y - shift.y).abs() < SAME
        && (out.z - c.r_z).abs() < SAME;
    lifted.then_some((dx, dy))
}

/// Voice one fine-bore cycle record: refuse it by name for a script that
/// does not declare `fineBoring`, and give the script the shift as
/// `ShiftX` / `ShiftY` (null when the chain carries none, which is also a
/// refusal). Nothing is guessed: no default shift, no other cycle.
pub(super) fn voice(
    voiced: bool,
    c: &CycleRec,
    rest: &[Record],
    op: &str,
    props: &mut BTreeMap<String, Value>,
    warnings: &mut Vec<String>,
) {
    if !voiced {
        warnings.push(format!(
            "operation {op}: fine boring is not supported by this post (fineBoring is not declared) - \
             the cycle needs an oriented spindle stop and a shift off the wall, and this post has no \
             verified word for them; it is refused, not posted as another cycle"
        ));
    }
    let shift = shift_of(c, rest);
    if shift.is_none() {
        warnings.push(format!(
            "operation {op}: the fine bore at X{} Y{} has no shift move at the bottom of its chain - \
             refused: with no shift the stopped tip drags up the finished wall",
            c.x, c.y
        ));
    }
    props.insert("ShiftX".into(), json!(shift.map(|s| s.0)));
    props.insert("ShiftY".into(), json!(shift.map(|s| s.1)));
}
