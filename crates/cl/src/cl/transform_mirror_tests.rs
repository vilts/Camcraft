//! The vertical-plane mirror's laws (#3011): G2 <-> G3 and G41 <-> G42
//! under `GeometricMirror`, a safe pass reversal under `PreserveClimb`,
//! and the planes, poses and paths it refuses by name.

use super::tests::{contour, near, p};
use super::*;
use crate::cl::{CompSide, MoveKind, PathBuilder};

/// The plane X = 5: (x, y, z) -> (10 - x, y, z).
fn x5() -> MirrorPlane {
    MirrorPlane { point: p(5.0, 0.0, 0.0), normal: [1.0, 0.0, 0.0] }
}

/// The signed area of the loop the planar cutting moves walk (shoelace
/// over their chords): positive is counter-clockwise.
fn signed_area(t: &Toolpath) -> f64 {
    t.moves
        .iter()
        .filter(|m| m.kind == MoveKind::Feed && m.from.z == m.to.z && m.from.z == -2.0)
        .flat_map(|m| m.chords(0.001))
        .map(|(a, b)| a.x * b.y - b.x * a.y)
        .sum::<f64>()
        / 2.0
}

#[test]
fn geometric_mirror_swaps_g2_g3_and_g41_g42() {
    let src = contour();
    let out = mirror_plane(&src, x5(), MirrorMode::GeometricMirror).expect("a vertical plane mirrors");
    assert!(out.is_chained());
    for (a, b) in src.moves.iter().zip(&out.moves) {
        assert_eq!((b.from, b.to), (p(10.0 - a.from.x, a.from.y, a.from.z), p(10.0 - a.to.x, a.to.y, a.to.z)));
    }
    let arc = out.moves[5].arc.expect("the arc stays an arc");
    assert_eq!((arc.center.x, arc.center.y, arc.ccw), (0.0, 5.0, false), "G3 becomes G2");
    assert_eq!(out.comps[0].side, CompSide::Right, "G41 becomes G42");
    assert!(out.comps[0].is_sane(&out.moves));
    assert!(out.comps[0].programmed.iter().any(|m| m.arc.is_some_and(|a| !a.ccw)));
    assert!(signed_area(&src) > 0.0 && signed_area(&out) < 0.0, "the loop now runs the other way");
}

#[test]
fn geometric_mirror_on_a_diagonal_plane_is_micron_accurate() {
    // The plane through the origin with normal (1, 1, 0) swaps X and Y.
    let plane = MirrorPlane { point: p(0.0, 0.0, 0.0), normal: [1.0, 1.0, 0.0] };
    let out = mirror_plane(&contour(), plane, MirrorMode::GeometricMirror).expect("mirrors");
    for (a, b) in contour().moves.iter().zip(&out.moves) {
        assert!(near(b.from, p(-a.from.y, -a.from.x, a.from.z)) && near(b.to, p(-a.to.y, -a.to.x, a.to.z)));
    }
    let arc = out.moves[5].arc.expect("arc");
    assert_eq!((arc.center.x, arc.center.y, arc.ccw), (-5.0, -10.0, false));
}

#[test]
fn mirror_refuses_horizontal_and_tilted_planes() {
    let flat = MirrorPlane { point: p(0.0, 0.0, 0.0), normal: [0.0, 0.0, 1.0] };
    assert_eq!(mirror_plane(&contour(), flat, MirrorMode::GeometricMirror), Err(TransformError::HorizontalMirrorPlane));
    let leaning = MirrorPlane { point: p(0.0, 0.0, 0.0), normal: [1.0, 0.0, 1.0] };
    assert_eq!(mirror_plane(&contour(), leaning, MirrorMode::GeometricMirror), Err(TransformError::TiltedMirrorPlane));
    let none = MirrorPlane { point: p(0.0, 0.0, 0.0), normal: [0.0, 0.0, 0.0] };
    assert!(matches!(mirror_plane(&contour(), none, MirrorMode::GeometricMirror), Err(TransformError::BadInput(_))));
}

#[test]
fn mirror_refuses_tilted_moves() {
    let mut t = contour();
    t.moves[4] = t.moves[4].with_axis([0.0, 0.5, 1.0]);
    assert_eq!(mirror_plane(&t, x5(), MirrorMode::GeometricMirror), Err(TransformError::TiltedMove));
}

#[test]
fn preserve_climb_reverses_the_pass_between_entry_and_retract() {
    let src = contour();
    let out = mirror_plane(&src, x5(), MirrorMode::PreserveClimb(PassFamily::Contour)).expect("a contour reverses");
    assert!(out.is_chained());
    assert_eq!(out.moves.len(), src.moves.len());
    // The approach now ends over the mirrored lead-out end, (10, -5).
    assert_eq!((out.moves[0].from, out.moves[0].to), (p(30.0, -20.0, 20.0), p(10.0, -5.0, 20.0)));
    // Same entry column as before - rapid to 5, plunge at 100 - at the new start.
    assert_eq!((out.moves[1].kind, out.moves[1].to), (MoveKind::Rapid, p(10.0, -5.0, 5.0)));
    assert_eq!((out.moves[2].kind, out.moves[2].feed, out.moves[2].to), (MoveKind::Plunge, Some(100.0), p(10.0, -5.0, -2.0)));
    // The lead-out, walked backwards, is now the lead-in.
    assert_eq!((out.moves[3].from, out.moves[3].to), (p(10.0, -5.0, -2.0), p(10.0, 0.0, -2.0)));
    assert_eq!((out.moves[8].from, out.moves[8].to), (p(10.0, 0.0, -2.0), p(15.0, 0.0, -2.0)));
    // Same exit column as before, at the mirrored lead-in start.
    assert_eq!((out.moves[9].kind, out.moves[9].to), (MoveKind::Feed, p(15.0, 0.0, 5.0)));
    assert_eq!((out.moves[10].kind, out.moves[10].to), (MoveKind::Rapid, p(15.0, 0.0, 20.0)));
    assert_eq!(out.end(), Some(p(30.0, -20.0, 20.0)));
    // Mirror and reversal cancel: the arc keeps G3, the span keeps G41.
    let arc = out.moves.iter().find_map(|m| m.arc).expect("arc");
    assert!(arc.ccw);
    assert_eq!((arc.center.x, arc.center.y), (0.0, 5.0));
    let c = &out.comps[0];
    assert_eq!((c.side, c.first, c.last), (CompSide::Left, 3, 8));
    assert!(c.is_sane(&out.moves));
    assert!(signed_area(&out) > 0.0, "the loop keeps its turning sense: the cut stays a climb cut");
}

#[test]
fn preserve_climb_refuses_a_helical_entry() {
    let mut b = PathBuilder::new(p(0.0, 0.0, 20.0));
    b.push(MoveKind::Rapid, p(5.0, 0.0, 20.0), None);
    b.push(MoveKind::Rapid, p(5.0, 0.0, 2.0), None);
    b.push_arc(MoveKind::Feed, p(-5.0, 0.0, 0.0), Some(300.0), Point2::new(0.0, 0.0), true);
    b.push_arc(MoveKind::Feed, p(5.0, 0.0, -2.0), Some(300.0), Point2::new(0.0, 0.0), true);
    b.push(MoveKind::Feed, p(5.0, 5.0, -2.0), Some(500.0));
    b.push(MoveKind::Rapid, p(5.0, 5.0, 20.0), None);
    let t = b.finish();
    let got = mirror_plane(&t, x5(), MirrorMode::PreserveClimb(PassFamily::Contour));
    assert!(matches!(got, Err(TransformError::NotReversible(ref why)) if why.contains("ramp or helical")), "{got:?}");
}

#[test]
fn preserve_climb_refuses_clearing_families_by_name() {
    for (family, word) in [
        (PassFamily::Adaptive, "adaptive"),
        (PassFamily::MultiPassRoughing, "roughing"),
        (PassFamily::HelicalRamp, "helical"),
    ] {
        let got = mirror_plane(&contour(), x5(), MirrorMode::PreserveClimb(family));
        assert!(matches!(got, Err(TransformError::NotReversible(ref why)) if why.contains(word)), "{got:?}");
    }
}

/// One open pass along Y at X = 0, depth `z`, from y0 to y1.
fn pass(b: &mut PathBuilder, z: f64, y0: f64, y1: f64) {
    b.push(MoveKind::Rapid, p(0.0, y0, 20.0), None);
    b.push(MoveKind::Rapid, p(0.0, y0, 2.0), None);
    b.push(MoveKind::Plunge, p(0.0, y0, z), Some(100.0));
    b.push(MoveKind::Feed, p(0.0, y1, z), Some(500.0));
    b.push(MoveKind::Feed, p(0.0, y1, 2.0), Some(100.0));
    b.push(MoveKind::Rapid, p(0.0, y1, 20.0), None);
}

#[test]
fn preserve_climb_reverses_two_depths_linked_on_the_clearance_plane() {
    let mut b = PathBuilder::new(p(-5.0, -5.0, 20.0));
    pass(&mut b, -1.0, 0.0, 10.0);
    pass(&mut b, -2.0, 0.0, 10.0);
    let src = b.finish();
    let out = mirror_plane(&src, x5(), MirrorMode::PreserveClimb(PassFamily::Profile)).expect("reverses");
    assert!(out.is_chained());
    let cuts: Vec<_> = out.moves.iter().filter(|m| m.kind == MoveKind::Feed && m.from.z == m.to.z).collect();
    assert_eq!(cuts.len(), 2);
    for (c, z) in cuts.iter().zip([-1.0, -2.0]) {
        assert_eq!((c.from, c.to), (p(10.0, 10.0, z), p(10.0, 0.0, z)), "each depth runs backwards, in order");
    }
    // Every link between the passes stays on the clearance plane Z20.
    for m in out.moves.iter().filter(|m| m.kind == MoveKind::Rapid && (m.from.x != m.to.x || m.from.y != m.to.y)) {
        assert!(m.from.z == 20.0 && m.to.z == 20.0, "{m:?}");
    }
}

#[test]
fn preserve_climb_refuses_a_cut_before_the_first_pass() {
    let mut b = PathBuilder::new(p(0.0, 0.0, 20.0));
    b.push(MoveKind::Feed, p(3.0, 0.0, 15.0), Some(500.0));
    b.push(MoveKind::Rapid, p(3.0, 0.0, 20.0), None);
    pass(&mut b, -1.0, 0.0, 10.0);
    let got = mirror_plane(&b.finish(), x5(), MirrorMode::PreserveClimb(PassFamily::Contour));
    assert!(matches!(got, Err(TransformError::NotReversible(ref why)) if why.contains("before")), "{got:?}");
}

#[test]
fn preserve_climb_refuses_a_level_change_without_a_retract() {
    let mut b = PathBuilder::new(p(0.0, 0.0, 20.0));
    b.push(MoveKind::Rapid, p(0.0, 0.0, 2.0), None);
    b.push(MoveKind::Plunge, p(0.0, 0.0, -1.0), Some(100.0));
    b.push(MoveKind::Feed, p(10.0, 0.0, -1.0), Some(500.0));
    b.push(MoveKind::Plunge, p(10.0, 0.0, -2.0), Some(100.0));
    b.push(MoveKind::Feed, p(0.0, 0.0, -2.0), Some(500.0));
    b.push(MoveKind::Rapid, p(0.0, 0.0, 20.0), None);
    let got = mirror_plane(&b.finish(), x5(), MirrorMode::PreserveClimb(PassFamily::Profile));
    assert!(matches!(got, Err(TransformError::NotReversible(_))), "{got:?}");
}

/// A pass at Z-2: a 5 mm lead-in from (-5, 0), the cut to (10, 0), then
/// a straight lead-out of `lead_out` mm (none at 0) and the lift.
fn led_pass(lead_out: f64) -> Toolpath {
    let mut b = PathBuilder::new(p(-5.0, 0.0, 20.0));
    b.push(MoveKind::Rapid, p(-5.0, 0.0, 2.0), None);
    b.push(MoveKind::Plunge, p(-5.0, 0.0, -2.0), Some(100.0));
    b.push(MoveKind::Feed, p(0.0, 0.0, -2.0), Some(500.0));
    b.push(MoveKind::Feed, p(10.0, 0.0, -2.0), Some(500.0));
    let end = 10.0 + lead_out;
    if lead_out > 0.0 {
        b.push(MoveKind::Feed, p(end, 0.0, -2.0), Some(500.0));
    }
    b.push(MoveKind::Feed, p(end, 0.0, 2.0), Some(100.0));
    b.push(MoveKind::Rapid, p(end, 0.0, 20.0), None);
    b.finish()
}

#[test]
fn preserve_climb_refuses_a_lead_in_without_a_lead_out() {
    // Reversed, the plunge would land at the old cut end (10, 0): on the
    // contour, at full depth.
    let got = mirror_plane(&led_pass(0.0), x5(), MirrorMode::PreserveClimb(PassFamily::Contour));
    assert!(matches!(got, Err(TransformError::NotReversible(ref why)) if why.contains("lead-out")), "{got:?}");
    // A lead-out shorter than the lead-in stands closer to the contour.
    let got = mirror_plane(&led_pass(2.0), x5(), MirrorMode::PreserveClimb(PassFamily::Contour));
    assert!(matches!(got, Err(TransformError::NotReversible(ref why)) if why.contains("lead-out")), "{got:?}");
    // A matching 5 mm lead-out: the reversed plunge is in the air at the
    // mirror of (15, 0).
    let out = mirror_plane(&led_pass(5.0), x5(), MirrorMode::PreserveClimb(PassFamily::Contour)).expect("reverses");
    let plunge = out.moves.iter().find(|m| m.kind == MoveKind::Plunge).map(|m| m.to);
    assert_eq!(plunge, Some(p(-5.0, 0.0, -2.0)));
}
