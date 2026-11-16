//! Unit tests of `step_guard` (#1536, #1568): the meter, the division it
//! wraps, the out-of-range flag and the refusal text.

use super::*;
use truck_stepio::r#in::alias::{BSplineSurface, KnotVec};

/// #1545: the curve inside a swept surface shares the face budget.
#[test]
fn a_zero_weight_swept_curve_division_is_bounded_and_trips() {
    use truck_stepio::r#in::alias::{BSplineCurve, Curve3D, ExtrudedCurve, NurbsCurve, SweptCurve, Vector4};
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let curve = Curve3D::NurbsCurve(NurbsCurve::new(BSplineCurve::new(
            KnotVec::from(vec![0.0, 0.0, 0.5, 0.5, 1.0, 1.0]),
            vec![Vector4::new(0.0, 0.0, 0.0, 0.0); 4],
        )));
        let surface = Surface::SweptCurve(Box::new(SweptCurve::ExtrudedCurve(ExtrudedCurve::by_extrusion(
            curve, Vector3::new(0.0, 0.0, 1.0),
        ))));
        let metered = Metered::new(surface, 100_000);
        let _ = metered.parameter_division(((0.0, 1.0), (0.0, 1.0)), 0.01);
        let _ = tx.send(metered.meter.refused());
    });
    let refused = rx.recv_timeout(std::time::Duration::from_secs(20)).expect("swept curve division must finish in 20 seconds");
    assert!(refused, "a non-finite swept curve trips its face budget");
}

#[test]
fn a_refused_revolution_does_not_divide_a_bogus_radius() {
    use truck_stepio::r#in::alias::{BSplineCurve, Curve3D, Matrix4, NurbsCurve, Processor, RevolutedCurve, SweptCurve, Vector4};
    let curve = Curve3D::NurbsCurve(NurbsCurve::new(BSplineCurve::new(
        KnotVec::bezier_knot(1), vec![Vector4::new(0.0, 0.0, 0.0, 0.0); 2],
    )));
    let revolution = RevolutedCurve::by_revolution(curve, Point3::new(1_000.0, 0.0, 0.0), Vector3::new(0.0, 0.0, 1.0));
    let surface = Surface::SweptCurve(Box::new(SweptCurve::RevolutedCurve(Processor::<_, Matrix4>::new(revolution))));
    let metered = Metered::new(surface, 100_000);
    let (_, angles) = metered.parameter_division(((0.0, 1.0), (0.0, 1.0)), 0.01);
    assert!(metered.meter.refused());
    assert_eq!(angles, vec![0.0, 1.0], "a refused generator must not allocate angular samples from a fake radius");
}

/// Degree 1 in u with the knot 1.0 repeated twice: the left half sits
/// at z = 0, the right half at z = 1, so the surface jumps at u = 1.
fn discontinuous() -> Surface {
    let uk = KnotVec::from(vec![0.0, 0.0, 1.0, 1.0, 2.0, 2.0]);
    let vk = KnotVec::bezier_knot(1);
    let p = |x: f64, y: f64, z: f64| Point3::new(x, y, z);
    let cps = vec![
        vec![p(0.0, 0.0, 0.0), p(0.0, 1.0, 0.0)],
        vec![p(1.0, 0.0, 0.0), p(1.0, 1.0, 0.0)],
        vec![p(1.0, 0.0, 1.0), p(1.0, 1.0, 1.0)],
        vec![p(2.0, 0.0, 1.0), p(2.0, 1.0, 1.0)],
    ];
    Surface::BSplineSurface(Box::new(BSplineSurface::new((uk, vk), cps)))
}

fn smooth() -> Surface {
    let k = KnotVec::bezier_knot(2);
    let p = |x: f64, y: f64, z: f64| Point3::new(x, y, z);
    let cps = vec![
        vec![p(0.0, 0.0, 0.0), p(0.0, 1.0, 0.5), p(0.0, 2.0, 0.0)],
        vec![p(1.0, 0.0, 0.5), p(1.0, 1.0, 2.0), p(1.0, 2.0, 0.5)],
        vec![p(2.0, 0.0, 0.0), p(2.0, 1.0, 0.5), p(2.0, 2.0, 0.0)],
    ];
    Surface::BSplineSurface(Box::new(BSplineSurface::new((k.clone(), k), cps)))
}

/// The heaviest healthy face measured (#1568: 10208.STEP's face #1073, at
/// the 60 M facet budget) converged at 28.07 M evaluations; the cap keeps
/// at least 5x that. The default-lane twin of the heavy-lane egg crate.
#[test]
fn the_cap_keeps_headroom_over_the_heaviest_measured_healthy_face() {
    const HEAVIEST_HEALTHY: u64 = 28_070_000;
    let cap = std::hint::black_box(FACE_EVAL_CAP);
    assert!(cap >= 5 * HEAVIEST_HEALTHY, "cap {cap}");
}

/// The chord tolerance at which the egg crate of
/// `a_smooth_face_needing_more_than_the_old_cap_is_divided_not_tripped`
/// needs 39.3 M evaluations (a 1206 x 1206 grid; 8 s in release).
const TOL_PAST_OLD_CAP: f64 = 6e-5;

#[test]
fn a_discontinuous_bspline_division_stops_at_the_cap_and_trips_the_meter() {
    let m = Metered::new(discontinuous(), 200_000);
    let started = std::time::Instant::now();
    let (u, v) = m.parameter_division(((0.0, 2.0), (0.0, 1.0)), 1e-3);
    assert!(m.meter.tripped(), "the meter must report the runaway face");
    assert_eq!((u, v), (vec![0.0, 2.0], vec![0.0, 1.0]), "a tripped face hands truck no grid to triangulate");
    assert!(started.elapsed().as_secs() < 10, "bounded: {:?}", started.elapsed());
}

/// On a patch curved both ways the per-direction division (#1602) splits
/// both directions every sweep, as truck's own division does, and gives
/// the same grid.
#[test]
fn a_smooth_bspline_divides_exactly_as_truck_does_and_does_not_trip() {
    let s = smooth();
    let m = Metered::new(s.clone(), FACE_EVAL_CAP);
    let range = ((0.0, 1.0), (0.0, 1.0));
    for tol in [0.1, 1e-2, 1e-3] {
        m.meter.reset();
        assert_eq!(m.parameter_division(range, tol), s.parameter_division(range, tol), "tol {tol}");
        assert!(!m.meter.tripped());
    }
}

/// A smooth face whose division needs more evaluations than the old
/// 25 M cap (#1568: real trimmed NURBS faces reach 23-28 M) still
/// divides to the end at the cap the importer runs with.
#[test]
#[ignore = "heavy lane (#1568 cap headroom): divides a smooth patch through 39.3 M evaluations, ~8 s release / ~35 s debug - run at the cut with --release -- --ignored"]
fn a_smooth_face_needing_more_than_the_old_cap_is_divided_not_tripped() {
    // A 12 x 12 bicubic net of alternating heights: an egg crate.
    let n = 12;
    let knots = |n: usize| {
        let mut k = vec![0.0; 4];
        k.extend((1..n - 3).map(|i| i as f64));
        k.extend([(n - 3) as f64; 4]);
        KnotVec::from(k)
    };
    let cps = (0..n)
        .map(|i| (0..n).map(|j| Point3::new(i as f64, j as f64, if (i + j) % 2 == 0 { 1.0 } else { -1.0 })).collect())
        .collect();
    let s = Surface::BSplineSurface(Box::new(BSplineSurface::new((knots(n), knots(n)), cps)));
    let m = Metered::new(s, FACE_EVAL_CAP);
    let top = (n - 3) as f64;
    let (u, v) = m.parameter_division(((0.0, top), (0.0, top)), TOL_PAST_OLD_CAP);
    let used = m.meter.used.load(Ordering::Relaxed);
    assert!(used > 25_000_000, "the fixture must need more than the old cap: {used}");
    assert!(!m.meter.tripped(), "a face that converges at {used} evaluations was tripped");
    assert!(u.len() > 2 && v.len() > 2, "the division is the converged grid, not the trip's bare range");
}

/// #1568: sc_logo's face #836 was divided over u from -2e51 - a
/// trimming bound gone wild. A smooth patch asked outside its own range
/// is flagged, so its trip is never taken for a continuous face's.
#[test]
fn a_division_outside_the_surface_range_is_flagged() {
    // The flag is set before any evaluation; a cap of 1 keeps the
    // division over the wild range from running (it would run away).
    let m = Metered::new(smooth(), 1);
    let _ = m.parameter_division(((0.0, 1.0), (0.0, 1.0)), 0.1);
    assert!(!m.meter.outside.load(Ordering::Relaxed), "inside its own range");
    let _ = m.parameter_division(((-2e51, 1.0), (0.0, 1.0)), 0.1);
    assert!(m.meter.outside.load(Ordering::Relaxed), "the wild range must be flagged");
    m.meter.reset();
    assert!(m.meter.outside.load(Ordering::Relaxed), "the flag outlives the pass (the kept pass may be the wild one)");
    assert_eq!(m.meter.used.load(Ordering::Relaxed), 0, "a new pass gets its whole budget");
    assert!(within((-1e-9, 1.0 + 1e-9), (Bound::Included(0.0), Bound::Included(1.0))), "round-off is not wild");
}

/// No work outlives a refused pass: truck's rayon tasks are joined
/// before `robust_triangulation` returns, no thread is spawned, so the
/// runaway face's meter is still once the call is back.
#[test]
fn nothing_keeps_running_after_a_refused_pass() {
    use truck_meshalgo::prelude::RobustMeshableShape;
    let text = include_str!("../../../../../testdata/models/bspline_discontinuous.step");
    let exchange = truck_stepio::r#in::ruststep::parser::parse(text).expect("fixture parses");
    let table = truck_stepio::r#in::Table::from_data_section(&exchange.data[0]);
    let shell = table.shell.values().next().expect("one shell");
    let cshell = table.to_compressed_shell(shell).expect("converts");
    // A cap large enough that the pass takes a while: work left running
    // in the background would still be counting when we look.
    let g = Guarded::new(cshell, 2_000_000);
    let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| g.shell.robust_triangulation(0.01)));
    assert_eq!(g.tripped(), Some(Trip { index: 5, kind: "B-spline surface", continuous: false }));
    let spent = |g: &Guarded| g.meters.iter().map(|m| m.used.load(Ordering::Relaxed)).sum::<u64>();
    let before = spent(&g);
    std::thread::sleep(std::time::Duration::from_millis(300));
    assert_eq!(spent(&g), before, "a tessellation is still evaluating after the call returned");
}

#[test]
fn the_refusal_names_the_face_its_kind_and_the_cap() {
    let trip = |index, kind, continuous| Trip { index, kind, continuous };
    let e = refusal(trip(36, "NURBS surface", false), 62, Some(1234), Some(17), 99).to_string();
    assert!(e.contains("face #1234 (NURBS surface, face 37 of 62 in shell #17)"), "{e}");
    assert!(e.contains("did not converge within 99 evaluations"), "{e}");
    assert!(e.contains("discontinuous or degenerate"), "{e}");
    let e = refusal(trip(0, "B-spline surface", false), 1, None, None, 5).to_string();
    assert!(e.contains("face 1 of 1 (B-spline surface)"), "{e}");
    // A continuous face is never called broken.
    let e = refusal(trip(0, "NURBS surface", true), 1, None, None, 5).to_string();
    assert!(e.contains("too dense") && !e.contains("discontinuous"), "{e}");
}

/// `within` refuses a non-finite division range even against an unbounded
/// parameter range, where no comparison would catch it. (B-spline and NURBS
/// ranges, the only ones it is asked about, are bounded, so in the importer
/// the comparisons already refuse it; this pins the function's contract.)
#[test]
fn a_non_finite_division_range_is_never_within() {
    let open = (Bound::Unbounded, Bound::Unbounded);
    assert!(within((-1e9, 1e9), open));
    assert!(!within((f64::NEG_INFINITY, 1.0), open));
    assert!(!within((0.0, f64::NAN), open));
}

/// A rational degree-(2,2) patch with positive weights - the shape of
/// sc_logo's face #836 - as a one-face shell, proven continuous.
fn rational_patch_shell(cap: u64) -> Guarded {
    use truck_stepio::r#in::alias::{NurbsSurface, Vector4};
    let k = KnotVec::bezier_knot(2);
    let cps = (0..3)
        .map(|i| {
            (0..3)
                .map(|j| {
                    let w = if i == 1 && j == 1 { 2.0 } else { 1.0 };
                    Vector4::new(i as f64 * w, j as f64 * w, ((i + j) % 2) as f64 * w, w)
                })
                .collect()
        })
        .collect();
    let surface = Surface::NurbsSurface(Box::new(NurbsSurface::new(BSplineSurface::new((k.clone(), k), cps))));
    let face = CompressedFace { boundaries: vec![], orientation: true, surface };
    Guarded::new(CompressedShell { vertices: vec![], edges: vec![], faces: vec![face] }, cap)
}

/// #1568 review item 3: the range clause is what keeps sc_logo.step
/// refused. A proven rational patch that trips inside its own range is a
/// continuous trip; the same patch divided over a wild range (u from
/// -2e51) trips as NOT continuous, so its coarser mesh is never kept.
#[test]
fn a_trip_on_a_proven_face_divided_outside_its_range_is_not_continuous() {
    let g = rational_patch_shell(1);
    let _ = g.shell.faces[0].surface.parameter_division(((0.0, 1.0), (0.0, 1.0)), 0.1);
    assert_eq!(g.tripped(), Some(Trip { index: 0, kind: "NURBS surface", continuous: true }), "in range: proven");
    let g = rational_patch_shell(1);
    let _ = g.shell.faces[0].surface.parameter_division(((-2e51, 1.0), (0.0, 1.0)), 0.1);
    assert_eq!(g.tripped(), Some(Trip { index: 0, kind: "NURBS surface", continuous: false }), "out of range");
}

/// #1568 review item 2: a pass that divided the face outside its range
/// voids the proof for every later pass of that import - the coarser,
/// wild pass is the mesh a fallback would keep.
#[test]
fn an_earlier_pass_outside_the_range_voids_a_later_trip() {
    let g = rational_patch_shell(1);
    let face = &g.shell.faces[0].surface;
    let _ = face.parameter_division(((-2e51, 1.0), (0.0, 1.0)), 0.1);
    g.reset();
    assert_eq!(g.tripped(), None, "a new pass starts with its whole budget");
    let _ = face.parameter_division(((0.0, 1.0), (0.0, 1.0)), 0.05);
    assert_eq!(g.tripped(), Some(Trip { index: 0, kind: "NURBS surface", continuous: false }));
}

fn fixture_shell(cap: u64) -> Guarded {
    let text = include_str!("../../../../../testdata/models/bspline_discontinuous.step");
    let exchange = truck_stepio::r#in::ruststep::parser::parse(text).expect("fixture parses");
    let table = truck_stepio::r#in::Table::from_data_section(&exchange.data[0]);
    let shell = table.shell.values().next().expect("one shell");
    Guarded::new(table.to_compressed_shell(shell).expect("converts"), cap)
}

/// #1568 review item 1: two faces run out in one refinement pass - face 1
/// a plane (proven) and face 6 the jump. The pass is not continuous, the
/// trip names the jump, and the schedule refuses instead of keeping the
/// coarser pass (which smears the jump into a wall).
#[test]
fn a_proven_and_a_jumping_face_tripping_in_one_pass_refuse_naming_the_jump() {
    let g = fixture_shell(10);
    let trip_faces = |faces: &[usize]| {
        g.reset();
        for &i in faces {
            g.meters[i].used.store(11, Ordering::Relaxed);
        }
    };
    trip_faces(&[0]);
    assert_eq!(g.tripped(), Some(Trip { index: 0, kind: "elementary surface", continuous: true }), "the plane alone");
    trip_faces(&[0, 5]);
    assert_eq!(g.tripped(), Some(Trip { index: 5, kind: "B-spline surface", continuous: false }));
    // Through the schedule, as `step::shell_facets` maps a pass: the first
    // pass meets, the second trips on both faces - refused, by the jump.
    let mut pass = 0;
    let r = super::super::step_refine::refine(4.0, 0.001, 1_000_000, |_tol| {
        pass += 1;
        if pass == 1 {
            g.reset();
            return Ok(vec![0u8; 10]);
        }
        trip_faces(&[0, 5]);
        let trip = g.tripped().expect("tripped");
        let refusal = refusal(trip, g.meters.len(), None, None, 10);
        Err(super::super::step_refine::PassError::Unconverged { continuous: trip.continuous, refusal })
    });
    let err = r.expect_err("a smeared jump is never kept").to_string();
    assert!(err.contains("face 6 of") && err.contains("did not converge"), "{err}");
}

/// #1602: a thread flank's parameters are stretched for the mesher; the
/// wrapper shows the same surface in the stretched plane - its points,
/// range and derivatives agree with the face at the parameters it maps to.
#[test]
fn a_stretched_face_shows_the_same_surface_in_the_mesher_plane() {
    use crate::geometry::import::step_thread_fixture::flank;
    use truck_stepio::r#in::alias::{InnerSpace, MetricSpace};
    let s = flank(3);
    let m = Metered::new(s.clone(), FACE_EVAL_CAP);
    let k = m.stretch;
    assert!(k.u == 1.0 && k.v > 50.0, "the strip's long direction is stretched: {k:?}");
    assert_eq!(m.parameter_range(), (scaled(s.parameter_range().0, k.u), scaled(s.parameter_range().1, k.v)));
    let (u, v) = (0.3, 0.41);
    let (pu, pv) = k.outer((u, v));
    assert!(m.subs(pu, pv).distance(s.subs(u, v)) < 1e-12);
    // Derivatives in the stretched plane match finite differences there.
    let h = 1e-6;
    let fd_u = (m.subs(pu + h, pv) - m.subs(pu - h, pv)) / (2.0 * h);
    let fd_v = (m.subs(pu, pv + h) - m.subs(pu, pv - h)) / (2.0 * h);
    assert!((m.uder(pu, pv) - fd_u).magnitude() < 1e-5 * fd_u.magnitude().max(1.0), "u derivative");
    assert!((m.vder(pu, pv) - fd_v).magnitude() < 1e-5 * fd_v.magnitude().max(1.0), "v derivative");
    let fd_vv = (m.vder(pu, pv + h) - m.vder(pu, pv - h)) / (2.0 * h);
    assert!((m.vvder(pu, pv) - fd_vv).magnitude() < 1e-4 * fd_vv.magnitude().max(1.0), "second derivative");
    let fd_uv = (m.uder(pu, pv + h) - m.uder(pu, pv - h)) / (2.0 * h);
    assert!((m.uvder(pu, pv) - fd_uv).magnitude() < 1e-4 * fd_uv.magnitude().max(1.0), "mixed derivative");
}

/// #1602: the searches the mesher runs on a metered face answer in the
/// stretched plane, on the point's own turn and inside the knot domain -
/// with no hint on a many-turn flank, and from a neighbouring turn's hint
/// on a root band whose rulings meet its other turns when extrapolated.
#[test]
fn a_metered_face_finds_boundary_points_on_their_own_turn() {
    use crate::geometry::import::step_thread_fixture::{flank, root};
    use truck_stepio::r#in::alias::{MetricSpace, SPHint2D, SearchNearestParameter, SearchParameter};
    let inside = |(u, v): (f64, f64)| (-1e-9..=1.0 + 1e-9).contains(&u) && (-1e-9..=1.0 + 1e-9).contains(&v);
    let f = Metered::new(flank(14), FACE_EVAL_CAP);
    let r = Metered::new(root(14), FACE_EVAL_CAP);
    for k in 0..40 {
        let v = (k as f64 + 0.37) / 40.0;
        let p = f.inner.subs(0.0, v);
        let got = f.search_nearest_parameter(p, SPHint2D::None, 100).map(|(a, b)| f.own(a, b));
        assert!(got.is_some_and(|uv| inside(uv) && f.inner.subs(uv.0, uv.1).distance(p) < 1e-6), "flank v {v}: {got:?}");
        let p = r.inner.subs(1.0, v);
        let hint = r.stretch.outer((0.5, (v - 1.0 / 14.0).max(0.0)));
        let got = r.search_parameter(p, hint, 100).map(|(a, b)| r.own(a, b));
        assert!(got.is_none_or(inside), "root v {v}: an answer outside the domain {got:?}");
    }
}

/// A flat degree-(1,1) B-spline sheet over the unit square with `n` knot
/// spans each way, as a one-face shell.
fn sheet_shell(n: usize, cap: u64) -> Guarded {
    let mut knots = vec![0.0];
    knots.extend((0..=n).map(|i| i as f64 / n as f64));
    knots.push(1.0);
    let at = |i: usize| i as f64 / n as f64;
    let cps = (0..=n).map(|i| (0..=n).map(|j| Point3::new(at(i), at(j), 0.0)).collect()).collect();
    let k = KnotVec::from(knots);
    let surface = Surface::BSplineSurface(Box::new(BSplineSurface::new((k.clone(), k), cps)));
    let face = CompressedFace { boundaries: vec![], orientation: true, surface };
    Guarded::new(CompressedShell { vertices: vec![], edges: vec![], faces: vec![face] }, cap)
}

/// PR #1638 review item 1: a face with more knot spans than the second
/// search may sample (300 x 300 here; a file may carry any number) gets no
/// sample grid at all. A boundary point that needs the second search (1e-4
/// off the face, a file's own tolerance) refuses the face by name.
#[test]
fn a_face_with_more_spans_than_the_search_grid_is_refused_by_name() {
    use truck_stepio::r#in::alias::SearchNearestParameter;
    let g = sheet_shell(300, FACE_EVAL_CAP);
    let got = g.shell.faces[0].surface.search_nearest_parameter(Point3::new(0.5, 0.5, 1e-4), (0.4, 0.4), 100);
    assert_eq!(g.tripped(), Some(Trip { index: 0, kind: "B-spline surface", continuous: false }));
    assert_eq!(got, None, "a refused search hands the mesher no parameters");
    assert_eq!(g.fault(0), Some(Fault::Grid));
    assert_eq!(g.meters[0].used.load(Ordering::Relaxed), 0, "no sample of the oversized grid is evaluated");
    let trip = Trip { index: 0, kind: "B-spline surface", continuous: false };
    let e = Fault::Grid.refusal(&face_name(trip, 1, Some(7), None)).to_string();
    assert!(e.contains("face #7 (B-spline surface, face 1 of 1) could not be tessellated"), "{e}");
    assert!(e.contains("knot spans"), "{e}");
    // A new pass starts clean, and a point on the face needs no second search.
    g.reset();
    let on = g.shell.faces[0].surface.search_nearest_parameter(Point3::new(0.5, 0.5, 0.0), (0.4, 0.4), 100);
    assert!(on.is_some() && g.tripped().is_none(), "{on:?}");
}

/// PR #1638 review item 2: the second search is charged to the face's
/// evaluation budget (#1536). Boundary points 1e-4 off a many-turn flank
/// each need one; past the budget the face trips like any runaway.
#[test]
fn second_searches_spend_the_face_budget_and_trip_it() {
    use crate::geometry::import::step_thread_fixture::flank;
    use truck_stepio::r#in::alias::{SPHint2D, SearchNearestParameter};
    let off = |m: &Metered, v: f64| m.inner.subs(0.0, v) + m.inner.normal(0.0, v) * 1e-4;
    let m = Metered::new(flank(14), FACE_EVAL_CAP);
    assert!(m.search_nearest_parameter(off(&m, 0.437), SPHint2D::None, 100).is_some());
    let one = m.meter.used.load(Ordering::Relaxed);
    assert!(one >= 4_096 && !m.meter.tripped(), "one second search is counted: {one}");
    let m = Metered::new(flank(14), 50_000);
    for k in 0..40 {
        let _ = m.search_nearest_parameter(off(&m, (k as f64 + 0.37) / 40.0), SPHint2D::None, 100);
    }
    assert!(m.meter.tripped(), "40 second searches must run a 50,000 budget out");
    assert!(m.meter.used.load(Ordering::Relaxed) < 50_000 + 2 * one, "a tripped face runs no further search");
}

/// PR #1638 review item 3: a boundary point whose only answer is outside
/// the knot domain (here on the extended plane past the sheet's edge) is
/// never handed to the mesher; the face is refused by name.
#[test]
fn a_boundary_point_outside_the_knot_domain_is_refused_not_answered() {
    use truck_stepio::r#in::alias::SearchNearestParameter;
    let g = sheet_shell(1, FACE_EVAL_CAP);
    let got = g.shell.faces[0].surface.search_nearest_parameter(Point3::new(1.5, 0.5, 0.0), (0.9, 0.5), 100);
    assert_eq!(got, None, "an answer outside the knot domain was returned as valid");
    assert_eq!(g.tripped(), Some(Trip { index: 0, kind: "B-spline surface", continuous: false }));
    assert_eq!(g.fault(0), Some(Fault::Outside));
    let trip = Trip { index: 0, kind: "B-spline surface", continuous: false };
    let e = Fault::Outside.refusal(&face_name(trip, 1, None, Some(3))).to_string();
    assert!(e.contains("face 1 of 1 (B-spline surface in shell #3) could not be tessellated"), "{e}");
    assert!(e.contains("outside its knot domain"), "{e}");
}
