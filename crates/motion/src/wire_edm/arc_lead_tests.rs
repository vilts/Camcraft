//! The tangent arc lead (#3991): a die circle and a line-and-arc punch
//! entered and left on tangent arcs that meet the path in its own
//! direction, the laws clean; a hole too near and a lead that would cross
//! the profile refused by name; a lead moved 2 um into the product caught
//! by the kerf law; the straight lead unchanged. The wire and gap are
//! SYNTHETIC (#290 D4, D10).

use std::f64::consts::{FRAC_PI_2, TAU};

use super::super::arc_laws;
use super::super::arc_span::Span;
use super::super::*;
use super::{arc_leads, WireLead};
use crate::cl::{Move, Toolpath};

const K: f64 = 0.165;

fn params(side: WireSide, start: (f64, f64)) -> WireParams {
    WireParams {
        wire_diameter: 0.25,
        gap: 0.04,
        condition: "C110".into(),
        side,
        start: Some(Point2::new(start.0, start.1)),
        plane_z: -12.0,
        tabs: None,
        stock: None,
        clearance: 1.0,
        sharp_corners: false,
    }
}

fn p(x: f64, y: f64) -> Point2 {
    Point2::new(x, y)
}

fn circle(r: f64) -> ArcChain {
    let a = p(r, 0.0);
    ArcChain::new(vec![Span::Arc { c: p(0.0, 0.0), r, a, b: a, ccw: true, sweep: TAU }]).unwrap()
}

fn line(a: (f64, f64), b: (f64, f64)) -> Span {
    Span::Line { a: p(a.0, a.1), b: p(b.0, b.1) }
}

/// A 20 x 20 L with a radius 3 fillet at (20, 0) and an inside corner.
fn el() -> ArcChain {
    ArcChain::new(vec![
        line((0.0, 0.0), (17.0, 0.0)),
        Span::Arc { c: p(17.0, 3.0), r: 3.0, a: p(17.0, 0.0), b: p(20.0, 3.0), ccw: true, sweep: FRAC_PI_2 },
        line((20.0, 3.0), (20.0, 10.0)),
        line((20.0, 10.0), (10.0, 10.0)),
        line((10.0, 10.0), (10.0, 20.0)),
        line((10.0, 20.0), (0.0, 20.0)),
        line((0.0, 20.0), (0.0, 0.0)),
    ])
    .unwrap()
}

fn arc(r: f64) -> WireLead {
    WireLead::Arc { radius: r }
}

fn xy(q: crate::geometry::Point3) -> Point2 {
    p(q.x, q.y)
}

/// The unit walk direction of move `m` at its start (`at_end` false) or
/// its end.
fn dir(m: &Move, at_end: bool) -> Point2 {
    let (a, b) = (xy(m.from), xy(m.to));
    let v = match m.arc {
        None => p(b.x - a.x, b.y - a.y),
        Some(arc) => {
            let q = if at_end { b } else { a };
            let r = p(q.x - arc.center.x, q.y - arc.center.y);
            if arc.ccw { p(-r.y, r.x) } else { p(r.y, -r.x) }
        }
    };
    let l = v.x.hypot(v.y);
    p(v.x / l, v.y / l)
}

/// Each joint of the path is tangent within the lattice (1e-5 rad).
fn assert_tangent_joints(path: &Toolpath, from: usize, to: usize) {
    for i in from..to {
        let (a, b) = (dir(&path.moves[i], true), dir(&path.moves[i + 1], false));
        let turn = (a.x * b.y - a.y * b.x).abs();
        assert!(turn < 1e-5 && a.x * b.x + a.y * b.y > 0.0, "joint {i} is not tangent: {a:?} then {b:?}");
    }
}

#[test]
fn a_die_circle_is_entered_and_left_on_tangent_arcs() {
    let chain = circle(10.0);
    let prm = params(WireSide::Die, (0.0, 0.0));
    let path = wire_contour_arcs_led(&chain, &prm, arc(1.0)).unwrap();
    let m = &path.moves;
    // Straight, lead arc, two half circles, lead arc, straight.
    assert_eq!(m.len(), 6, "{m:?}");
    assert!(m[0].arc.is_none() && m[5].arc.is_none(), "the hole is left and reached by straight moves");
    let lead = m[1].arc.expect("the lead-in is a true arc");
    assert!(lead.ccw, "a die's lead turns to the waste side (left)");
    let entry = xy(m[1].to);
    assert!((entry.x - (10.0 - K)).abs() < 1e-9 && entry.y.abs() < 1e-9, "the entry stays the path's nearest point: {entry:?}");
    assert!(((lead.center.x - (9.0 - K)).abs()) < 1e-6 && lead.center.y.abs() < 1e-6, "radius 1 on the waste side");
    assert_eq!(xy(m[5].to), p(0.0, 0.0), "the lead-out ends at the threading point");
    assert_tangent_joints(&path, 0, m.len() - 1);
    arc_laws::kerf(&path, &chain, K, -12.0).unwrap();
    assert!(path.notes.iter().any(|n| n.contains("tangent arc lead-in of radius 1.0000 mm")), "{:?}", path.notes);
}

#[test]
fn a_line_and_arc_punch_is_entered_and_left_on_tangent_arcs() {
    let chain = el();
    // The L's inside corner keeps the wire radius (#3207).
    let prm = WireParams { sharp_corners: true, ..params(WireSide::Punch, (25.0, 6.0)) };
    let path = wire_contour_arcs_led(&chain, &prm, arc(1.5)).unwrap();
    let m = &path.moves;
    assert!(m[0].arc.is_none(), "the first move is straight (a Sodick control needs it)");
    let lead = m[1].arc.expect("the lead-in is a true arc");
    assert!(!lead.ccw, "a punch's lead turns to the waste side (right)");
    let entry = xy(m[1].to);
    assert!((entry.x - (20.0 + K)).abs() < 1e-9 && (entry.y - 6.0).abs() < 1e-9, "{entry:?}");
    let n = m.len();
    assert!(m[n - 2].arc.is_some() && m[n - 1].arc.is_none(), "the lead-out is an arc, then a straight");
    assert_tangent_joints(&path, 0, 2);
    assert_tangent_joints(&path, n - 3, n - 1);
    arc_laws::kerf(&path, &chain, K, -12.0).unwrap();
    arc_laws::cover(&path, &chain, &prm).unwrap();
}

#[test]
fn a_hole_too_near_for_the_lead_circle_is_refused_by_name() {
    // The hole stands 1.835 mm from the path: a radius 1 lead needs more than 2.
    let e = wire_contour_arcs_led(&circle(10.0), &params(WireSide::Die, (8.0, 0.0)), arc(1.0)).unwrap_err().to_string();
    assert!(e.contains("too near the entry point") && e.contains("tangent lead arc of radius 1"), "{e}");
    // The same hole takes a radius 0.9 lead.
    wire_contour_arcs_led(&circle(10.0), &params(WireSide::Die, (8.0, 0.0)), arc(0.9)).unwrap();
}

#[test]
fn a_radius_not_over_the_kerf_offset_is_refused_by_name() {
    for r in [K, 0.1, 0.0, -1.0, f64::NAN] {
        let e = wire_contour_arcs_led(&circle(10.0), &params(WireSide::Die, (0.0, 0.0)), arc(r)).unwrap_err().to_string();
        assert!(e.contains("must be larger than the kerf offset"), "{r}: {e}");
    }
}

#[test]
fn a_lead_that_would_cross_the_profile_is_refused_by_name() {
    // The seam is the path's nearest point, so a generated lead stays in
    // the disc about the hole that the path only touches. The guard is
    // read here on a seam that is not the nearest point: the L die
    // entered on its bottom edge from a hole in its upper arm, a radius 6
    // lead circle that reaches past the left wall.
    let chain = el();
    let seq = [line((5.0, K), (6.0, K)), line((4.0, K), (5.0, K))];
    let e = arc_leads(p(5.0, 18.0), &seq, true, 6.0, &chain, K, -12.0).err().expect("refused").to_string();
    assert!(e.contains("would bring the wire within") && e.contains("lead would cross the profile"), "{e}");
    // A radius 2 lead from the same hole stays clear.
    arc_leads(p(5.0, 18.0), &seq, true, 2.0, &chain, K, -12.0).unwrap();
}

#[test]
fn a_lead_arc_moved_two_microns_into_the_product_is_caught_by_the_kerf_law() {
    let chain = circle(10.0);
    let mut path = wire_contour_arcs_led(&chain, &params(WireSide::Die, (0.0, 0.0)), arc(1.0)).unwrap();
    // Out towards the product (+X at the entry) by 2 um.
    let mv = &mut path.moves[1];
    mv.from.x += 0.002;
    mv.to.x += 0.002;
    if let Some(a) = mv.arc.as_mut() {
        a.center.x += 0.002;
    }
    let e = arc_laws::kerf(&path, &chain, K, -12.0).unwrap_err().to_string();
    assert!(e.contains("move 1 would bring the wire within"), "{e}");
}

#[test]
fn the_straight_lead_is_the_default_and_unchanged() {
    let chain = el();
    // The L's inside corner keeps the wire radius (#3207).
    let prm = WireParams { sharp_corners: true, ..params(WireSide::Punch, (25.0, 6.0)) };
    let a = wire_contour_arcs(&chain, &prm).unwrap();
    let b = wire_contour_arcs_led(&chain, &prm, WireLead::Straight).unwrap();
    assert_eq!(a.moves, b.moves);
    assert_eq!(a.notes, b.notes);
    assert_eq!(WireLead::default(), WireLead::Straight);
}
