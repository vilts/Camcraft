//! #1993: on a lathe the viewport orbits in the lathe frame - the spindle
//! axis (world Z) lies left to right, the cross-slide (world X) is up -
//! so the work-envelope cylinder lies along the spindle axis on screen.
//! The world, the envelope data and the history do not change; a mill
//! again takes the milling view back.
//! Provenance: our own tests over our own shell; no HSMWorks material.

use super::*;
use crate::view_frame::ViewFrame;
use crate::viewport::Camera;
use camcraft_app::machine::{self, AxisLimits, Machine};

fn ax(name: &str, travel: (f64, f64)) -> AxisLimits {
    AxisLimits { name: name.into(), travel: Some(travel), ..AxisLimits::default() }
}

fn lathe() -> Machine {
    Machine {
        model: "Test lathe".into(),
        kind: Some("lathe".into()),
        axes: vec![ax("X", (-5.0, 60.0)), ax("Z", (-197.0, 3.0))],
        ..Machine::default()
    }
}

fn mill() -> Machine {
    Machine {
        model: "Test mill".into(),
        axes: vec![ax("X", (-100.0, 200.0)), ax("Y", (-50.0, 50.0)), ax("Z", (-30.0, 0.0))],
        ..Machine::default()
    }
}

/// A world point on screen, in NDC (x right, y up), through the camera's
/// own draw matrix.
fn ndc(cam: &Camera, p: [f64; 3]) -> [f64; 2] {
    let m = cam.mvp(1.5);
    let v = [p[0], p[1], p[2], 1.0];
    let row = |r: usize| (0..4).map(|c| f64::from(m[c][r]) * v[c]).sum::<f64>();
    let w = row(3);
    [row(0) / w, row(1) / w]
}

fn dot(a: (f64, f64, f64), b: (f64, f64, f64)) -> f64 {
    a.0 * b.0 + a.1 * b.1 + a.2 * b.2
}

fn cross(a: (f64, f64, f64), b: (f64, f64, f64)) -> (f64, f64, f64) {
    (a.1 * b.2 - a.2 * b.1, a.2 * b.0 - a.0 * b.2, a.0 * b.1 - a.1 * b.0)
}

fn near(a: (f64, f64, f64), b: (f64, f64, f64)) -> bool {
    (a.0 - b.0).abs() < 1e-9 && (a.1 - b.1).abs() < 1e-9 && (a.2 - b.2).abs() < 1e-9
}

/// The basis is orthonormal and right-handed (right x up = back), in
/// any frame: draw, pick and the view cube read one rotation.
fn assert_proper(cam: &Camera) {
    let (r, u, f) = cam.view_basis();
    for v in [r, u, f] {
        assert!((dot(v, v) - 1.0).abs() < 1e-9, "unit basis: {v:?}");
    }
    assert!(dot(r, u).abs() < 1e-9 && dot(r, f).abs() < 1e-9 && dot(u, f).abs() < 1e-9);
    assert!(near(cross(r, u), (-f.0, -f.1, -f.2)), "right-handed: {r:?} {u:?} {f:?}");
}

#[test]
fn a_lathe_lays_the_envelope_along_the_spindle_axis_left_to_right() {
    let mut h = shell_harness();
    let undo_before = h.state().state.history.can_undo();
    machine::set(&h.state().state.world, "test-lathe.mch", Some(&lathe()));
    h.state_mut().state.show_work_envelope = true;
    h.run();
    let st = &h.state().state;
    assert_eq!(st.cam.frame, ViewFrame::Lathe, "a lathe takes the lathe frame");
    assert_eq!((st.cam.yaw, st.cam.pitch), ViewFrame::Lathe.home(), "the frame change opens its home view");
    assert_eq!(st.history.can_undo(), undo_before, "the view frame is view state, never a command");
    assert_proper(&st.cam);
    let (right, up, _) = st.cam.view_basis();
    assert!(near(right, (0.0, 0.0, 1.0)), "screen right is world +Z (away from the chuck): {right:?}");
    assert!(up.0 > 0.9 && up.2.abs() < 1e-9, "screen up is the cross-slide, world +X: {up:?}");

    // The envelope data stays about world Z (the turning paths' spindle
    // axis): only the view turns.
    let lines = crate::work_envelope::envelope_lines(st);
    assert!(!lines.is_empty(), "the lathe states its travel");
    let zs = lines.iter().flat_map(|l| [l[2], l[5]]);
    let (zmin, zmax) = zs.fold((f32::MAX, f32::MIN), |(a, b), z| (a.min(z), b.max(z)));
    assert_eq!((zmin, zmax), (-197.0, 3.0), "the cylinder ends are the Z travel");

    // On screen the axis is horizontal, +Z to the right, and the radius
    // (world +X) goes up.
    let a0 = ndc(&st.cam, [0.0, 0.0, -197.0]);
    let a1 = ndc(&st.cam, [0.0, 0.0, 3.0]);
    assert!((a0[1] - a1[1]).abs() < 1e-3, "the spindle axis is level on screen: {a0:?} {a1:?}");
    assert!(a1[0] > a0[0] + 0.1, "+Z runs left to right: {a0:?} {a1:?}");
    let rim = ndc(&st.cam, [60.0, 0.0, 3.0]);
    assert!(rim[1] > a1[1] + 0.1, "the X radius stands up from the axis: {rim:?} {a1:?}");
}

#[test]
fn a_mill_again_takes_the_milling_view_back() {
    let mut h = shell_harness();
    machine::set(&h.state().state.world, "test-lathe.mch", Some(&lathe()));
    h.run();
    assert_eq!(h.state().state.cam.frame, ViewFrame::Lathe);
    machine::set(&h.state().state.world, "test-mill.mch", Some(&mill()));
    h.run();
    let st = &h.state().state;
    assert_eq!(st.cam.frame, ViewFrame::Mill);
    assert_eq!((st.cam.yaw, st.cam.pitch), (0.9, 0.5), "the house isometric");
    let (right, _, _) = st.cam.view_basis();
    assert_eq!(right.2, 0.0, "a mill keeps world Z up: right has no Z");
    assert_proper(&st.cam);
}

#[test]
fn the_lathe_frame_keeps_pick_pan_and_the_view_cube_on_one_basis() {
    let base = Camera { target: [1.0, 2.0, -40.0], dist: 300.0, ..Camera::default() };
    let cam = base.with_frame(ViewFrame::Lathe);
    assert_proper(&cam);
    // A pick through the screen centre passes the target.
    let (o, d) = cam.ray_through([0.0, 0.0], 1.5).expect("finite ray");
    let t = cam.target;
    let s = (0..3).map(|i| (t[i] - o[i]) * d[i]).sum::<f64>();
    let miss = (0..3).map(|i| (o[i] + d[i] * s - t[i]).powi(2)).sum::<f64>().sqrt();
    assert!(miss < 1e-6, "the centre ray hits the target: miss {miss}");
    // A sideways pan moves the target along the spindle axis only.
    let mut p = cam;
    p.pan(-100.0, 0.0);
    assert!((p.target[0] - t[0]).abs() < 1e-9 && (p.target[1] - t[1]).abs() < 1e-9);
    assert!(p.target[2] > t[2], "a drag to the left moves the view toward +Z: {:?}", p.target);
    // The view cube names world directions: Front (world -Y) puts the
    // eye on world -Y in the lathe frame too, and orbits keep the frame.
    let front = cam.toward([0.0, -1.0, 0.0]);
    assert_eq!(front.frame, ViewFrame::Lathe);
    let (_, _, f) = front.view_basis();
    assert!(near(f, (0.0, 1.0, 0.0)), "the eye looks along world +Y: {f:?}");
    for d in [[1.0, 0.0, 0.0], [0.0, 0.0, 1.0], [0.0, 0.0, -1.0], [1.0, 1.0, 1.0]] {
        assert_proper(&cam.toward(d));
    }
    let mut o = cam;
    o.orbit(37.0, -12.0);
    assert_eq!(o.frame, ViewFrame::Lathe);
    assert_proper(&o);
}
