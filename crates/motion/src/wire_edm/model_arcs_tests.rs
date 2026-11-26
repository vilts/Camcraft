//! Arcs read back from a meshed model's wire profile (#3215): a facet
//! polygon whose chords lie on a circle is cut as that circle's arcs, a
//! true polygon stays a polygon, degenerate runs stay lines, and the
//! laws judge the arc path. The wire and gap are SYNTHETIC (#290 D4,
//! D10).

use std::f64::consts::{FRAC_PI_2, PI, TAU};

use super::arc_span::Span;
use super::model_arcs::{fit_ring, FIT_TOL, MIN_CHORD};
use super::*;

const K: f64 = 0.165;
/// The importer's chord promise (`CHORD_CAP`, 5 um).
const CHORD: f64 = 0.005;

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

/// A circle of radius `r` about `c` as an `n`-gon, vertices on it.
fn ngon(c: (f64, f64), r: f64, n: usize) -> Vec<Point2> {
    (0..n).map(|i| {
        let t = TAU * i as f64 / n as f64;
        p(c.0 + r * t.cos(), c.1 + r * t.sin())
    }).collect()
}

/// A `2hx` x `2hy` rectangle with corner fillets of radius `r`, each
/// meshed with `steps` chords (vertices on the fillet).
fn rounded(hx: f64, hy: f64, r: f64, steps: usize) -> Vec<Point2> {
    let mut pts = Vec::new();
    for (cx, cy, a0) in [(hx - r, hy - r, 0.0), (r - hx, hy - r, FRAC_PI_2), (r - hx, r - hy, PI), (hx - r, r - hy, 1.5 * PI)] {
        for s in 0..=steps {
            let t = a0 + FRAC_PI_2 * s as f64 / steps as f64;
            pts.push(p(cx + r * t.cos(), cy + r * t.sin()));
        }
    }
    pts
}

fn arcs_of(chain: &ArcChain) -> Vec<(Point2, f64)> {
    chain.spans().iter().filter_map(|s| match *s {
        Span::Arc { c, r, .. } => Some((c, r)),
        Span::Line { .. } => None,
    }).collect()
}

#[test]
fn a_meshed_hole_is_read_back_as_one_circle_and_cut_as_true_arcs() {
    // A dia-8 hole meshed at the importer's chord: 63 chords.
    let ring = ngon((0.0, 0.0), 4.0, 63);
    let m = fit_ring(&ring, CHORD).expect("the facets read as a circle");
    assert_eq!(m.arcs, 1);
    let arcs = arcs_of(&m.chain);
    assert_eq!(arcs.len(), 1);
    let (c, r) = arcs[0];
    assert!(c.x.hypot(c.y) < 1e-9 && (r - 4.0).abs() < 1e-9, "{c:?} {r}");
    assert!(m.worst_sag <= CHORD, "{}", m.worst_sag);
    let path = wire_contour_model_arcs(&m, &params(WireSide::Die, (0.0, 0.0))).expect("cuts");
    assert!(path.moves.iter().filter(|mv| mv.arc.is_some()).count() >= 2, "{:?}", path.moves);
    assert!(path.moves.iter().filter(|mv| mv.arc.is_none()).count() <= 2, "only the lead in and out are lines");
    assert!(path.notes.iter().any(|n| n.starts_with(MODEL_ARCS_NOTE)), "{:?}", path.notes);
}

#[test]
fn meshed_fillets_are_read_as_arcs_between_the_straight_sides() {
    // The die opening of the wire-iso suite: 2 mm corners at 3 degrees.
    let ring = rounded(20.0, 10.0, 2.0, 30);
    let m = fit_ring(&ring, CHORD).expect("the fillets read as arcs");
    assert_eq!(m.arcs, 4);
    let lines = m.chain.spans().iter().filter(|s| matches!(s, Span::Line { .. })).count();
    assert_eq!(lines, 4, "{:?}", m.chain.spans());
    for (c, r) in arcs_of(&m.chain) {
        assert!((r - 2.0).abs() < 1e-9, "{r}");
        assert!(((c.x.abs() - 18.0).abs() < 1e-9) && ((c.y.abs() - 8.0).abs() < 1e-9), "{c:?}");
    }
    let path = wire_contour_model_arcs(&m, &params(WireSide::Die, (0.0, 0.0))).expect("cuts");
    assert_eq!(path.moves.iter().filter(|mv| mv.arc.is_some()).count(), 4, "{:?}", path.moves);
    // A punch round the same outline cuts the same arcs outside it.
    let punch = wire_contour_model_arcs(&m, &params(WireSide::Punch, (-30.0, 0.0))).expect("cuts");
    assert_eq!(punch.moves.iter().filter(|mv| mv.arc.is_some()).count(), 4);
}

#[test]
fn collinear_vertices_on_a_side_do_not_break_the_read() {
    let mut ring = rounded(20.0, 10.0, 2.0, 30);
    // Extra vertices along the top side (a mesh fan's split points).
    let at = ring.iter().position(|q| (q.y - 10.0).abs() < 1e-12 && q.x < 0.0).expect("the top side ends");
    ring.insert(at, p(0.0, 10.0));
    ring.insert(at, p(10.0, 10.0));
    let m = fit_ring(&ring, CHORD).expect("arcs");
    assert_eq!((m.arcs, m.chain.spans().len()), (4, 8));
}

#[test]
fn a_profile_with_no_arcs_is_not_read() {
    let rect = vec![p(20.0, 10.0), p(-20.0, 10.0), p(-20.0, -10.0), p(20.0, -10.0)];
    assert!(fit_ring(&rect, CHORD).is_none());
    // An octagon's vertices lie on a circle, but its sides bow 0.76 mm
    // off it: far past what a mesh at 5 um hides. It stays a polygon.
    assert!(fit_ring(&ngon((0.0, 0.0), 10.0, 8), CHORD).is_none());
    // No chord tolerance: no read.
    assert!(fit_ring(&ngon((0.0, 0.0), 4.0, 63), 0.0).is_none());
}

#[test]
fn degenerate_runs_stay_lines() {
    // Tiny fillets: two chords each, under the three a run needs.
    assert!(fit_ring(&rounded(20.0, 10.0, 0.05, 2), CHORD).is_none());
    // A near-straight run: a radius 5000 mm bow of 20 mm meshed in 8
    // chords stays lines (past the 1000 mm radius).
    let mut flat: Vec<Point2> = (0..=8).map(|i| {
        let x = -10.0 + 2.5 * f64::from(i);
        p(x, 5000.0 - (5000.0f64 * 5000.0 - x * x).sqrt())
    }).collect();
    flat.extend([p(10.0, 20.0), p(-10.0, 20.0)]);
    assert!(fit_ring(&flat, CHORD).is_none());
    // A run whose vertices all stand within the fit tolerance of the
    // end chord is a line, never an arc.
    let mut wobble: Vec<Point2> = (0..=6).map(|i| p(f64::from(i), if i % 2 == 1 { 0.5 * FIT_TOL } else { 0.0 })).collect();
    wobble.extend([p(6.0, 6.0), p(0.0, 6.0)]);
    assert!(fit_ring(&wobble, CHORD).is_none());
}

#[test]
fn a_vertex_off_the_circle_is_never_read_into_an_arc() {
    let mut ring = ngon((0.0, 0.0), 4.0, 63);
    // One facet vertex 2 um off the circle: no single circle holds it.
    ring[20] = p(ring[20].x * (1.0 + 0.002 / 4.0), ring[20].y * (1.0 + 0.002 / 4.0));
    let m = fit_ring(&ring, CHORD).expect("the rest still reads as arcs");
    for s in m.chain.spans() {
        if let Span::Arc { c, r, .. } = *s {
            assert!((ring[20].x - c.x).hypot(ring[20].y - c.y) - r > FIT_TOL || s.start() == ring[20] || s.end() == ring[20], "{s:?}");
        }
    }
}

#[test]
fn the_laws_judge_the_read_arcs_and_catch_a_2_um_mutation() {
    let m = fit_ring(&ngon((0.0, 0.0), 4.0, 63), CHORD).expect("a circle");
    let prm = params(WireSide::Die, (0.0, 0.0));
    let good = wire_contour_model_arcs(&m, &prm).expect("cuts");
    assert!(arc_laws::kerf(&good, &m.chain, K, -12.0).is_ok());
    let mut bad = good.clone();
    for mv in bad.moves.iter_mut().filter(|mv| mv.arc.is_some()) {
        for q in [&mut mv.from, &mut mv.to] {
            let f = (4.0 - K + 0.002) / (4.0 - K);
            q.x *= f;
            q.y *= f;
        }
    }
    let why = arc_laws::kerf(&bad, &m.chain, K, -12.0).unwrap_err().to_string();
    assert!(why.contains("burn into the product"), "{why}");
    let mut chords = good;
    for mv in &mut chords.moves {
        mv.arc = None;
    }
    let why = arc_laws::cover(&chords, &m.chain, &prm).unwrap_err().to_string();
    assert!(why.contains("tighter than the wire can cut"), "{why}");
}

#[test]
fn a_meshed_inside_fillet_tighter_than_the_offset_is_refused_by_name() {
    // A die whose 0.1 mm corners (8 chords each) the 0.165 offset
    // cannot reach.
    let m = fit_ring(&rounded(10.0, 5.0, 0.1, 8), CHORD).expect("the fillets read as arcs");
    let why = wire_contour_model_arcs(&m, &params(WireSide::Die, (0.0, 0.0))).unwrap_err().to_string();
    assert!(why.contains("inside arc of radius 0.1000") && why.contains("minimum inside radius is 0.1650"), "{why}");
}

#[test]
fn a_cluster_of_noise_vertices_is_never_read_as_a_tiny_circle() {
    // A rectangle whose corner (20, 10) holds five vertices 1 um apart,
    // all on a circle of radius 0.0666: chords under the 5 um minimum.
    let c = p(20.0 - 0.0666, 10.0);
    let mut ring: Vec<Point2> = (0..5)
        .map(|k| {
            let t = 0.015 * f64::from(k);
            p(c.x + 0.0666 * t.cos(), c.y + 0.0666 * t.sin())
        })
        .collect();
    assert!((ring[1].x - ring[0].x).hypot(ring[1].y - ring[0].y) < MIN_CHORD);
    ring.extend([p(-20.0, 10.0), p(-20.0, -10.0), p(20.0, -10.0)]);
    ring.rotate_left(1);
    assert!(fit_ring(&ring, CHORD).is_none());
}

#[test]
fn split_points_beside_polygon_corners_are_never_read_as_tiny_arcs() {
    // Fillets of 8 chords bow 9.6 um: no arc at a 5 um chord. A
    // silhouette leaves split points 12 um either side of each vertex,
    // a little off the chord (float noise); no run of them is an arc.
    let base = rounded(20.0, 10.0, 2.0, 8);
    let n = base.len();
    let mut ring = Vec::new();
    for i in 0..n {
        let (a, b) = (base[i], base[(i + 1) % n]);
        let l = (b.x - a.x).hypot(b.y - a.y);
        let at = |d: f64, off: f64| p(a.x + (b.x - a.x) * d / l - (b.y - a.y) * off / l, a.y + (b.y - a.y) * d / l + (b.x - a.x) * off / l);
        ring.push(a);
        if l > 0.1 {
            ring.push(at(0.012, 0.4e-3));
            ring.push(at(l - 0.012, -0.4e-3));
        }
    }
    assert!(fit_ring(&ring, CHORD).is_none());
}

#[test]
fn fillets_on_a_nanometre_lattice_meet_their_sides_tangent_with_no_pivot() {
    // A section's vertices stand on a 1e-6 mm lattice: the fitted circles
    // miss tangency by nanoradians. The arcs are made tangent, so the
    // cut writes one arc per fillet and no pivot arc of no length.
    let ring: Vec<Point2> = rounded(20.0, 10.0, 2.0, 30).iter().map(|q| p((q.x * 1e6).round() / 1e6, (q.y * 1e6).round() / 1e6)).collect();
    let m = fit_ring(&ring, CHORD).expect("arcs");
    assert_eq!(m.arcs, 4);
    for prm in [params(WireSide::Die, (0.0, 0.0)), params(WireSide::Punch, (-30.0, 0.0))] {
        let path = wire_contour_model_arcs(&m, &prm).expect("cuts");
        assert_eq!(path.moves.iter().filter(|mv| mv.arc.is_some()).count(), 4, "{:?}", path.moves);
        assert!(path.moves.iter().all(|mv| (mv.to.x - mv.from.x).hypot(mv.to.y - mv.from.y) > 1e-3), "no move of no length: {:?}", path.moves);
    }
}
