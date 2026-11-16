//! Unit tests of `step_nearest` (#1602): a boundary point's parameters are
//! found on its own turn of a many-turn strip, inside the knot domain.

use super::*;
use crate::geometry::import::step_thread_fixture::{flank, root};
use truck_stepio::r#in::alias::{BSplineSurface, KnotVec, ParametricSurface3D, SPHint2D};

/// A meter no test here is meant to run out of.
fn meter() -> Meter {
    Meter::new(crate::geometry::import::step_guard::FACE_EVAL_CAP)
}

fn on_surface_inside(s: &Surface, p: Point3, uv: Option<(f64, f64)>) -> bool {
    let Some(domain) = super::super::step_pole::knot_domain(s) else { return false };
    uv.is_some_and(|uv| inside(uv, domain) && s.subs(uv.0, uv.1).distance(p) <= SAME_POINT)
}

/// The first boundary point of a 14-turn flank, searched with no hint:
/// truck's 51 x 51 presearch lands on the neighbouring turn and Newton
/// settles off the surface (the #1602 hole). The rescue finds it.
#[test]
fn a_point_on_a_many_turn_flank_is_found_on_its_own_turn() {
    let s = flank(14);
    let mut fixed = 0;
    for k in 0..40 {
        let v = (k as f64 + 0.37) / 40.0;
        let p = s.subs(0.0, v);
        let truck = s.search_nearest_parameter(p, SPHint2D::None, TRIALS);
        let found = rescue(&s, p, truck, &meter());
        assert!(on_surface_inside(&s, p, found), "v {v}: truck {truck:?} -> {found:?}");
        fixed += usize::from(!on_surface_inside(&s, p, truck));
    }
    assert!(fixed > 0, "the fixture must reproduce truck's miss on some point");
}

/// A root band's straight rulings, extrapolated past u = 1, meet its other
/// turns: truck can find the point exactly at a u outside [0, 1]. The
/// answer must come back inside the domain, on the point's own turn.
#[test]
fn a_point_on_a_root_band_is_found_inside_the_knot_domain() {
    let s = root(14);
    let mut fixed = 0;
    for k in 0..40 {
        let v = (k as f64 + 0.37) / 40.0;
        let p = s.subs(1.0, v);
        // The hint a mesher would carry from a neighbouring turn.
        let hint = (0.5, (v - 1.0 / 14.0).max(0.0));
        let truck = s.search_nearest_parameter(p, hint, TRIALS);
        let found = rescue(&s, p, truck, &meter());
        assert!(on_surface_inside(&s, p, found), "v {v}: truck {truck:?} -> {found:?}");
        fixed += usize::from(!on_surface_inside(&s, p, truck));
    }
    assert!(fixed > 0, "the fixture must reproduce truck's out-of-domain answer on some point");
}

/// A point the face really does not reach is refused, not answered: on a
/// flat unit square, a point past its edge is found on the extended plane,
/// which no parameter inside the domain matches (PR #1638 review item 3).
#[test]
fn a_point_off_the_face_is_refused_not_answered_outside_the_domain() {
    let k = KnotVec::bezier_knot(1);
    let p = |x: f64, y: f64| Point3::new(x, y, 0.0);
    let cps = vec![vec![p(0.0, 0.0), p(0.0, 1.0)], vec![p(1.0, 0.0), p(1.0, 1.0)]];
    let s = Surface::BSplineSurface(Box::new(BSplineSurface::new((k.clone(), k), cps)));
    let off = Point3::new(1.5, 0.5, 0.0);
    let truck = s.search_nearest_parameter(off, (0.9, 0.5), TRIALS);
    assert!(truck.is_some_and(|(u, _)| u > 1.4), "truck extends the plane: {truck:?}");
    let m = meter();
    assert_eq!(rescue(&s, off, truck, &m), None);
    assert_eq!(m.fault(), Some(Fault::Outside));
    // An answer already inside and on the surface is returned untouched,
    // and so is an answer inside the domain that no search betters.
    let m = meter();
    let on = Point3::new(0.25, 0.75, 0.0);
    assert_eq!(rescue(&s, on, Some((0.25, 0.75)), &m), Some((0.25, 0.75)));
    let above = Point3::new(0.25, 0.75, 1e-4);
    let kept = rescue(&s, above, Some((0.25, 0.75)), &m);
    assert!(kept.is_some_and(|(u, v)| (u - 0.25).abs() < 1e-9 && (v - 0.75).abs() < 1e-9), "{kept:?}");
    assert_eq!(m.fault(), None);
}

/// The sample grid has a few samples in every knot span and stays inside
/// the evaluation bound however many spans a face has.
#[test]
fn the_sample_grid_covers_every_span_within_its_bound() {
    let s = flank(14);
    let Some(domain) = super::super::step_pole::knot_domain(&s) else { panic!("a NURBS has a knot domain") };
    let Some((us, vs)) = samples(&s, domain) else { panic!("141 x 2 knots are inside the bound") };
    assert!(us.len() * vs.len() <= MAX_SAMPLES, "{} x {}", us.len(), vs.len());
    assert!(vs.len() > 140, "every one of the 140 spans is sampled: {}", vs.len());
    assert_eq!((us.first(), us.last()), (Some(&0.0), Some(&1.0)));
    // The refusal's text names the bound the grid is held to.
    assert!(Fault::Grid.why().contains(&MAX_SAMPLES.to_string()));
}

/// Whatever the samples' Newton runs find, the rescue never answers
/// outside the knot domain: on a flat square, a point on the plane past
/// its edge draws every run there, and truck's own (bogus) answer is
/// refused with the face flagged.
#[test]
fn a_rescue_never_answers_outside_the_knot_domain() {
    let k = KnotVec::bezier_knot(1);
    let p = |x: f64, y: f64| Point3::new(x, y, 0.0);
    let cps = vec![vec![p(0.0, 0.0), p(0.0, 1.0)], vec![p(1.0, 0.0), p(1.0, 1.0)]];
    let s = Surface::BSplineSurface(Box::new(BSplineSurface::new((k.clone(), k), cps)));
    let bogus = Some((5.0, 0.5));
    let m = meter();
    assert_eq!(rescue(&s, Point3::new(1.5, 0.5, 0.0), bogus, &m), None);
    assert_eq!(m.fault(), Some(Fault::Outside));
}

/// A root band's point found exactly on an extrapolated ruling (u = 19.5,
/// one pitch down at the same angle) is replaced by its own in-domain
/// parameter, although that one is a rounding error farther away.
#[test]
fn an_exact_answer_outside_the_domain_gives_way_to_one_on_the_surface_inside() {
    let s = root(14);
    let v = 0.5;
    let outside = (19.5, v + 1.0 / 14.0);
    let p = s.subs(outside.0, outside.1);
    let found = rescue(&s, p, Some(outside), &meter());
    assert!(on_surface_inside(&s, p, found), "{found:?}");
}

/// A boundary point a little off its face (a file's own tolerance), whose
/// first answer is a turn away, takes the nearer answer on its own turn
/// although that one is not within the acceptance distance either.
#[test]
fn a_nearer_answer_replaces_a_far_one_off_the_surface() {
    let s = flank(14);
    let (u, v) = (0.0, 0.437);
    let p = s.subs(u, v) + s.normal(u, v) * 1e-4;
    let wrong_turn = Some((0.0, v - 1.0 / 14.0));
    let found = rescue(&s, p, wrong_turn, &meter()).expect("an answer");
    assert!((found.1 - v).abs() < 1e-3, "{found:?}");
}

/// A degree-1 hairpin: sheet A along x at z = 0 for u in [0, 0.5], folded at
/// x = 1 back to sheet B, which climbs to z = `gap` at x = 0.
fn hairpin(gap: f64) -> Surface {
    let uk = KnotVec::from(vec![0.0, 0.0, 0.5, 1.0, 1.0]);
    let vk = KnotVec::bezier_knot(1);
    let p = |x: f64, y: f64, z: f64| Point3::new(x, y, z);
    let cps = vec![
        vec![p(0.0, 0.0, 0.0), p(0.0, 1.0, 0.0)],
        vec![p(1.0, 0.0, 0.0), p(1.0, 1.0, 0.0)],
        vec![p(0.0, 0.0, gap), p(0.0, 1.0, gap)],
    ];
    Surface::BSplineSurface(Box::new(BSplineSurface::new((uk, vk), cps)))
}

/// PR #1638 review item 4: on a surface that folds near itself, a boundary
/// point a little off its sheet but nearer the other sheet keeps truck's
/// answer on its own sheet (the one its hint followed). It moves only when
/// truck's answer is more than `JUMP` off and the other sheet is
/// `JUMP_RATIO` times nearer.
#[test]
fn a_boundary_point_does_not_jump_to_a_fold_of_the_surface_within_the_bound() {
    // Sheets 5e-4 apart at x = 0.5; the point 4e-4 above A, 1e-4 below B.
    let s = hairpin(1e-3);
    let p = Point3::new(0.5, 0.5, 4e-4);
    let on_a = (0.25, 0.5);
    let to_b = s.search_nearest_parameter(p, (0.75, 0.5), TRIALS);
    let b_d = to_b.map(|(u, v)| s.subs(u, v).distance(p));
    assert!(to_b.is_some_and(|(u, _)| u > 0.5) && b_d.is_some_and(|d| d < 2e-4), "the fold is nearer: {to_b:?} {b_d:?}");
    let m = meter();
    assert_eq!(rescue(&s, p, Some(on_a), &m), Some(on_a), "a point within JUMP of its own sheet jumped the fold");
    assert_eq!(m.fault(), None);
    // Sheets 0.05 apart at x = 0.5: a point 0.03 off A and 0.02 off B is
    // not clearly on B and stays; one 0.0495 off A and 5e-4 off B moves.
    let s = hairpin(0.1);
    let between = Point3::new(0.5, 0.5, 0.03);
    assert_eq!(rescue(&s, between, Some(on_a), &meter()), Some(on_a));
    let on_b = Point3::new(0.5, 0.5, 0.0495);
    let moved = rescue(&s, on_b, Some(on_a), &meter());
    assert!(moved.is_some_and(|(u, _)| u > 0.5), "a point clearly on the other sheet must move there: {moved:?}");
}
