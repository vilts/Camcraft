//! The no-core burnout's laws (#540): a pocket burns out with no core
//! left (checked independently of the generator's own law), a clearance
//! under the kerf, a missing or misplaced start hole, a punch side and
//! a split pattern are refused by name, and a pattern that leaves a
//! core is caught by the no-core law. The wire and gap are SYNTHETIC
//! (#290 D4, D10).

use super::super::ring::pt_ring;
use super::*;
use crate::cl::MoveKind;

const K: f64 = 0.165;

fn params(side: WireSide, start: Option<(f64, f64)>) -> WireParams {
    WireParams {
        wire_diameter: 0.25,
        gap: 0.04,
        condition: "C110".into(),
        side,
        start: start.map(|(x, y)| Point2::new(x, y)),
        plane_z: 0.0,
        stock: None,
        clearance: 1.0,
        tabs: None,
    }
}

fn rect(x0: f64, y0: f64, x1: f64, y1: f64) -> Loop {
    Loop::new(vec![Point2::new(x0, y0), Point2::new(x1, y0), Point2::new(x1, y1), Point2::new(x0, y1)]).unwrap()
}

fn refusal(l: &[Loop], p: &WireParams, c: f64) -> String {
    wire_burnout(l, &[], p, c).unwrap_err().to_string()
}

fn polyline(path: &Toolpath) -> Vec<Point2> {
    let mut v = vec![Point2::new(path.moves[0].from.x, path.moves[0].from.y)];
    v.extend(path.moves.iter().map(|m| Point2::new(m.to.x, m.to.y)));
    v
}

/// Done means: a pocket burns out with no core region left - read by
/// connectivity, apart from the generator's own law: what the kerf
/// leaves of the pocket all stands against the walls.
#[test]
fn a_pocket_burns_out_with_no_core_left() {
    let pocket = rect(-6.0, -3.0, 6.0, 3.0);
    let c = 0.5;
    let path = wire_burnout(std::slice::from_ref(&pocket), &[], &params(WireSide::Die, Some((1.0, 0.5))), c).unwrap();
    assert!(path.moves.iter().all(|m| m.kind == MoveKind::Feed && m.axis.is_none() && m.arc.is_none()));
    let pts = polyline(&path);
    assert_eq!((pts[0], *pts.last().unwrap()), (Point2::new(1.0, 0.5), Point2::new(1.0, 0.5)), "from and back to the start hole");
    let walls = pocket.points().to_vec();
    assert!(pts.iter().all(|q| pt_ring(*q, &walls) >= c - 1e-5), "every pass keeps the clearance to the walls");
    let region = Region::new(vec![pocket]).unwrap();
    let left = region.difference(&Region::sweep(&[pts], K).unwrap()).unwrap();
    assert!(left.area() > 0.0, "the clearance leaves a skin on the walls");
    // Connectivity: every piece left (each outer loop) touches a wall,
    // so it is held by the plate - none stands free in the pocket.
    for l in left.loops().iter().filter(|l| l.area() > 0.0) {
        let touch = l.points().iter().map(|q| pt_ring(*q, &walls)).fold(f64::INFINITY, f64::min);
        assert!(touch < 1e-3, "a piece of {:.4} mm² stands {touch:.4} mm off the walls - a core", l.area());
    }
    assert!(path.notes[0].contains("no core") && path.notes[0].contains("innermost first"), "{:?}", path.notes);
}

/// At exactly the kerf offset the outermost ring is the finished wall,
/// so a sharp corner is refused by the corner minimum; a rounded pocket
/// burns out to size.
#[test]
fn at_the_kerf_offset_the_outer_ring_is_the_wall() {
    let sharp = refusal(&[rect(-5.0, -5.0, 5.0, 5.0)], &params(WireSide::Die, Some((0.0, 0.0))), K);
    assert!(sharp.contains("inside corner") && sharp.contains("0.1650"), "{sharp}");
    let mut pts = Vec::new();
    for (cx, cy, a0) in [(4.0, 4.0, 0.0_f64), (-4.0, 4.0, 90.0), (-4.0, -4.0, 180.0), (4.0, -4.0, 270.0)] {
        for s in 0..=30 {
            let t = (a0 + 3.0 * f64::from(s)).to_radians();
            pts.push(Point2::new(cx + t.cos(), cy + t.sin()));
        }
    }
    let path = wire_burnout(&[Loop::new(pts).unwrap()], &[], &params(WireSide::Die, Some((0.0, 0.0))), K).unwrap();
    assert!(path.moves.len() > 20, "{}", path.moves.len());
}

/// Done means: a clearance below the kerf is refused by name - at the
/// door as in the generator.
#[test]
fn a_clearance_below_the_kerf_is_refused_by_name() {
    let why = refusal(&[rect(-6.0, -3.0, 6.0, 3.0)], &params(WireSide::Die, Some((0.0, 0.0))), 0.1);
    assert!(why.contains("wall clearance 0.1000 mm is below the kerf offset 0.1650 mm"), "{why}");
    assert!(clearance_door(f64::NAN, K).unwrap_err().contains("not a finite number"));
    assert!(clearance_door(K, K).is_ok());
}

#[test]
fn a_missing_or_misplaced_start_hole_and_a_punch_are_refused_by_name() {
    let pocket = [rect(-6.0, -3.0, 6.0, 3.0)];
    let none = refusal(&pocket, &params(WireSide::Die, None), 0.5);
    assert!(none.contains("no start hole is stated"), "{none}");
    let near = refusal(&pocket, &params(WireSide::Die, Some((5.8, 0.0))), 0.5);
    assert!(near.contains("outside the burnout's reach"), "{near}");
    let punch = refusal(&pocket, &params(WireSide::Punch, Some((0.0, 0.0))), 0.5);
    assert!(punch.contains("die-side cut") && punch.contains("punch"), "{punch}");
    let tiny = refusal(&[rect(0.0, 0.0, 0.5, 0.5)], &params(WireSide::Die, Some((0.25, 0.25))), 0.5);
    assert!(tiny.contains("too small for the wire"), "{tiny}");
    // Tabs on a burnout (#539): refused by name, never silently dropped.
    let tabs = super::super::TabParams { count: Some(1), width: Some(1.0), at: vec![Point2::new(6.0, 0.0)], cutoff: None };
    let tabbed = refusal(&pocket, &WireParams { tabs: Some(tabs), ..params(WireSide::Die, Some((0.0, 0.0))) }, 0.5);
    assert!(tabbed.contains("tabs are stated on a burnout"), "{tabbed}");
}

/// A pattern that cannot reach every interior point is refused: a neck
/// splits the rings into islands, and a path that leaves a core is
/// caught by the no-core law.
#[test]
fn a_pattern_that_cannot_reach_every_point_is_refused_by_name() {
    // Two 10 x 10 rooms joined by a 0.8 mm neck.
    let dumbbell = Loop::new(
        [(0.0, 0.0), (10.0, 0.0), (10.0, 4.6), (12.0, 4.6), (12.0, 0.0), (22.0, 0.0), (22.0, 10.0), (12.0, 10.0), (12.0, 5.4), (10.0, 5.4), (10.0, 10.0), (0.0, 10.0)]
            .iter()
            .map(|&(x, y)| Point2::new(x, y))
            .collect(),
    )
    .unwrap();
    let neck = refusal(&[dumbbell], &params(WireSide::Die, Some((5.0, 5.0))), 0.2);
    assert!(neck.contains("splits into 2 islands"), "{neck}");
    // One ring alone around a 10 x 10 pocket: the middle drops.
    let pocket = rect(0.0, 0.0, 10.0, 10.0);
    let ring = Region::new(vec![pocket.clone()]).unwrap().offset_sharp(-0.5).unwrap().loops()[0].points().to_vec();
    let mut b = PathBuilder::new(Point3::new(ring[0].x, ring[0].y, 0.0));
    for q in ring.iter().skip(1).chain(ring.first()) {
        b.push(MoveKind::Feed, Point3::new(q.x, q.y, 0.0), None);
    }
    let why = no_core(&Region::new(vec![pocket]).unwrap(), &b.finish(), K, 0.0).unwrap_err().to_string();
    assert!(why.contains("leaves a core of") && why.contains("drop free"), "{why}");
}
