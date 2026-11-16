//! Near-miss and refusal pins of the slot fit law (#1504 review): each
//! tolerance is pinned from both sides (0.02 mm out refused, 0.005 mm
//! out accepted) on the edge room and the face room, each refusal has a
//! pin that turns red when its line is dropped, and a tessellated
//! round-ended face is inscribed, never refused for its chord sag.

use super::*;
use crate::geo2d::Loop;

fn p(x: f64, y: f64) -> Point2 {
    Point2::new(x, y)
}

/// The two long floor edges of a 30 x 10 pocket centred on the origin:
/// x in [-15, 15], y in [-5, 5].
fn edges() -> SlotBound {
    SlotBound::from_edges(&[p(-15.0, -5.0), p(15.0, -5.0)], &[p(15.0, 5.0), p(-15.0, 5.0)]).expect("bound")
}

fn rect(x0: f64, y0: f64, x1: f64, y1: f64) -> Region {
    Region::new(vec![Loop::new(vec![p(x0, y0), p(x1, y0), p(x1, y1), p(x0, y1)]).expect("loop")]).expect("region")
}

/// A 10 x 30 face standing along Y: x in [10, 20], y in [-5, 25].
fn face() -> SlotBound {
    SlotBound::from_face(rect(10.0, -5.0, 20.0, 25.0)).expect("bound")
}

/// A stadium 2a long and 2r wide on the origin, each round end of
/// `seg` chords per quarter - a tessellated round-ended slot face.
fn stadium(a: f64, r: f64, seg: usize) -> Region {
    let mut pts = Vec::new();
    for (cx, from) in [(a - r, -0.5), (-(a - r), 0.5)] {
        for k in 0..=2 * seg {
            let t = std::f64::consts::PI * (from + k as f64 / (2 * seg) as f64);
            pts.push(p(cx + r * t.cos(), r * t.sin()));
        }
    }
    Region::new(vec![Loop::new(pts).expect("loop")]).expect("region")
}

fn refused(b: &SlotBound, a: Point2, c: Point2, w: f64) -> String {
    let e = b.confine(a, c, w).expect_err("refused").to_string();
    assert!(e.contains("does not fit the selection"), "{e}");
    e
}

#[test]
fn the_edge_room_refuses_0_02_out_and_takes_0_005_out() {
    let b = edges();
    // Too wide.
    let e = refused(&b, p(-10.0, 0.0), p(10.0, 0.0), 10.02);
    assert!(e.contains("10.020 mm wide"), "{e}");
    b.confine(p(-10.0, 0.0), p(10.0, 0.0), 10.005).expect("0.005 wide is in tolerance");
    // Too long.
    let e = refused(&b, p(-12.01, 0.0), p(12.01, 0.0), 6.0);
    assert!(e.contains("30.020 mm long end to end"), "{e}");
    b.confine(p(-12.0025, 0.0), p(12.0025, 0.0), 6.0).expect("0.005 long is in tolerance");
    // Past the picks across the slot.
    let e = refused(&b, p(-5.0, 2.02), p(5.0, 2.02), 6.0);
    assert!(e.contains("runs 0.020 mm outside the picked edges"), "{e}");
    b.confine(p(-5.0, 2.005), p(5.0, 2.005), 6.0).expect("0.005 across is in tolerance");
    // Past the picks along the slot.
    let e = refused(&b, p(-11.98, 0.0), p(12.02, 0.0), 6.0);
    assert!(e.contains("runs 0.020 mm outside the picked edges"), "{e}");
    b.confine(p(-11.995, 0.0), p(12.005, 0.0), 6.0).expect("0.005 along is in tolerance");
}

#[test]
fn the_face_room_refuses_0_02_out_and_takes_0_005_out() {
    let b = face();
    let e = refused(&b, p(15.0, 0.0), p(15.0, 20.0), 10.02);
    assert!(e.contains("10.020 mm wide"), "{e}");
    b.confine(p(15.0, 0.0), p(15.0, 20.0), 10.005).expect("0.005 wide is in tolerance");
    let e = refused(&b, p(15.0, -2.01), p(15.0, 22.01), 6.0);
    assert!(e.contains("30.020 mm long end to end"), "{e}");
    b.confine(p(15.0, -2.0025), p(15.0, 22.0025), 6.0).expect("0.005 long is in tolerance");
    let e = refused(&b, p(17.02, 0.0), p(17.02, 20.0), 6.0);
    assert!(e.contains("runs 0.020 mm outside the picked face"), "{e}");
    b.confine(p(17.005, 0.0), p(17.005, 20.0), 6.0).expect("0.005 across is in tolerance");
    let e = refused(&b, p(15.0, -1.98), p(15.0, 22.02), 6.0);
    assert!(e.contains("runs 0.020 mm outside the picked face"), "{e}");
    b.confine(p(15.0, -1.995), p(15.0, 22.005), 6.0).expect("0.005 along is in tolerance");
}

#[test]
fn an_edge_bowing_0_02_is_refused_and_0_005_taken() {
    let second = [p(0.0, 5.0), p(10.0, 5.0)];
    let e = SlotBound::from_edges(&[p(0.0, 0.0), p(5.0, 0.02), p(10.0, 0.0)], &second).expect_err("bows 0.02").to_string();
    assert!(e.contains("first picked edge is not straight (it bows 0.020 mm"), "{e}");
    SlotBound::from_edges(&[p(0.0, 0.0), p(5.0, 0.005), p(10.0, 0.0)], &second).expect("bows 0.005");
    let first = [p(0.0, 0.0), p(10.0, 0.0)];
    let e = SlotBound::from_edges(&first, &[p(0.0, 5.0), p(10.0, 5.02)]).expect_err("wanders 0.02").to_string();
    assert!(e.contains("wanders 0.020 mm across the slot"), "{e}");
    SlotBound::from_edges(&first, &[p(0.0, 5.0), p(10.0, 5.005)]).expect("wanders 0.005");
}

#[test]
fn the_face_wall_law_refuses_a_slot_shifted_into_the_side() {
    // 1 mm sideways: both ends stay inside the face, the side runs
    // through its wall - only the wall law (region edges against the
    // slot's centre segment) sees it.
    let e = refused(&face(), p(16.0, 0.0), p(16.0, 20.0), 10.0);
    assert!(e.contains("runs 1.000 mm outside the picked face"), "{e}");
}

#[test]
fn the_face_end_law_refuses_a_slot_wholly_past_the_face() {
    // Beside the face, touching no wall: only the end law sees it.
    let e = refused(&face(), p(40.0, 0.0), p(40.0, 20.0), 6.0);
    assert!(e.contains("runs 23.000 mm outside the picked face"), "{e}");
    // In a hole of the face, touching no wall.
    let ring = Region::new(vec![
        Loop::new(vec![p(0.0, 0.0), p(60.0, 0.0), p(60.0, 30.0), p(0.0, 30.0)]).expect("outer"),
        Loop::new(vec![p(10.0, 10.0), p(10.0, 20.0), p(50.0, 20.0), p(50.0, 10.0)]).expect("hole"),
    ])
    .expect("region");
    let e = refused(&SlotBound::from_face(ring).expect("bound"), p(20.0, 15.0), p(40.0, 15.0), 4.0);
    assert!(e.contains("outside the picked face"), "{e}");
}

#[test]
fn a_tessellated_round_ended_face_is_inscribed_not_refused() {
    // 30 x 16, r 8, 8 chords per quarter: the chords sag 0.039 mm.
    let b = SlotBound::from_face(stadium(15.0, 8.0, 8)).expect("bound");
    let f = b.fit().expect("the stadium face seeds its slot");
    let sag = 8.0 * (1.0 - (std::f64::consts::PI / 32.0).cos());
    assert!((f.width - (16.0 - 2.0 * sag)).abs() < 1e-6, "inscribed by the sag: {f:?}");
    let (lo, hi) = if f.a.x < f.b.x { (f.a, f.b) } else { (f.b, f.a) };
    assert!((lo.x + 7.0).abs() < 1e-6 && (hi.x - 7.0).abs() < 1e-6 && lo.y.abs() < 1e-6, "{f:?}");
    b.confine(f.a, f.b, f.width).expect("the inscribed fit fits");
    // The uninscribed slot is the one the chords refuse - the tolerance
    // is never loosened into the model.
    let e = refused(&b, f.a, f.b, 16.0);
    assert!(e.contains("runs 0.039 mm outside the picked face"), "{e}");
}

#[test]
fn a_face_coarser_than_the_inscribe_limit_seeds_nothing_and_says_why() {
    // One chord per quarter: the ends are points, the chords 2.343 mm
    // inside the arc - no slot face; it is refused, and a typed slot
    // judged against it names why the picks seeded nothing.
    let b = SlotBound::from_face(stadium(30.0, 8.0, 1)).expect("bound");
    let e = b.fit().expect_err("no slot").to_string();
    assert!(e.contains("holds no straight slot") && e.contains("runs 2.343 mm outside"), "{e}");
    let e = b.judge(p(-27.0, 0.0), p(27.0, 0.0), 6.0).expect_err("refused").to_string();
    assert!(e.contains("runs 0.879 mm outside the picked face") && e.contains("the picks seed no slot: the picked face holds no straight slot"), "{e}");
    // A fitting room judges a slot that fails without the extra clause.
    let e = edges().judge(p(-20.0, 0.0), p(20.0, 0.0), 6.0).expect_err("too long").to_string();
    assert!(!e.contains("seed no slot"), "{e}");
}

#[test]
fn degenerate_picks_and_numbers_are_refused_by_name() {
    let e = SlotBound::from_face(rect(0.0, 0.0, 0.009, 0.009)).expect_err("no area").to_string();
    assert!(e.contains("bounds no area in XY"), "{e}");
    let e = SlotBound::from_face(rect(0.0, 0.0, 30.0, 0.008)).expect("bound").fit().expect_err("no width").to_string();
    assert!(e.contains("the picked face leaves no width for a slot"), "{e}");
    let e = SlotBound::from_edges(&[p(0.0, 0.0), p(0.005, 0.0)], &[p(0.0, 5.0), p(10.0, 5.0)]).expect_err("no length").to_string();
    assert!(e.contains("first picked edge has no length"), "{e}");
    let b = edges();
    for (a, w) in [(p(-5.0, 0.0), f64::NAN), (p(-5.0, 0.0), 0.0), (p(-5.0, 0.0), -2.0), (p(-5.0, 0.0), f64::INFINITY), (p(f64::NAN, 0.0), 4.0)] {
        let e = b.confine(a, p(5.0, 0.0), w).expect_err("not finite").to_string();
        assert!(e.contains("must be finite, the width positive"), "{a:?} {w}: {e}");
    }
}

#[test]
fn the_face_wall_law_refuses_a_thin_wall_across_the_slot() {
    // #1504 review: a 0.1 mm slit hole across the slot, every vertex of it
    // farther than r from the centre line - only the segment crossing
    // test (`seg_seg_dist`) sees it.
    let reg = Region::new(vec![
        Loop::new(vec![p(10.0, -5.0), p(20.0, -5.0), p(20.0, 25.0), p(10.0, 25.0)]).expect("outer"),
        Loop::new(vec![p(10.5, 10.22), p(10.5, 10.32), p(19.5, 10.32), p(19.5, 10.22)]).expect("slit"),
    ])
    .expect("region");
    let e = refused(&SlotBound::from_face(reg).expect("bound"), p(15.0, 0.0), p(15.0, 20.0), 6.0);
    assert!(e.contains("runs 3.000 mm outside the picked face"), "{e}");
}
