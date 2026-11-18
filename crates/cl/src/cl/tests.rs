//! The CL contract's own law tests: chaining, the arc refusals, the
//! status line, and the pose seam's conversion laws (M5.1 stage A).

use super::*;
use crate::geo2d::Point2;
use crate::geometry::Point3;
use crate::geometry::units::Unit;

fn p(x: f64, y: f64, z: f64) -> Point3 {
    Point3::new(x, y, z)
}

fn pt(x: f64, y: f64) -> Point2 {
    Point2::new(x, y)
}

#[test]
fn builder_chains_and_drops_no_motion() {
    let mut b = PathBuilder::new(p(0.0, 0.0, 5.0));
    b.push(MoveKind::Plunge, p(0.0, 0.0, 1.0), Some(100.0));
    b.push(MoveKind::Feed, p(0.0, 0.0, 1.0), Some(500.0)); // no motion
    b.push(MoveKind::Feed, p(10.0, 0.0, 1.0), Some(500.0));
    let t = b.finish();
    assert_eq!(t.len(), 2);
    assert!(t.is_chained());
    assert_eq!(t.start(), Some(p(0.0, 0.0, 5.0)));
    assert_eq!(t.end(), Some(p(10.0, 0.0, 1.0)));
    assert!(t.all_finite());
    assert!(t.is_prismatic());
}

#[test]
fn builder_drops_zero_sweep_arcs() {
    // An arc back to where the tool already stands is no motion:
    // dropped, like every other zero-length move. This is exactly
    // why fit_arcs splits full circles - the ring must not reach
    // here as one zero-sweep arc and evaporate.
    let mut b = PathBuilder::new(p(1.0, 0.0, 0.0));
    b.push_arc(MoveKind::Feed, p(1.0, 0.0, 0.0), Some(500.0), pt(0.0, 0.0), true);
    assert!(b.finish().is_empty());
    // Same XY, different Z: that is a vertical move, and the arc
    // (helical - not a thing until M2.1) falls back to the honest
    // straight plunge. The motion survives.
    let mut b = PathBuilder::new(p(1.0, 0.0, 0.0));
    b.push_arc(MoveKind::Plunge, p(1.0, 0.0, -2.0), Some(100.0), pt(0.0, 0.0), true);
    let t = b.finish();
    assert_eq!(t.len(), 1);
    assert_eq!(t.moves[0].kind, MoveKind::Plunge);
    assert!(t.moves[0].arc.is_none());
    assert!(t.is_chained());
}

#[test]
fn status_summary_counts_levels_and_arcs() {
    // Two cut levels, a lead-in arc on the first, two warnings: the
    // line reports exactly what the path shows.
    let mut b = PathBuilder::new(p(0.0, 0.0, 5.0));
    b.push(MoveKind::Rapid, p(10.0, 0.0, 5.0), None);
    b.push(MoveKind::Plunge, p(10.0, 0.0, 0.0), Some(100.0));
    b.push_arc(MoveKind::Feed, p(0.0, 10.0, 0.0), Some(500.0), pt(0.0, 0.0), true);
    b.push(MoveKind::Feed, p(-10.0, 0.0, 0.0), Some(500.0));
    b.push(MoveKind::Rapid, p(-10.0, 0.0, 3.0), None);
    b.push(MoveKind::Rapid, p(-10.0, 0.0, 5.0), None);
    b.push(MoveKind::Rapid, p(10.0, 0.0, 5.0), None);
    b.push(MoveKind::Plunge, p(10.0, 0.0, -2.0), Some(100.0));
    b.push(MoveKind::Feed, p(0.0, 0.0, -2.0), Some(500.0));
    let t = b.finish();
    assert_eq!(t.len(), 9, "every move above is real motion");
    assert_eq!(
        t.status_summary("P1", 2, Unit::Millimeters),
        "generated P1: 9 moves (2 plunge(s) @ 100, 2 level(s), 1 arc(s)), 2 warning(s)"
    );
}

#[test]
fn notes_ride_the_path_and_the_status_line() {
    // M8.1's diagnostics channel: a generator that substituted an
    // entry motion says so on the path, and the status line carries
    // the sentence. A quiet path renders exactly as it always did -
    // the notes suffix is absent, not empty.
    let mut b = PathBuilder::new(p(0.0, 0.0, 5.0));
    b.push(MoveKind::Rapid, p(10.0, 0.0, 5.0), None);
    let t = b.finish();
    assert_eq!(t.notes.len(), 0);
    assert!(
        t.status_summary("P1", 0, Unit::Millimeters)
            .ends_with("warning(s)"),
        "a quiet path's line ends at the warning count"
    );
    let mut b = PathBuilder::new(p(0.0, 0.0, 5.0));
    b.note("entry: the ramp's chord is blocked - axial dive on the floor instead");
    b.push(MoveKind::Rapid, p(10.0, 0.0, 5.0), None);
    let t = b.finish();
    assert_eq!(t.notes.len(), 1, "the note rode the builder into the path");
    let line = t.status_summary("P1", 0, Unit::Millimeters);
    assert!(
        line.ends_with("warning(s) - entry: the ramp's chord is blocked - axial dive on the floor instead"),
        "the line says it: {line}"
    );
}

#[test]
fn arc_moves_are_planar_and_chain() {
    let mut b = PathBuilder::new(p(10.0, 0.0, -2.0));
    b.push_arc(MoveKind::Feed, p(0.0, 10.0, -2.0), Some(500.0), pt(0.0, 0.0), true);
    let t = b.finish();
    assert_eq!(t.len(), 1);
    assert_eq!(t.arc_count(), 1);
    assert!(t.has_arcs());
    assert!(t.is_chained());
    assert!(t.all_finite());
    // Chords: quarter circle ccw from (10,0) to (0,10), sagitta
    // bound respected, endpoints exact.
    let cs = t.moves[0].chords(0.01);
    assert_eq!(cs.first().unwrap().0, pt(10.0, 0.0));
    assert_eq!(cs.last().unwrap().1, pt(0.0, 10.0));
    for (a, _) in &cs {
        let r = ((a.x * a.x + a.y * a.y).sqrt() - 10.0).abs();
        assert!(r <= 1e-9, "chord endpoint off the circle by {r}");
    }
    assert!(cs.windows(2).all(|w| w[0].1 == w[1].0), "chords must chain");
}

// ---- the pose seam (M5.1 stage A) ----

#[test]
fn pose_round_trips_over_the_sphere() {
    // The canonical form is exact where it is claimable: every grid
    // of A tips and B turns must rebuild a unit vector whose own
    // canonical angles rebuild THE SAME vector. Equality is geometric
    // (at the A = ±90° poles any B is the same vector, and the
    // canonical says B = 0 there - the vectors still agree).
    for a_deg in [-90.0, -60.0, -30.0, 0.0, 30.0, 60.0, 90.0] {
        for b_deg in [-180.0, -90.0, -45.0, 0.0, 45.0, 90.0, 135.0, 180.0] {
            let v = angles_to_axis([a_deg, b_deg, 0.0]).expect("finite angles");
            let len = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
            assert!((len - 1.0).abs() < 1e-12, "axis not unit: {v:?}");
            let back = axis_to_angles(v);
            let v2 = angles_to_axis(back).expect("canonical is finite");
            for k in 0..3 {
                assert!(
                    (v[k] - v2[k]).abs() < 1e-12,
                    "A {a_deg} B {b_deg}: axis {v:?} rebuilt {v2:?} (canonical {back:?})"
                );
            }
        }
    }
    // A commanded C spins the vector about Z without touching its
    // tilt: the canonical of the spun vector is a DIFFERENT pair of
    // letters for the SAME axis - the dump's law in one line.
    let tilted = angles_to_axis([30.0, 40.0, 0.0]).unwrap();
    let spun = angles_to_axis([30.0, 40.0, 90.0]).unwrap();
    assert!((tilted[2] - spun[2]).abs() < 1e-12, "C never tips");
    assert_ne!(
        axis_to_angles(tilted)[..2],
        axis_to_angles(spun)[..2],
        "the same lean seen from a quarter turn is commanded by different letters"
    );
    let fixed = angles_to_axis(axis_to_angles(tilted)).expect("canonical is finite");
    for k in 0..3 {
        assert!(
            (fixed[k] - tilted[k]).abs() < 1e-12,
            "the canonical is a fixed point of the pair: {tilted:?} -> {fixed:?}"
        );
    }
}

#[test]
fn vertical_poses_stay_exact() {
    // The branch the sim's bands and the stream dump take to keep
    // every prismatic byte identical: an exact vertical states
    // nothing new, and normalization does not move it.
    let m = Move::new(MoveKind::Feed, p(0.0, 0.0, 0.0), p(1.0, 0.0, 0.0), Some(500.0))
        .with_axis([0.0, 0.0, 1.0]);
    assert_eq!(m.axis, Some([0.0, 0.0, 1.0]), "exact in, exact out");
    assert!(is_vertical(&m.axis.unwrap()));
    let stretched = Move::new(MoveKind::Feed, p(0.0, 0.0, 0.0), p(1.0, 0.0, 0.0), Some(500.0))
        .with_axis([0.0, 0.0, 2.0]);
    assert_eq!(stretched.axis, Some([0.0, 0.0, 1.0]), "a longer up is still up");
    // End-over-end is NOT prismatic: it states a 180° B rotation.
    let flipped = Move::new(MoveKind::Feed, p(0.0, 0.0, 0.0), p(1.0, 0.0, 0.0), Some(500.0))
        .with_axis([0.0, 0.0, -1.0]);
    assert!(!is_vertical(&flipped.axis.unwrap()));
    assert_eq!(axis_to_angles(flipped.axis.unwrap()), [0.0, 180.0, 0.0]);
}

#[test]
fn a_pose_that_states_no_direction_is_refused_loudly() {
    // Prime Directive: the move keeps the honest prismatic shape and
    // the refusal says so on the way - never a silent garbage axis.
    for bad in [[0.0, 0.0, 0.0], [f64::NAN, 0.0, 1.0], [f64::INFINITY, 0.0, 0.0]] {
        let m = Move::new(MoveKind::Feed, p(0.0, 0.0, 0.0), p(1.0, 0.0, 0.0), Some(500.0))
            .with_axis(bad);
        assert_eq!(m.axis, None, "{bad:?} states no direction");
        let mut b = PathBuilder::new(p(0.0, 0.0, 0.0));
        b.set_axis(Some(bad));
        b.push(MoveKind::Feed, p(1.0, 0.0, 0.0), Some(500.0));
        assert_eq!(b.finish().moves[0].axis, None, "{bad:?} refuses to ride");
    }
}

#[test]
fn a_pose_change_at_a_standing_tip_is_real_motion() {
    // The builder's no-motion drop must never swallow a rotation:
    // same point, new angle, a move exists. Same point, same angle -
    // or no angle both times - it stays dropped, exactly as the
    // goldens have always had it.
    let tilt = angles_to_axis([40.0, 0.0, 0.0]).unwrap();
    let mut b = PathBuilder::new(p(5.0, 5.0, 0.0));
    b.push(MoveKind::Rapid, p(5.0, 5.0, 0.0), None); // nothing moved, nothing said
    b.set_axis(Some(tilt));
    b.push(MoveKind::Feed, p(5.0, 5.0, 0.0), Some(100.0)); // the head tilts
    let t = b.finish();
    assert_eq!(t.len(), 1, "a rotation in place is motion; the nothing-move dropped itself");
    assert_eq!(t.moves[0].from, t.moves[0].to);
    assert_eq!(t.moves[0].axis, Some(tilt));
    let mut b = PathBuilder::new(p(5.0, 5.0, 0.0));
    b.set_axis(Some(tilt));
    b.push(MoveKind::Feed, p(5.0, 5.0, 0.0), Some(100.0)); // tilts: motion
    b.push(MoveKind::Feed, p(5.0, 5.0, 0.0), Some(100.0)); // same pose re-stated
    assert_eq!(b.finish().len(), 1, "saying it twice posts the tilt once, never twice");
}

#[test]
fn is_prismatic_reads_the_moves_not_the_intent() {
    let mut b = PathBuilder::new(p(0.0, 0.0, 5.0));
    b.push(MoveKind::Rapid, p(1.0, 0.0, 5.0), None);
    b.set_axis(Some([0.0, 0.0, 1.0])); // a stated UP changes nothing
    b.push(MoveKind::Feed, p(2.0, 0.0, 5.0), Some(500.0));
    assert!(b.finish().is_prismatic(), "vertical is prismatic by fact");
    let t = {
        let mut b2 = PathBuilder::new(p(0.0, 0.0, 0.0));
        b2.set_axis(Some(angles_to_axis([0.0, 45.0, 0.0]).unwrap()));
        b2.push(MoveKind::Feed, p(1.0, 0.0, 0.0), Some(500.0));
        b2.finish()
    };
    assert!(!t.is_prismatic());
}

#[test]
fn arcs_carry_the_pose_they_were_walked_with() {
    let tilt = angles_to_axis([25.0, -60.0, 0.0]).unwrap();
    let mut b = PathBuilder::new(p(10.0, 0.0, -2.0));
    b.set_axis(Some(tilt));
    b.push_arc(MoveKind::Feed, p(0.0, 10.0, -2.0), Some(500.0), pt(0.0, 0.0), true);
    b.push(MoveKind::Feed, p(0.0, 20.0, -2.0), Some(500.0));
    let t = b.finish();
    assert!(t.is_chained());
    assert_eq!(t.moves[0].axis, Some(tilt));
    assert_eq!(t.moves[1].axis, Some(tilt), "the pose is modal until said otherwise");
    assert!(!t.is_prismatic());
}

/// A C-live run (issue #33): the part a quarter turn and then one and a
/// quarter turns round (unwrapped, so the second reads past -360), with
/// fractions so the dump's millidegrees show.
fn c_live() -> Toolpath {
    Toolpath::new(vec![
        Move::new(MoveKind::Feed, p(10.0, 0.0, 0.0), p(10.0, 0.0, -1.0), Some(300.0)).with_rotary(-90.5),
        Move::new(MoveKind::Feed, p(10.0, 0.0, -1.0), p(10.0, 0.0, -2.0), Some(300.0)).with_rotary(-450.25),
    ])
}

/// The stated C angles of `t`'s moves, each within `1e-9` deg of `want`.
fn assert_spindle_c(t: &Toolpath, want: &[f64]) {
    let got: Vec<Option<f64>> = t.moves.iter().map(|m| m.rotary).collect();
    assert_eq!(got.len(), want.len(), "moves: {got:?}");
    for (g, w) in got.iter().zip(want) {
        assert!(g.is_some_and(|c| (c - w).abs() < 1e-9), "C {g:?}, want {w}");
    }
}

#[test]
fn a_stated_spindle_c_is_not_a_prismatic_path() {
    // #295: a C-live cut turns the part, so it is no 3-axis program
    // even though its moves state no lean.
    assert!(!c_live().is_prismatic());
    let plain = Toolpath::new(vec![Move::new(MoveKind::Feed, p(10.0, 0.0, 0.0), p(10.0, 0.0, -1.0), Some(300.0))]);
    assert!(plain.is_prismatic(), "no C, no lean: prismatic as before");
}

#[test]
fn the_text_dump_round_trips_a_stated_spindle_c() {
    let t = c_live();
    let s = text::to_text(&t).expect("a finite path dumps");
    assert!(s.starts_with("; HSMW-CL v4"), "a C-live path is not prismatic, so v4: {s}");
    assert!(s.contains(" SC-90500\n") && s.contains(" SC-450250\n"), "every move carries its C word: {s}");
    let back = text::from_text(&s).expect("the dump reads back");
    assert_spindle_c(&back, &[-90.5, -450.25]);
    for (a, b) in t.moves.iter().zip(&back.moves) {
        assert_eq!((a.kind, a.to, a.feed), (b.kind, b.to, b.feed), "the C word moved nothing else");
    }
    let plain = Toolpath::new(vec![Move::new(MoveKind::Feed, p(10.0, 0.0, 0.0), p(10.0, 0.0, -1.0), Some(300.0))]);
    assert!(!text::to_text(&plain).expect("dumps").contains("SC"), "a path with no C dumps as before");
}

#[test]
fn a_frame_keeps_a_stated_spindle_c_on_every_move_it_maps() {
    let upright = TiltedFrame::new(p(10.0, 0.0, 1.0), [0.0, 0.0, 1.0]).expect("upright frame");
    let tilted = TiltedFrame::from_angles(p(1.0, 0.0, 5.0), 30.0, 0.0).expect("tilted frame");
    for f in [upright, tilted] {
        assert_spindle_c(&f.world_path(&c_live(), 0.001), &[-90.5, -450.25]);
    }
}

#[test]
fn a0_b180_frame_keeps_plane_x_on_machine_x() {
    // Regression (#2133): sin(180 deg) float dust (~1.22e-16) must not make
    // TiltedFrame::new pick machine Y as plane X on the face opposite the top.
    for b in [180.0, -180.0] {
        let f = TiltedFrame::from_angles(Point3::ZERO, 0.0, b).expect("A0 B+-180 frame");
        assert!(
            f.u[0] > 1.0 - 1e-12 && f.u[1].abs() < 1e-12 && f.u[2].abs() < 1e-12,
            "plane X at B={b} must stay on machine +X, got u={:?}",
            f.u
        );
        assert!(
            (f.v[1] + 1.0).abs() < 1e-12 && f.v[0].abs() < 1e-12 && f.v[2].abs() < 1e-12,
            "plane Y at B={b} must be machine -Y, got v={:?}",
            f.v
        );
        let local = f.to_local(p(-19.05, 0.0, -9.525));
        assert!(
            (local.x + 19.05).abs() < 1e-9
                && local.y.abs() < 1e-9
                && (local.z - 9.525).abs() < 1e-9,
            "point along -X on bottom face mapped to {local:?}"
        );
    }
}
