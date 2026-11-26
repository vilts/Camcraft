//! The wire contour of a drawn line-and-arc profile (#3937): circles and
//! fillets cut as true arcs at the kerf offset, outside corners pivoted,
//! inside corners trimmed, and every case the wire cannot cut refused by
//! name. The wire and gap are SYNTHETIC (#290 D4, D10).

use std::f64::consts::{FRAC_PI_2, PI, TAU};

use super::arc_span::Span;
use super::*;
use crate::cl::MoveKind;

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

fn line(a: (f64, f64), b: (f64, f64)) -> Span {
    Span::Line { a: p(a.0, a.1), b: p(b.0, b.1) }
}

fn circle(c: (f64, f64), r: f64) -> ArcChain {
    let a = p(c.0 + r, c.1);
    ArcChain::new(vec![Span::Arc { c: p(c.0, c.1), r, a, b: a, ccw: true, sweep: TAU }]).unwrap()
}

/// A 20 x 20 L: the corner at (20, 0) rounded by a radius 3 fillet, an
/// inside corner at (10, 10), four sharp outside corners.
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

fn xy(q: crate::geometry::Point3) -> Point2 {
    p(q.x, q.y)
}

fn near(a: Point2, b: Point2) -> bool {
    (a.x - b.x).hypot(a.y - b.y) < 1e-9
}

fn refusal(chain: &ArcChain, prm: &WireParams) -> String {
    wire_contour_arcs(chain, prm).unwrap_err().to_string()
}

#[test]
fn die_circle_with_a_centred_hole_enters_towards_plus_x_and_cuts_two_half_arcs() {
    let path = wire_contour_arcs(&circle((0.0, 0.0), 5.0), &params(WireSide::Die, (0.0, 0.0))).unwrap();
    let m = &path.moves;
    assert_eq!(m.len(), 4, "{m:?}");
    assert!(near(xy(m[0].to), p(5.0 - K, 0.0)), "the lead enters towards +X: {:?}", m[0]);
    for arc_move in &m[1..3] {
        let arc = arc_move.arc.expect("a true arc");
        assert!(arc.ccw && near(arc.center, p(0.0, 0.0)));
        for q in [arc_move.from, arc_move.to] {
            assert!(((q.x.hypot(q.y)) - (5.0 - K)).abs() < 1e-12, "on the kerf circle: {q:?}");
        }
    }
    assert!(near(xy(m[1].to), p(-(5.0 - K), 0.0)), "the half turn ends at the antipode");
    assert!(near(xy(m[3].to), p(0.0, 0.0)) && m.iter().all(|q| q.kind == MoveKind::Feed && q.to.z == -12.0));
    assert!(path.notes.iter().any(|n| n.contains("towards +X")), "{:?}", path.notes);
    assert!(path.notes.iter().any(|n| n.contains("2 arc move(s)")), "{:?}", path.notes);
}

#[test]
fn punch_circle_is_entered_on_the_radius_through_the_hole() {
    let path = wire_contour_arcs(&circle((3.0, 4.0), 5.0), &params(WireSide::Punch, (3.0, 14.0))).unwrap();
    assert!(near(xy(path.moves[0].to), p(3.0, 9.0 + K)), "{:?}", path.moves[0]);
    assert_eq!(path.arc_count(), 2);
    assert!(path.moves[1].arc.is_some_and(|a| a.ccw && near(a.center, p(3.0, 4.0))));
    assert!(!path.notes.iter().any(|n| n.contains("towards +X")));
}

#[test]
fn line_and_arc_punch_pivots_outside_corners_and_trims_the_inside_corner() {
    // The L's inside corner is sharp: stated (#3207), else the cover law
    // refuses the rest the wire's own radius leaves there.
    let strict = refusal(&el(), &params(WireSide::Punch, (30.0, 5.0)));
    assert!(strict.contains("near X10.000 Y10.000 is tighter than the wire can cut"), "{strict}");
    let mut prm = params(WireSide::Punch, (30.0, 5.0));
    prm.sharp_corners = true;
    let path = wire_contour_arcs(&el(), &prm).unwrap();
    assert!(path.notes.iter().any(|n| n.starts_with(SHARP_NOTE)), "{:?}", path.notes);
    // The fillet at radius 3 + K about its own centre, and a pivot of
    // radius K about each of the four sharp outside corners.
    let arcs: Vec<_> = path.moves.iter().filter_map(|m| m.arc.map(|a| (a, m))).collect();
    assert_eq!(arcs.len(), 5, "{:?}", path.moves);
    let fillet = arcs.iter().find(|(a, _)| near(a.center, p(17.0, 3.0))).expect("the fillet's arc");
    assert!((xy(fillet.1.from).x - 17.0).abs() < 1e-12 && (xy(fillet.1.from).y + K).abs() < 1e-12);
    for corner in [p(20.0, 10.0), p(10.0, 20.0), p(0.0, 20.0), p(0.0, 0.0)] {
        let (a, m) = arcs.iter().find(|(a, _)| near(a.center, corner)).expect("a pivot");
        assert!(a.ccw && ((xy(m.from).x - corner.x).hypot(xy(m.from).y - corner.y) - K).abs() < 1e-12);
    }
    // The inside corner is trimmed to the meet of its two parallels.
    let trim = p(10.0 + K, 10.0 + K);
    assert!(path.moves.iter().any(|m| near(xy(m.to), trim)), "{:?}", path.moves);
}

#[test]
fn line_and_arc_die_trims_outside_corners_and_pivots_the_reentrant_one() {
    let mut prm = params(WireSide::Die, (5.0, 5.0));
    prm.sharp_corners = true;
    let path = wire_contour_arcs(&el(), &prm).unwrap();
    let arcs: Vec<_> = path.moves.iter().filter_map(|m| m.arc).collect();
    assert_eq!(arcs.len(), 2, "the fillet at 3 - K and one pivot: {:?}", path.moves);
    assert!(arcs.iter().any(|a| near(a.center, p(17.0, 3.0)) && a.ccw));
    assert!(arcs.iter().any(|a| near(a.center, p(10.0, 10.0)) && !a.ccw), "{arcs:?}");
    assert!(path.moves.iter().any(|m| near(xy(m.to), p(K, K))), "a trimmed corner");
}

#[test]
fn an_inside_arc_tighter_than_the_offset_is_refused_by_name() {
    // A die whose corner fillet (radius 0.1) is under the 0.165 offset.
    let chain = ArcChain::new(vec![
        line((0.0, 0.0), (9.9, 0.0)),
        Span::Arc { c: p(9.9, 0.1), r: 0.1, a: p(9.9, 0.0), b: p(10.0, 0.1), ccw: true, sweep: FRAC_PI_2 },
        line((10.0, 0.1), (10.0, 10.0)),
        line((10.0, 10.0), (0.0, 10.0)),
        line((0.0, 10.0), (0.0, 0.0)),
    ])
    .unwrap();
    let why = refusal(&chain, &params(WireSide::Die, (5.0, 5.0)));
    assert!(why.contains("inside arc of radius 0.1000") && why.contains("minimum inside radius is 0.1650"), "{why}");
}

#[test]
fn a_slot_under_the_kerf_width_and_a_crossing_neck_are_refused() {
    let thin = ArcChain::new(vec![
        line((0.0, 0.0), (10.0, 0.0)),
        line((10.0, 0.0), (10.0, 0.2)),
        line((10.0, 0.2), (0.0, 0.2)),
        line((0.0, 0.2), (0.0, 0.0)),
    ])
    .unwrap();
    let why = refusal(&thin, &params(WireSide::Die, (5.0, 0.1)));
    assert!(why.contains("under the kerf width"), "{why}");
    // Two round chambers joined by a 0.2 mm channel: the parallels of the
    // two channel walls cross.
    let r = 5.0;
    let h = 0.1_f64;
    let x = (r * r - h * h).sqrt();
    let s = h.atan2(x);
    let dumbbell = ArcChain::new(vec![
        Span::Arc { c: p(0.0, 0.0), r, a: p(x, h), b: p(x, -h), ccw: true, sweep: TAU - 2.0 * s },
        line((x, -h), (20.0 - x, -h)),
        Span::Arc { c: p(20.0, 0.0), r, a: p(20.0 - x, -h), b: p(20.0 - x, h), ccw: true, sweep: TAU - 2.0 * s },
        line((20.0 - x, h), (x, h)),
    ])
    .unwrap();
    let why = refusal(&dumbbell, &params(WireSide::Die, (0.0, 0.0)));
    assert!(why.contains("under the kerf width") || why.contains("neck"), "{why}");
}

#[test]
fn a_joint_gap_past_the_tolerance_is_refused_and_one_under_it_is_charged_to_the_margin() {
    let gap = |g: f64| {
        ArcChain::new(vec![
            line((0.0, 0.0), (10.0, 0.0)),
            Span::Arc { c: p(10.0, 5.0), r: 5.0, a: p(10.0, 0.0), b: p(10.0, 10.0), ccw: true, sweep: PI },
            line((10.0, 10.0 + g), (0.0, 10.0)),
            line((0.0, 10.0), (0.0, 0.0)),
        ])
    };
    let why = gap(1e-3).unwrap_err().to_string();
    assert!(why.contains("joint gap of 0.001000 mm") && why.contains("at most 0.00025"), "{why}");
    let mut prm = params(WireSide::Die, (5.0, 5.0));
    prm.sharp_corners = true;
    let path = wire_contour_arcs(&gap(1e-4).unwrap(), &prm).unwrap();
    assert!(path.notes.iter().any(|n| n.contains("200.0 nm farther on the waste side") && n.contains("gap up to 100.0 nm")), "{:?}", path.notes);
}

#[test]
fn an_off_lattice_centre_is_snapped_with_its_radius_kept() {
    let path = wire_contour_arcs(&circle((1.000_000_000_3, 2.000_000_000_4), 4.0), &params(WireSide::Die, (1.0, 2.5))).unwrap();
    for m in path.moves.iter().filter(|m| m.arc.is_some()) {
        let a = m.arc.unwrap();
        assert_eq!((a.center.x, a.center.y), (1.0, 2.0), "a lattice centre");
        for q in [m.from, m.to] {
            let r = (q.x - 1.0).hypot(q.y - 2.0);
            assert!((r - (4.0 - K)).abs() < 1e-12, "radius kept about the lattice centre: {r}");
        }
    }
    assert!(!path.notes.iter().any(|n| n.contains("farther")), "under 1 nm needs no margin: {:?}", path.notes);
}

/// A four-centre oval of tangent arcs (#3959): big arcs of radius 10 about
/// (0, -+sqrt 3), small arcs about (+-2 sqrt 2, 0). No centre is on the
/// lattice, so the snapped ends of two neighbour arcs differ. `lift` moves
/// the top arc up: a joint gap of that much at each of its ends.
fn oval_lifted(lift: f64) -> ArcChain {
    let (e, d, big) = (2.0 * 2f64.sqrt(), 3f64.sqrt(), 10.0);
    let small = big - e.hypot(d);
    let joint = |c: (f64, f64), to: (f64, f64)| {
        let (ux, uy) = (to.0 - c.0, to.1 - c.1);
        let l = ux.hypot(uy);
        p(c.0 + big * ux / l, c.1 + big * uy / l)
    };
    let (rb, rt) = (joint((0.0, d), (e, 0.0)), joint((0.0, -d), (e, 0.0)));
    let (lt, lb) = (joint((0.0, -d), (-e, 0.0)), joint((0.0, d), (-e, 0.0)));
    let arc = |c: Point2, r: f64, a: Point2, b: Point2| {
        let t = (b.y - c.y).atan2(b.x - c.x) - (a.y - c.y).atan2(a.x - c.x);
        Span::Arc { c, r, a, b, ccw: true, sweep: t.rem_euclid(TAU) }
    };
    ArcChain::new(vec![
        arc(p(e, 0.0), small, rb, rt),
        arc(p(0.0, lift - d), big, p(rt.x, rt.y + lift), p(lt.x, lt.y + lift)),
        arc(p(-e, 0.0), small, lt, lb),
        arc(p(0.0, d), big, lb, rb),
    ])
    .unwrap()
}

fn oval() -> ArcChain {
    oval_lifted(0.0)
}

#[test]
fn tangent_arcs_join_arc_to_arc_with_no_connector_move() {
    for side in [WireSide::Die, WireSide::Punch] {
        let hole = if side == WireSide::Die { (0.0, 0.0) } else { (20.0, 0.0) };
        let path = wire_contour_arcs(&oval(), &params(side, hole)).unwrap();
        let m = &path.moves;
        // The lead in, the arcs (the seam's arc in two parts), the lead out.
        assert!(m[1..m.len() - 1].iter().all(|q| q.arc.is_some()), "{side:?}: a line between two arcs: {m:?}");
        assert_eq!(path.arc_count(), 5, "{side:?}: {m:?}");
        for q in m {
            let len = (q.to.x - q.from.x).hypot(q.to.y - q.from.y);
            assert!(len > 1e-6, "{side:?}: a move no longer than the lattice: {q:?}");
        }
        for w in m.windows(2) {
            assert_eq!(w[0].to, w[1].from, "{side:?}: the moves chain");
        }
        assert!(arc_laws::kerf(&path, &oval(), K, -12.0).is_ok());
        // The joints gap well under 1 nm: no margin, and no note of one.
        assert!(!path.notes.iter().any(|n| n.contains("farther")), "{:?}", path.notes);
        if side == WireSide::Die {
            // One arc 2 um nearer the product: the kerf law still refuses it.
            let mut bad = path.clone();
            let m = &mut bad.moves[2];
            let c = m.arc.unwrap().center;
            for q in [&mut m.from, &mut m.to] {
                let (dx, dy) = (q.x - c.x, q.y - c.y);
                let f = 1.0 + 0.002 / dx.hypot(dy);
                (q.x, q.y) = (c.x + dx * f, c.y + dy * f);
            }
            let why = arc_laws::kerf(&bad, &oval(), K, -12.0).unwrap_err().to_string();
            assert!(why.contains("burn into the product"), "{why}");
        }
    }
}

#[test]
fn tangent_arcs_across_a_joint_gap_meet_at_its_mean_and_the_margin_holds_it() {
    let path = wire_contour_arcs(&oval_lifted(1e-4), &params(WireSide::Die, (0.0, 0.0))).unwrap();
    let m = &path.moves;
    assert!(m[1..m.len() - 1].iter().all(|q| q.arc.is_some()), "a line between two arcs: {m:?}");
    // 100 nm of drawing gap, 50 nm of joint shift (half the gap) and the
    // centres' snap (under 1 nm).
    assert!(path.notes.iter().any(|n| n.contains("150.2 nm farther on the waste side")), "{:?}", path.notes);
    assert!(path.notes.iter().any(|n| n.contains("meet at the mean of their written ends, up to 50.0 nm")), "{:?}", path.notes);
}

#[test]
fn the_laws_catch_a_mutated_arc_path() {
    let chain = circle((0.0, 0.0), 5.0);
    let prm = params(WireSide::Die, (0.0, 0.0));
    let good = wire_contour_arcs(&chain, &prm).unwrap();
    assert!(arc_laws::kerf(&good, &chain, K, -12.0).is_ok());
    // One half arc 2 um nearer the product: the kerf law refuses it.
    let mut bad = good.clone();
    for m in &mut bad.moves[1..3] {
        for q in [&mut m.from, &mut m.to] {
            let f = (5.0 - K + 0.002) / (5.0 - K);
            q.x *= f;
            q.y *= f;
        }
    }
    let why = arc_laws::kerf(&bad, &chain, K, -12.0).unwrap_err().to_string();
    assert!(why.contains("burn into the product"), "{why}");
    // The arcs written as straight chords leave the drawing uncut.
    let mut chords = good;
    for m in &mut chords.moves {
        m.arc = None;
    }
    let why = arc_laws::cover(&chords, &chain, &prm).unwrap_err().to_string();
    assert!(why.contains("tighter than the wire can cut"), "{why}");
}

#[test]
fn a_hole_too_near_the_profile_or_on_the_wrong_side_is_refused() {
    let chain = circle((0.0, 0.0), 5.0);
    assert!(refusal(&chain, &params(WireSide::Die, (4.9, 0.0))).contains("closer to the profile than the kerf offset"));
    assert!(refusal(&chain, &params(WireSide::Die, (9.0, 0.0))).contains("lies outside the profile"));
    assert!(refusal(&chain, &params(WireSide::Punch, (1.0, 0.0))).contains("lies inside the profile"));
    let mut tabbed = params(WireSide::Die, (0.0, 0.0));
    tabbed.tabs = Some(TabParams { count: Some(1), width: Some(1.0), at: Vec::new(), cutoff: None });
    assert!(refusal(&chain, &tabbed).contains("tabs"));
}

#[test]
fn the_waste_ring_lies_on_the_waste_side_of_every_arc() {
    let chain = el();
    for (side, outside) in [(WireSide::Die, false), (WireSide::Punch, true)] {
        let ring = chain.waste_ring(side, 2.5e-5).unwrap();
        for q in ring.points() {
            let r = (q.x - 17.0).hypot(q.y - 3.0);
            if q.x > 17.0 && q.y < 3.0 {
                assert!(if outside { r >= 3.0 - 1e-12 } else { r <= 3.0 + 1e-12 }, "{side:?} {q:?} {r}");
                assert!((r - 3.0).abs() <= 2.5e-5 + 1e-12);
            }
        }
    }
}
