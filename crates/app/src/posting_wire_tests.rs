//! #1164 review (PR #1218): the post-side angle-word refusals of
//! `posting_wire`, on hand-built records and rules - the paths the
//! generation door and the stamp keep out of a real post.
//! Gate map (#1164): covers app::posting (wire: angle_lean, angled).
//! Provenance: our own tests over synthetic records.

use super::*;
use crate::cl::{axis_to_angles, ArcPlane, MoveKind};
use crate::machine::{AngleSide, TaperAngle, TaperCorner};

/// A posed feed at (x, y, 0) leaning toward `axis` (not normalised).
fn posed(x: f64, y: f64, axis: [f64; 3]) -> cnc::Record {
    let l = (axis[0] * axis[0] + axis[1] * axis[1] + axis[2] * axis[2]).sqrt();
    cnc::Record::Move(cnc::MoveRec {
        kind: MoveKind::Feed,
        x,
        y,
        z: 0.0,
        feed: Some(100.0),
        center: None,
        ccw: false,
        plane: ArcPlane::Xy,
        abc: Some(axis_to_angles([axis[0] / l, axis[1] / l, axis[2] / l])),
    })
}

#[test]
fn a_lean_that_changes_side_or_never_leans_is_refused_by_name() {
    let (left, right, up) = ([0.0, 0.1, 1.0], [0.0, -0.1, 1.0], [0.0, 0.0, 1.0]);
    let one_side = vec![posed(0.0, 0.0, left), posed(10.0, 0.0, left), posed(20.0, 0.0, left)];
    assert!(angled(one_side, 10.0, 3.0, true).is_ok(), "one side leans: the control");
    let flips = vec![posed(0.0, 0.0, left), posed(10.0, 0.0, left), posed(20.0, 0.0, right)];
    let err = angled(flips, 10.0, 3.0, true).err().unwrap_or_default();
    assert!(err.contains("the wire's lean changes side along the cut"), "{err}");
    let upright = vec![posed(0.0, 0.0, up), posed(10.0, 0.0, up), posed(20.0, 0.0, up)];
    let err = angled(upright, 10.0, 3.0, true).err().unwrap_or_default();
    assert!(err.contains("a tapered cut with no leaning side"), "{err}");
}

#[test]
fn an_angle_lean_refuses_a_four_axis_cut_an_unstated_rule_and_an_unresolved_top() {
    let full = TaperAngle { positive: Some(AngleSide::TopLeft), corner: Some(TaperCorner::Mitre) };
    let err = |r: Result<Lean, String>| r.err().unwrap_or_default();
    assert!(angle_lean("W1", "M", full, false, Some(12.0), 2.0).is_ok_and(|l| matches!(l, Lean::Angle { left: true, plane, .. } if plane == 10.0)));
    let four = err(angle_lean("W1", "M", full, true, Some(12.0), 0.0));
    assert!(four.starts_with("W1: a 4-axis cut, and machine M tapers by angle word"), "{four}");
    let unstated = err(angle_lean("W1", "M", TaperAngle { corner: None, ..full }, false, Some(12.0), 0.0));
    assert!(unstated.contains("states no edm.taper_angle.corner"), "{unstated}");
    let no_top = err(angle_lean("W1", "M", full, false, None, 0.0));
    assert!(no_top.contains("the workpiece's top does not resolve"), "{no_top}");
}
