//! #1556 pins: the boring ride clears every point of its hole's floor,
//! or refuses by name - never a core left that no pass touches.
//!
//! Provenance: our own law tests over the circular-interpolation bore
//! (`bore.rs`); the coverage check is plain point-to-segment distance.

use super::*;
use crate::cl::{MoveKind, Toolpath};
use crate::geo2d::Point2;

fn hole(radius: f64, through: bool) -> BoreHole {
    BoreHole {
        xy: Point2::new(10.0, 20.0),
        radius,
        top_z: 0.0,
        floor_z: -4.0,
        through,
        stepped: false,
    }
}

fn params(h: BoreHole, dia: f64, leave: f64) -> BoreParams {
    params_c(h, dia, 0.0, leave)
}

fn params_c(h: BoreHole, dia: f64, corner: f64, leave: f64) -> BoreParams {
    BoreParams {
        links: crate::links::Links::PLANAR,
        holes: vec![h],
        tool_diameter: dia,
        corner_radius: corner,
        stock_to_leave: leave,
        stepdown: 2.0,
        through_allowance: 1.0,
        clearance_z: 5.0,
        retract_z: None,
        feed: 800.0,
        plunge_feed: 200.0,
        climb: true,
        tolerance: 0.01,
        entry: crate::entry::Entry3D::Plunge,
        helix_radius: 0.0,
    }
}

fn seg_dist(p: Point2, a: Point2, b: Point2) -> f64 {
    let (dx, dy) = (b.x - a.x, b.y - a.y);
    let len2 = dx * dx + dy * dy;
    let t = if len2 > 0.0 { (((p.x - a.x) * dx + (p.y - a.y) * dy) / len2).clamp(0.0, 1.0) } else { 0.0 };
    let (qx, qy) = (a.x + t * dx, a.y + t * dy);
    ((p.x - qx).powi(2) + (p.y - qy).powi(2)).sqrt()
}

/// The farthest floor point (inside the finished wall, short by `inset`)
/// from every cut move at the deepest level, minus the tool radius:
/// positive means a point no pass touches.
fn worst_uncut(path: &Toolpath, h: &BoreHole, r: f64, leave: f64, inset: f64) -> f64 {
    worst_uncut_within(path, h, r, h.radius - leave - inset)
}

/// [`worst_uncut`] over the floor disc of radius `reach` about the hole's
/// centre (a corner-radiused cutter's own wall fillet stands outside the
/// ring's floor-plane band and is joined to the wall, so it is no core).
fn worst_uncut_within(path: &Toolpath, h: &BoreHole, r: f64, reach: f64) -> f64 {
    let floor = path
        .moves
        .iter()
        .filter(|m| m.kind != MoveKind::Rapid)
        .map(|m| m.to.z)
        .fold(f64::INFINITY, f64::min);
    let segs: Vec<(Point2, Point2)> = path
        .moves
        .iter()
        .filter(|m| m.kind != MoveKind::Rapid && (m.from.z - floor).abs() < 1e-9 && (m.to.z - floor).abs() < 1e-9)
        // Arcs walked as fine chords (inscribed: a chord stands inside
        // its arc, so the check can only under-credit the ride).
        .flat_map(|m| m.chords(1e-4))
        .collect();
    assert!(!segs.is_empty(), "the floor level cuts");
    let mut worst = f64::NEG_INFINITY;
    let n = 60;
    for i in 0..=n {
        for j in 0..=n {
            let x = -reach + 2.0 * reach * i as f64 / n as f64;
            let y = -reach + 2.0 * reach * j as f64 / n as f64;
            if x * x + y * y > reach * reach {
                continue;
            }
            let p = Point2::new(h.xy.x + x, h.xy.y + y);
            let d = segs.iter().map(|(a, b)| seg_dist(p, *a, *b)).fold(f64::INFINITY, f64::min);
            worst = worst.max(d - r);
        }
    }
    worst
}

#[test]
fn bore_ring_budget_refuses_extreme_holes_by_name() {
    let limit = max_bore_diameter(4.0, 0.0, 0.5, None);
    assert_eq!(limit, 4.0 + 1.0 + 2.0 * 4096.0 * 1.6);
    let err = bore(&params(hole(limit * 0.5 + 0.01, false), 4.0, 0.5))
        .expect_err("more than 4096 rings is refused before allocation");
    let msg = format!("{err:?}");
    assert!(msg.contains("4096"), "the refusal names its ring budget: {msg}");
}

#[test]
fn every_bore_the_ride_takes_clears_its_whole_floor() {
    // Below the limit, no floor point is left: the dive's disc and the
    // ring's band meet or overlap, whatever the ratio.
    for (radius, dia) in [(4.0, 6.0), (5.0, 4.0), (6.0, 4.0), (8.9, 6.0), (2.6, 4.0)] {
        let h = hole(radius, false);
        let path = bore(&params(h, dia, 0.0)).expect("a hole within 3 tool diameters bores");
        let gap = worst_uncut(&path, &h, dia * 0.5, 0.0, 1e-3);
        assert!(gap <= 1e-3, "{}-mm hole, {dia} mm tool: a floor point {gap} mm past every pass", radius * 2.0);
    }
}


#[test]
fn the_cutting_radius_follows_the_corner_torus() {
    // r = 6, corner 2: the flat bottom is R4; at 1 mm up the corner adds
    // sqrt(2^2 - 1^2) = sqrt(3); from 2 mm up it is the full R6.
    assert!((cutting_radius_at(6.0, 2.0, 1.0) - (4.0 + 3f64.sqrt())).abs() < 1e-12);
    assert_eq!(cutting_radius_at(6.0, 2.0, 0.0), 4.0);
    assert_eq!(cutting_radius_at(6.0, 2.0, 2.0), 6.0);
    assert_eq!(cutting_radius_at(6.0, 2.0, 7.0), 6.0);
    // A ball (corner = r): no flat; 1 mm up a R3 ball cuts sqrt(1 x 5).
    assert!((cutting_radius_at(3.0, 3.0, 1.0) - 5f64.sqrt()).abs() < 1e-12);
    assert_eq!(cutting_radius_at(3.0, 3.0, 0.0), 0.0);
    // A ball smaller than the drop reaches its full radius.
    assert_eq!(cutting_radius_at(0.5, 0.5, 1.0), 0.5);
    // A square end mill is its radius at every height.
    assert_eq!(cutting_radius_at(2.0, 0.0, 0.0), 2.0);
    // Through: the floor plane; blind: the flat bottom.
    assert!((bore_floor_reach(12.0, 2.0, Some(1.0)) - (4.0 + 3f64.sqrt())).abs() < 1e-12);
    assert_eq!(bore_floor_reach(12.0, 2.0, None), 4.0);
}

#[test]
fn wide_bull_nose_bores_clear_the_floor_without_an_interior_ridge() {
    for (radius, through) in [(18.0, false), (25.0, false), (25.0, true)] {
        let h = hole(radius, through);
        let reach = if through { 4.0 + 3f64.sqrt() } else { 4.0 };
        let path = bore(&params_c(h, 12.0, 2.0, 0.0)).expect("wide bull nose bores clear rings");
        // The wall fillet is intentional. The flat floor inside it must clear.
        let floor_radius = h.radius - 6.0 + reach - 1e-3;
        let gap = worst_uncut_within(&path, &h, reach, floor_radius);
        assert!(gap <= 1e-3, "through={through}: an interior floor ridge remains ({gap})");
    }
}

#[test]
fn ball_bore_needs_positive_floor_reach_and_corner_must_fit() {
    let path = bore(&params_c(hole(12.0, true), 6.0, 3.0, 0.0))
        .expect("a through ball clears with its positive floor reach");
    assert!(worst_uncut_within(&path, &hole(12.0, true), 5f64.sqrt(), 9.0 + 5f64.sqrt() - 1e-3) <= 1e-3);
    let err = bore(&params_c(hole(12.0, false), 6.0, 3.0, 0.0))
        .expect_err("a ball cannot clear a flat blind floor");
    assert!(format!("{err:?}").contains("floor"), "{err:?}");
    let err = bore(&params_c(hole(5.0, false), 6.0, 3.5, 0.0))
        .expect_err("a 3.5 corner does not fit a 6 mm cutter");
    assert!(format!("{err:?}").contains("corner radius does not fit"), "{err:?}");
}

#[test]
fn wide_bore_rings_clear_the_whole_floor() {
    for (radius, dia, leave) in [(10.0, 6.0, 0.0), (15.0, 4.0, 0.5), (37.15, 4.0, 0.0)] {
        for through in [false, true] {
            for climb in [false, true] {
                let h = hole(radius, through);
                let mut p = params(h, dia, leave);
                p.climb = climb;
                let path = bore(&p).expect("wide bores clear concentric floor bands");
                let gap = worst_uncut(&path, &h, dia * 0.5, leave, 1e-3);
                assert!(gap <= 1e-3, "radius={radius}, tool={dia}, leave={leave}, through={through}, climb={climb}: uncut {gap}");
            }
        }
    }
}

#[test]
fn bore_ring_count_enforces_its_allocation_budget() {
    let limit = max_bore_diameter(4.0, 0.0, 0.5, None);
    assert_eq!(super::bore_core::bore_ring_count(limit * 0.5, 4.0, 0.0, 0.5, None), Some(4096));
    assert_eq!(super::bore_core::bore_ring_count(limit * 0.5 + 0.01, 4.0, 0.0, 0.5, None), None);
    assert_eq!(super::bore_core::bore_ring_count(f64::INFINITY, 4.0, 0.0, 0.0, None), None);
}
