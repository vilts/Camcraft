//! The rigid transforms' laws (#3011): translation, the XY turn and
//! the A / B turn land every record to the micron, and the records a
//! rigid map cannot carry are refused by name.

use super::*;
use crate::cl::{
    CompSide, CompSpan, Cycle, CycleKind, EdgeBreak, MoveKind, PathBuilder, ProbeTouch, ThreadCycle,
};

pub(super) fn p(x: f64, y: f64, z: f64) -> Point3 {
    Point3::new(x, y, z)
}

pub(super) fn near(a: Point3, b: Point3) -> bool {
    (a.x - b.x).abs() <= 1e-6 && (a.y - b.y).abs() <= 1e-6 && (a.z - b.z).abs() <= 1e-6
}

/// One 2D contour pass: a clearance-plane approach, a rapid then a
/// plunge down at the lead-in start, a lead-in, a closed loop with one
/// XY arc, a lead-out, a feed then a rapid up, a clearance-plane exit.
/// Moves 3..=8 are a wear-mode compensated span.
pub(super) fn contour() -> Toolpath {
    let mut b = PathBuilder::new(p(-20.0, -20.0, 20.0));
    b.push(MoveKind::Rapid, p(-5.0, 0.0, 20.0), None);
    b.push(MoveKind::Rapid, p(-5.0, 0.0, 5.0), None);
    b.push(MoveKind::Plunge, p(-5.0, 0.0, -2.0), Some(100.0));
    b.push(MoveKind::Feed, p(0.0, 0.0, -2.0), Some(500.0));
    b.push(MoveKind::Feed, p(10.0, 0.0, -2.0), Some(500.0));
    b.push_arc(MoveKind::Feed, p(10.0, 10.0, -2.0), Some(500.0), Point2::new(10.0, 5.0), true);
    b.push(MoveKind::Feed, p(0.0, 10.0, -2.0), Some(500.0));
    b.push(MoveKind::Feed, p(0.0, 0.0, -2.0), Some(500.0));
    b.push(MoveKind::Feed, p(0.0, -5.0, -2.0), Some(500.0));
    b.push(MoveKind::Feed, p(0.0, -5.0, 5.0), Some(100.0));
    b.push(MoveKind::Rapid, p(0.0, -5.0, 20.0), None);
    b.push(MoveKind::Rapid, p(-20.0, -20.0, 20.0), None);
    let mut t = b.finish();
    t.comps.push(CompSpan {
        side: CompSide::Left,
        radius: 3.0,
        wear: true,
        first: 3,
        last: 8,
        programmed: t.moves[3..=8].to_vec(),
    });
    assert!(t.comps[0].is_sane(&t.moves));
    t
}

fn drilled() -> Toolpath {
    let mut t = Toolpath::new(vec![
        Move::new(MoveKind::Rapid, p(0.0, 0.0, 10.0), p(4.0, 2.0, 10.0), None),
        Move::new(MoveKind::Plunge, p(4.0, 2.0, 10.0), p(4.0, 2.0, -6.0), Some(80.0)),
        Move::new(MoveKind::Rapid, p(4.0, 2.0, -6.0), p(4.0, 2.0, 10.0), None),
    ]);
    t.cycles.push(Cycle {
        kind: CycleKind::Drill,
        xy: Point2::new(4.0, 2.0),
        r_z: 2.0,
        bottom_z: -6.0,
        entry_z: 0.0,
        peck: 0.0,
        dwell: 0.0,
        first_move: 0,
    });
    t.breaks.push(EdgeBreak { edge: vec![p(0.0, 0.0, 0.0), p(5.0, 0.0, 0.0)], depth: 0.5, slope: 1.0, volume: None });
    t
}

#[test]
fn translate_shifts_every_record_to_the_micron() {
    let d = [1.5, -2.25, 0.125];
    let shift = |q: Point3| p(q.x + d[0], q.y + d[1], q.z + d[2]);
    let src = contour();
    let out = translate(&src, d).expect("a finite shift translates");
    assert_eq!(out.moves.len(), src.moves.len());
    assert!(out.is_chained());
    for (a, b) in src.moves.iter().zip(&out.moves) {
        assert!(near(b.from, shift(a.from)) && near(b.to, shift(a.to)), "{a:?} -> {b:?}");
        assert_eq!(a.kind, b.kind);
    }
    let arc = out.moves[5].arc.expect("the arc stays an arc");
    assert_eq!((arc.center.x, arc.center.y, arc.ccw), (11.5, 2.75, true));
    let c = &out.comps[0];
    assert_eq!(c.side, CompSide::Left);
    assert!(c.is_sane(&out.moves));
    let dr = translate(&drilled(), d).expect("a drilled path translates");
    let cy = dr.cycles[0];
    assert_eq!((cy.xy.x, cy.xy.y, cy.r_z, cy.bottom_z, cy.entry_z), (5.5, -0.25, 2.125, -5.875, 0.125));
    assert!(near(dr.breaks[0].edge[1], p(6.5, -2.25, 0.125)));
}

#[test]
fn fine_bore_refuses_an_xy_turn_and_a_mirror_and_translates() {
    // The shift rapid at the bottom is fixed to the machine by the
    // oriented stop; a turn or a mirror points it into the finished wall.
    let mut t = drilled();
    t.cycles[0].kind = CycleKind::FineBore;
    let o = Point2::new(0.0, 0.0);
    assert_eq!(transform_xy(&t, 0.0, 0.0, 180.0, o), Err(TransformError::FineBoreShift));
    assert_eq!(transform_xy(&t, 0.0, 0.0, 90.0, o), Err(TransformError::FineBoreShift));
    let plane = MirrorPlane { point: p(0.0, 0.0, 0.0), normal: [1.0, 0.0, 0.0] };
    assert_eq!(mirror_plane(&t, plane, MirrorMode::GeometricMirror), Err(TransformError::FineBoreShift));
    // A shift, or a whole turn, keeps the shift direction.
    assert!(translate(&t, [1.0, 2.0, 0.5]).is_ok());
    assert!(transform_xy(&t, 3.0, 0.0, 360.0, o).is_ok());
    // A plain drill cycle still turns and mirrors.
    assert!(transform_xy(&drilled(), 0.0, 0.0, 180.0, o).is_ok());
    assert!(mirror_plane(&drilled(), plane, MirrorMode::GeometricMirror).is_ok());
}

#[test]
fn translate_refuses_a_non_finite_offset() {
    assert!(matches!(translate(&contour(), [f64::NAN, 0.0, 0.0]), Err(TransformError::BadInput(_))));
    assert!(matches!(translate(&contour(), [f64::MAX, 0.0, 0.0]), Err(TransformError::BadInput(_))));
}

#[test]
fn transform_xy_turns_a_quarter_about_the_pivot_then_shifts() {
    // (x, y) -> pivot + R90 (p - pivot) + (tx, ty); R90 (x, y) = (-y, x).
    let pivot = Point2::new(10.0, 0.0);
    let map = |q: Point3| p(10.0 - q.y + 1.0, q.x - 10.0 + 2.0, q.z);
    let src = contour();
    let out = transform_xy(&src, 1.0, 2.0, 90.0, pivot).expect("an XY turn maps");
    for (a, b) in src.moves.iter().zip(&out.moves) {
        assert_eq!((b.from, b.to), (map(a.from), map(a.to)), "a quarter turn is exact");
    }
    let arc = out.moves[5].arc.expect("an XY arc stays an XY arc");
    assert_eq!((arc.center.x, arc.center.y, arc.ccw), (6.0, 2.0, true));
    assert_eq!(out.comps[0].side, CompSide::Left, "a turn keeps G41");
    assert!(out.comps[0].is_sane(&out.moves));
}

#[test]
fn transform_xy_at_thirty_degrees_is_micron_accurate() {
    let (s, c) = 30f64.to_radians().sin_cos();
    let map = |q: Point3| p(3.0 + (q.x - 3.0) * c - (q.y - 4.0) * s, 4.0 + (q.x - 3.0) * s + (q.y - 4.0) * c, q.z);
    let src = contour();
    let out = transform_xy(&src, 0.0, 0.0, 30.0, Point2::new(3.0, 4.0)).expect("maps");
    assert!(out.is_chained());
    for (a, b) in src.moves.iter().zip(&out.moves) {
        assert!(near(b.from, map(a.from)) && near(b.to, map(a.to)));
    }
    let arc = out.moves[5].arc.expect("arc");
    assert!(near(p(arc.center.x, arc.center.y, 0.0), map(p(10.0, 5.0, 0.0))));
}

#[test]
fn transform_xy_refuses_a_vertical_arc_it_cannot_keep() {
    let m = Move::new(MoveKind::Feed, p(0.0, 0.0, 0.0), p(10.0, 0.0, 10.0), Some(300.0))
        .with_arc_in(Point2::new(10.0, 0.0), true, crate::cl::ArcPlane::Xz);
    let t = Toolpath::new(vec![m]);
    assert_eq!(transform_xy(&t, 0.0, 0.0, 90.0, Point2::new(0.0, 0.0)), Err(TransformError::VerticalArc));
    // A pure shift keeps the plane, so the arc rides it.
    let kept = transform_xy(&t, 5.0, 0.0, 0.0, Point2::new(0.0, 0.0)).expect("a shift keeps the plane");
    assert_eq!(kept.moves[0].arc.map(|a| (a.center.x, a.center.y)), Some((15.0, 0.0)));
}

#[test]
fn rotate_about_a_turns_the_point_and_states_the_axis() {
    // Rx(90): (x, y, z) -> (x, -z, y); the tool axis +Z leans to -Y,
    // which pose.rs reads back as A = 90.
    let t = Toolpath::new(vec![Move::new(MoveKind::Feed, p(1.0, 2.0, 10.0), p(5.0, 2.0, 10.0), Some(400.0))]);
    let out = rotate_about_axis(&t, RotaryAxis::A, p(0.0, 0.0, 0.0), 90.0, 0.01).expect("turns");
    assert_eq!((out.moves[0].from, out.moves[0].to), (p(1.0, -10.0, 2.0), p(5.0, -10.0, 2.0)));
    let axis = out.moves[0].axis.expect("a tilted move states its axis");
    assert_eq!(crate::cl::axis_to_angles(axis)[0], 90.0);
}

#[test]
fn rotate_about_b_flattens_arcs_within_the_chord_tolerance() {
    let (s, c) = 30f64.to_radians().sin_cos();
    let o = p(5.0, 0.0, -2.0);
    let ry = |q: Point3| {
        let (x, z) = (q.x - o.x, q.z - o.z);
        p(o.x + x * c + z * s, q.y, o.z - x * s + z * c)
    };
    let mut src = contour();
    src.comps.clear();
    let out = rotate_about_axis(&src, RotaryAxis::B, o, 30.0, 0.01).expect("turns");
    assert!(out.is_chained() && !out.has_arcs());
    assert!(out.moves.len() > src.moves.len(), "the arc is now chords");
    let centre = ry(p(10.0, 5.0, -2.0));
    for m in &out.moves {
        assert_eq!(m.axis.map(crate::cl::axis_to_angles).map(|a| (a[1] * 1e6).round() / 1e6), Some(30.0));
        let r = |q: Point3| ((q.x - centre.x).powi(2) + (q.y - centre.y).powi(2) + (q.z - centre.z).powi(2)).sqrt();
        if m.from.z != m.to.z && m.kind == MoveKind::Feed && r(m.from) < 5.01 && r(m.to) < 5.01 {
            assert!((r(m.from) - 5.0).abs() < 2e-6 && (r(m.to) - 5.0).abs() < 2e-6);
        }
    }
    assert!(near(out.moves[0].from, ry(src.moves[0].from)));
    assert!(near(out.end().expect("end"), ry(src.end().expect("end"))));
}

#[test]
fn rotate_about_a_refuses_an_arc_beyond_the_chord_budget() {
    let m = Move::new(
        MoveKind::Feed,
        p(1000.0, 0.0, 0.0),
        p(-1000.0, 0.0, 0.0),
        Some(400.0),
    )
    .with_arc(Point2::new(0.0, 0.0), true);
    let src = Toolpath::new(vec![m]);
    assert_eq!(
        rotate_about_axis(&src, RotaryAxis::A, p(0.0, 0.0, 0.0), 90.0, 0.0001),
        Err(TransformError::ArcChordTolerance)
    );
    // At a looser tolerance the capped sampler is sufficient. Measure
    // actual chord midpoints, which lie inside the original circle.
    let tol = 0.002;
    let out =
        rotate_about_axis(&src, RotaryAxis::A, p(0.0, 0.0, 0.0), 90.0, tol).expect("budget fits");
    assert!(!out.moves.is_empty() && !out.has_arcs());
    let mut max_error: f64 = 0.0;
    for m in &out.moves {
        let x = (m.from.x + m.to.x) * 0.5;
        let z = (m.from.z + m.to.z) * 0.5;
        max_error = max_error.max((1000.0 - x.hypot(z)).abs());
    }
    assert!(
        max_error > 0.001,
        "the pin measures sagitta, not just endpoints"
    );
    assert!(
        max_error <= tol,
        "chord midpoint error {max_error} exceeds {tol}"
    );
}

#[test]
fn rotate_about_b_bounds_arc_midpoints_in_each_plane() {
    use crate::cl::ArcPlane;
    let tol = 0.001;
    let (s, c) = 30f64.to_radians().sin_cos();
    let turn = |q: Point3| p(q.x * c + q.z * s, q.y, -q.x * s + q.z * c);
    for plane in [ArcPlane::Xy, ArcPlane::Xz, ArcPlane::Yz] {
        for (radius_end, ccw) in [(10.0, true), (12.0, true), (12.0, false)] {
            let direction = if ccw { 1.0 } else { -1.0 };
            let m = Move::new(
                MoveKind::Feed,
                plane.point(10.0, 0.0, 0.0),
                plane.point(0.0, direction * radius_end, 2.0),
                Some(400.0),
            )
            .with_arc_in(Point2::new(0.0, 0.0), ccw, plane);
            let src = Toolpath::new(vec![m]);
            let out = rotate_about_axis(&src, RotaryAxis::B, p(0.0, 0.0, 0.0), 30.0, tol)
                .expect("budget fits");
            assert!(!out.moves.is_empty() && !out.has_arcs() && out.is_chained());
            let count = out.moves.len() as f64;
            for (i, chord) in out.moves.iter().enumerate() {
                let f = (i as f64 + 0.5) / count;
                let angle = direction * std::f64::consts::FRAC_PI_2 * f;
                let radius = 10.0 + (radius_end - 10.0) * f;
                let exact = turn(plane.point(radius * angle.cos(), radius * angle.sin(), 2.0 * f));
                let midpoint = p(
                    (chord.from.x + chord.to.x) * 0.5,
                    (chord.from.y + chord.to.y) * 0.5,
                    (chord.from.z + chord.to.z) * 0.5,
                );
                let error = (midpoint.x - exact.x)
                    .hypot(midpoint.y - exact.y)
                    .hypot(midpoint.z - exact.z);
                assert!(
                    error <= tol,
                    "{plane:?}, radius {radius_end}, ccw {ccw}: {error} > {tol}"
                );
            }
        }
    }
}

#[test]
fn rotate_about_a_refuses_rounding_beyond_the_chord_tolerance() {
    let offset = 0.00000049;
    let m = Move::new(
        MoveKind::Feed,
        p(0.0001 + offset, offset, 0.0),
        p(-0.0001 + offset, offset, 0.0),
        Some(400.0),
    )
    .with_arc(Point2::new(offset, offset), true);
    assert_eq!(
        rotate_about_axis(
            &Toolpath::new(vec![m]),
            RotaryAxis::A,
            p(0.0, 0.0, 0.0),
            90.0,
            0.0000001
        ),
        Err(TransformError::ArcChordTolerance)
    );
}

#[test]
fn rotate_about_a_preserves_the_axis_when_an_arc_collapses() {
    let angle = 1e-7_f64;
    let endpoint = p(angle.cos(), angle.sin(), 0.0);
    for use_arc in [false, true] {
        let mut b = PathBuilder::new(p(1.0, 0.0, 1.0));
        b.set_axis(Some([1.0, 0.0, 0.0]));
        b.push(MoveKind::Rapid, p(1.0, 0.0, 0.0), None);
        b.set_axis(Some([0.0, 0.0, 1.0]));
        if use_arc {
            b.push_arc(MoveKind::Feed, endpoint, Some(100.0), Point2::new(0.0, 0.0), true);
        } else {
            b.push(MoveKind::Feed, endpoint, Some(100.0));
        }
        let src = b.finish();
        assert_eq!(src.has_arcs(), use_arc);
        let out = rotate_about_axis(&src, RotaryAxis::A, p(0.0, 0.0, 0.0), 90.0, 0.01)
            .expect("finite transform");
        assert!(out.is_chained() && !out.has_arcs());
        assert_eq!(out.moves.len(), 2, "arc={use_arc}: the pose change is real motion");
        assert_eq!(out.moves[0].axis, Some([1.0, 0.0, 0.0]));
        let last = out.moves.last().expect("the pose change remains");
        assert_eq!((last.from, last.to), (p(1.0, 0.0, 0.0), p(1.0, 0.0, 0.0)));
        assert_eq!(last.axis, Some([0.0, -1.0, 0.0]));
        assert_eq!((last.kind, last.feed), (MoveKind::Feed, Some(100.0)));
    }

    // A collapsed chord at an unchanged pose remains redundant.
    let mut b = PathBuilder::new(p(1.0, 0.0, 1.0));
    b.set_axis(Some([0.0, 0.0, 1.0]));
    b.push(MoveKind::Rapid, p(1.0, 0.0, 0.0), None);
    b.push_arc(MoveKind::Feed, endpoint, Some(100.0), Point2::new(0.0, 0.0), true);
    let out = rotate_about_axis(&b.finish(), RotaryAxis::A, p(0.0, 0.0, 0.0), 90.0, 0.01)
        .expect("finite transform");
    assert_eq!(out.moves.len(), 1, "an unchanged pose adds no motion");
    assert_eq!(out.moves[0].axis, Some([0.0, -1.0, 0.0]));
}

#[test]
fn rotate_about_an_axis_by_zero_is_the_identity() {
    let src = contour();
    let out = rotate_about_axis(&src, RotaryAxis::A, p(3.0, 3.0, 3.0), 360.0, 0.01).expect("identity");
    assert_eq!(out, src);
}

#[test]
fn rotate_about_an_axis_refuses_a_tilted_canned_cycle() {
    assert_eq!(
        rotate_about_axis(&drilled(), RotaryAxis::B, p(0.0, 0.0, 0.0), 15.0, 0.01),
        Err(TransformError::CannedCycleTilt)
    );
}

#[test]
fn every_transform_refuses_thread_cycles_probe_touches_and_c_moves() {
    let mut probed = contour();
    probed.probes.push(ProbeTouch { index: 3, measure: None, target: p(1.0, 0.0, -2.0) });
    assert_eq!(translate(&probed, [1.0, 0.0, 0.0]), Err(TransformError::ProbeTouch));
    let mut c_live = contour();
    c_live.moves[4] = c_live.moves[4].with_rotary(30.0);
    assert_eq!(transform_xy(&c_live, 0.0, 0.0, 10.0, Point2::new(0.0, 0.0)), Err(TransformError::RotaryMove));
    let mut threaded = contour();
    threaded.threads.push(ThreadCycle {
        start: p(0.0, 0.0, 0.0),
        end: p(1.0, 0.0, -10.0),
        major_diameter: 10.0,
        minor_diameter: 8.0,
        lead: 1.5,
        height: 1.0,
        first_depth: 0.3,
        min_depth: 0.05,
        finish_allowance: 0.05,
        infeed_angle: 0.0,
        spring_passes: 0,
        alternating: false,
        repeat_each_depth: false,
        chip_break_dips: false,
        pull_out: 0.0,
        first: 0,
        last: 1,
        hand: crate::cl::ThreadHand::Right,
        start_in_air: true,
    });
    assert_eq!(
        rotate_about_axis(&threaded, RotaryAxis::A, p(0.0, 0.0, 0.0), 0.0, 0.01),
        Err(TransformError::ThreadCycle)
    );
}
