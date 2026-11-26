//! Deburr edge-filter law tests (#1888): what the kernel promises when
//! the machinist narrows the automatic edge set - a picked chain rides
//! alone, an excluded chain is left out, a convex crease that bends
//! less than the minimum edge angle is skipped and SPOKEN, and the rim
//! of a recognized round hole inside the diameter range is skipped.
//! A pick that names no exposed edge refuses by name, never a path.
//!
//! Gate map (M4.4): covers camcraft_motion::finishing3d (the deburr
//! kernel and its edge filters) and camcraft_motion::drilling
//! (`recognize`, read by the hole filter).
//! Lane shape (M4.4): every law here runs in seconds; no ignored
//! verdict lives in this file.

use camcraft_motion::cl::Toolpath;
use camcraft_motion::finishing3d::{deburr, deburr_edges, DeburrEdges, DeburrParams};
use camcraft_motion::geometry::{Point3, Triangle};
use camcraft_motion::machining2d::GenError;

fn p3(x: f64, y: f64, z: f64) -> Point3 {
    Point3::new(x, y, z)
}

/// A 90-deg cone taking a 2-wide break, every filter at rest: no
/// minimum angle, no hole filter - what the kernel did before #1888.
fn params(top_z: f64, bottom_z: f64) -> DeburrParams {
    DeburrParams {
        width: 2.0 * std::f64::consts::SQRT_2, // #1890: the chord of a 2 mm setback
        tip_angle_deg: 90.0,
        clearance_z: 20.0,
        top_z,
        bottom_z,
        feed: 500.0,
        plunge_feed: 100.0,
        min_edge_angle_deg: 0.0,
        exclude_holes: false,
        hole_min_diameter: 0.0,
        hole_max_diameter: 0.0,
        unreachable: camcraft_motion::finishing3d::UnreachableEdges::SkipWithNote,
        sizing: camcraft_motion::finishing3d::DeburrBreak::default(),
    }
}

/// Extrude a CCW (y, z) polygon along +x into a closed solid (the
/// `deburr.rs` fixture, restated: a test file owns its fixtures).
fn prism(poly: &[(f64, f64)], x_len: f64) -> Vec<Triangle> {
    let v = |side: f64, (y, z): (f64, f64)| p3(side, y, z);
    let mut t = Vec::new();
    for i in 0..poly.len() {
        let j = (i + 1) % poly.len();
        let (a, b) = (v(0.0, poly[i]), v(0.0, poly[j]));
        let (c, d) = (v(x_len, poly[j]), v(x_len, poly[i]));
        t.push(Triangle::new(a, b, c));
        t.push(Triangle::new(a, c, d));
    }
    for side in [0.0, x_len] {
        for i in 1..poly.len() - 1 {
            let (a, b, c) = (v(side, poly[0]), v(side, poly[i]), v(side, poly[i + 1]));
            t.push(if side > 0.0 { Triangle::new(a, b, c) } else { Triangle::new(a, c, b) });
        }
    }
    t
}

/// A house: walls y 0..10, z 0..5, and a roof whose two planes each
/// slope 10 degrees to a ridge at y = 5 - so the ridge bends 20
/// degrees and each eave (wall to roof) bends 80.
fn house() -> (Vec<Triangle>, f64) {
    let ridge_z = 5.0 + 5.0 * 10f64.to_radians().tan();
    (prism(&[(0.0, 0.0), (10.0, 0.0), (10.0, 5.0), (5.0, ridge_z), (0.0, 5.0)], 30.0), ridge_z)
}

/// Does a declared break run the edge a-b (either way)?
fn rides(t: &Toolpath, a: Point3, b: Point3) -> bool {
    let near = |p: Point3, q: Point3| (p.x - q.x).abs() < 1e-6 && (p.y - q.y).abs() < 1e-6 && (p.z - q.z).abs() < 1e-6;
    t.breaks
        .iter()
        .any(|br| br.edge.windows(2).any(|w| (near(w[0], a) && near(w[1], b)) || (near(w[0], b) && near(w[1], a))))
}

fn err<T>(r: Result<T, GenError>) -> String {
    match r {
        Ok(_) => panic!("a door was meant to refuse"),
        Err(e) => e.to_string(),
    }
}

/// A round bore wall alone: `n` strips about the Z axis through
/// (cx, cy), z0..z1, every facet facing the axis. Its two rings are
/// the open mesh's rims - the hole's mouths.
fn bore(cx: f64, cy: f64, r: f64, z0: f64, z1: f64, n: usize) -> Vec<Triangle> {
    let at = |i: usize, z: f64| {
        let a = std::f64::consts::TAU * (i % n) as f64 / n as f64;
        p3(cx + r * a.cos(), cy + r * a.sin(), z)
    };
    let mut t = Vec::new();
    for i in 0..n {
        let (a, b, c, d) = (at(i, z0), at(i + 1, z0), at(i + 1, z1), at(i, z1));
        // Wound so the normal points at the axis (the void's side).
        t.push(Triangle::new(a, c, b));
        t.push(Triangle::new(a, d, c));
    }
    t
}

/// A flat square patch at z = 0: a rim that is no hole.
fn patch(x0: f64) -> Vec<Triangle> {
    vec![
        Triangle::new(p3(x0, 0.0, 0.0), p3(x0 + 4.0, 0.0, 0.0), p3(x0 + 4.0, 4.0, 0.0)),
        Triangle::new(p3(x0, 0.0, 0.0), p3(x0 + 4.0, 4.0, 0.0), p3(x0, 4.0, 0.0)),
    ]
}

#[test]
fn a_crease_below_the_minimum_edge_angle_is_skipped_out_loud() {
    let (f, zr) = house();
    let (ridge_a, ridge_b) = (p3(0.0, 5.0, zr), p3(30.0, 5.0, zr));
    let (eave_a, eave_b) = (p3(0.0, 10.0, 5.0), p3(30.0, 10.0, 5.0));
    // The window keeps the roof's edges and drops the floor's.
    let all = deburr(&f, &params(20.0, 2.0)).expect("the roof is all burr");
    assert!(rides(&all, ridge_a, ridge_b), "no filter: the 20-deg ridge rides");
    assert!(rides(&all, eave_a, eave_b));

    let t = deburr(&f, &DeburrParams { min_edge_angle_deg: 40.0, ..params(20.0, 2.0) }).expect("the eaves still ride");
    assert!(!rides(&t, ridge_a, ridge_b), "a 20-deg bend is under the 40-deg minimum");
    assert!(rides(&t, eave_a, eave_b), "an 80-deg bend is over it");
    assert!(
        t.notes.iter().any(|n| n.contains("1 convex edges bend less than 40")),
        "the skip is spoken: {:?}",
        t.notes
    );
    // The bound itself: a bend equal to the minimum is kept.
    let at = deburr(&f, &DeburrParams { min_edge_angle_deg: 20.0, ..params(20.0, 2.0) }).expect("rides");
    assert!(rides(&at, ridge_a, ridge_b), "bend >= minimum is kept");
}

#[test]
fn a_picked_chain_rides_alone() {
    let (f, zr) = house();
    let eave = vec![p3(0.0, 10.0, 5.0), p3(30.0, 10.0, 5.0)];
    let pick = [eave.clone()];
    let t = deburr_edges(&f, &params(20.0, 2.0), &DeburrEdges { include: &pick, exclude: &[] }).expect("the eave rides");
    assert_eq!(t.breaks.len(), 1, "one picked chain, one run: {:?}", t.breaks);
    assert!(rides(&t, eave[0], eave[1]));
    assert!(!rides(&t, p3(0.0, 5.0, zr), p3(30.0, 5.0, zr)));

    // A manual pick is the machinist's word: the minimum angle does
    // not drop it.
    let ridge = [vec![p3(0.0, 5.0, zr), p3(30.0, 5.0, zr)]];
    let p = DeburrParams { min_edge_angle_deg: 40.0, ..params(20.0, 2.0) };
    let t = deburr_edges(&f, &p, &DeburrEdges { include: &ridge, exclude: &[] }).expect("the picked ridge rides");
    assert!(rides(&t, ridge[0][0], ridge[0][1]));
}

#[test]
fn a_pick_that_names_no_exposed_edge_refuses_by_name() {
    // A C-channel's inner corner is concave: pencil's corner, not a burr.
    let c = prism(&[(0.0, 0.0), (10.0, 0.0), (10.0, 6.0), (8.0, 6.0), (8.0, 2.0), (2.0, 2.0), (2.0, 6.0), (0.0, 6.0)], 30.0);
    let concave = [vec![p3(0.0, 8.0, 2.0), p3(30.0, 8.0, 2.0)]];
    let e = err(deburr_edges(&c, &params(20.0, -10.0), &DeburrEdges { include: &concave, exclude: &[] }));
    assert!(e.contains("picked"), "{e}");
    assert!(e.contains("rim or a convex crease"), "{e}");

    // One good pick and one concave pick: the good one rides, the
    // other is spoken.
    let both = [vec![p3(0.0, 10.0, 6.0), p3(30.0, 10.0, 6.0)], concave[0].clone()];
    let t = deburr_edges(&c, &params(20.0, -10.0), &DeburrEdges { include: &both, exclude: &[] }).expect("one rides");
    assert_eq!(t.breaks.len(), 1);
    assert!(t.notes.iter().any(|n| n.contains("1 picked edge chains")), "{:?}", t.notes);
}

#[test]
fn an_excluded_chain_is_left_out_of_the_automatic_edges() {
    let (f, _) = house();
    let (eave_a, eave_b) = (p3(0.0, 10.0, 5.0), p3(30.0, 10.0, 5.0));
    let (other_a, other_b) = (p3(0.0, 0.0, 5.0), p3(30.0, 0.0, 5.0));
    let skip = [vec![eave_a, eave_b]];
    let t = deburr_edges(&f, &params(20.0, 2.0), &DeburrEdges { include: &[], exclude: &skip }).expect("the rest rides");
    assert!(!rides(&t, eave_a, eave_b), "the excluded eave is left sharp");
    assert!(rides(&t, other_a, other_b), "the other eave still rides");
    assert!(t.notes.iter().any(|n| n.contains("1 edges excluded")), "{:?}", t.notes);
}

#[test]
fn a_hole_rim_inside_the_diameter_range_is_skipped() {
    // A 6 mm bore beside a square patch, both mouths at z = 0.
    let mut f = bore(0.0, 0.0, 3.0, -8.0, 0.0, 32);
    f.extend(patch(20.0));
    let on_bore = |t: &Toolpath| {
        t.breaks.iter().flat_map(|b| b.edge.iter()).filter(|q| (q.x.hypot(q.y) - 3.0).abs() < 1e-6).count()
    };
    let all = deburr(&f, &params(0.0, -4.0)).expect("rims ride");
    assert!(on_bore(&all) > 0, "no filter: the bore's mouth rides");

    let p = DeburrParams { exclude_holes: true, hole_min_diameter: 5.0, hole_max_diameter: 7.0, ..params(0.0, -4.0) };
    let t = deburr(&f, &p).expect("the patch still rides");
    assert_eq!(on_bore(&t), 0, "a 6 mm hole is inside 5..7");
    assert_eq!(t.breaks.len(), 1, "the patch's rim alone");
    assert!(t.notes.iter().any(|n| n.contains("hole")), "the skip is spoken: {:?}", t.notes);

    // Outside the range the hole is an edge like any other.
    let p = DeburrParams { exclude_holes: true, hole_min_diameter: 1.0, hole_max_diameter: 2.0, ..params(0.0, -4.0) };
    let t = deburr(&f, &p).expect("rims ride");
    assert!(on_bore(&t) > 0, "a 6 mm hole is outside 1..2");
}

#[test]
fn a_cross_hole_rim_inside_the_diameter_range_is_skipped() {
    // The same 6 mm bore laid along X (x -8..0, axis through y 0,
    // z 0): `recognize_cross`'s finding. Its two rings stand in YZ
    // planes; the stretches a vertical cone can ride are their top
    // and bottom arcs.
    let mut f: Vec<Triangle> = bore(0.0, 0.0, 3.0, -8.0, 0.0, 32)
        .iter()
        .map(|t| {
            let m = |q: Point3| p3(q.z, q.x, q.y);
            Triangle::new(m(t.a), m(t.b), m(t.c))
        })
        .collect();
    f.extend(patch(20.0));
    let on_bore = |t: &Toolpath| {
        t.breaks.iter().flat_map(|b| b.edge.iter()).filter(|q| (q.y.hypot(q.z) - 3.0).abs() < 1e-6).count()
    };
    let all = deburr(&f, &params(2.0, -6.0)).expect("rims ride");
    assert!(on_bore(&all) > 0, "no filter: the cross bore's rims ride");

    let p = DeburrParams { exclude_holes: true, hole_min_diameter: 5.0, hole_max_diameter: 7.0, ..params(2.0, -6.0) };
    let t = deburr(&f, &p).expect("the patch still rides");
    assert_eq!(on_bore(&t), 0, "both rims of the 6 mm cross hole are inside 5..7: {:?}", t.breaks);
    assert_eq!(t.breaks.len(), 1, "the patch's rim alone");
}

#[test]
fn the_filter_doors_refuse_by_name() {
    let f = patch(0.0);
    let e = err(deburr(&f, &DeburrParams { min_edge_angle_deg: f64::NAN, ..params(0.0, -4.0) }));
    assert!(e.contains("minimum edge angle"), "{e}");
    let e = err(deburr(&f, &DeburrParams { min_edge_angle_deg: 180.0, ..params(0.0, -4.0) }));
    assert!(e.contains("minimum edge angle"), "{e}");
    let p = DeburrParams { exclude_holes: true, hole_min_diameter: 5.0, hole_max_diameter: 2.0, ..params(0.0, -4.0) };
    let e = err(deburr(&f, &p));
    assert!(e.contains("hole diameter range"), "{e}");
    let bad = [vec![p3(f64::NAN, 0.0, 0.0), p3(1.0, 0.0, 0.0)]];
    let e = err(deburr_edges(&f, &params(0.0, -4.0), &DeburrEdges { include: &bad, exclude: &[] }));
    assert!(e.contains("non-finite"), "{e}");
}
