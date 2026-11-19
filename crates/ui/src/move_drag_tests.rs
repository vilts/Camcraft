//! #1198 slice b, the drag law without a window: the dragged distance is
//! exact (cursor ray against the axis line), a drag in a setup that holds
//! the part turned moves it along the axis the user SEES, the rotation is
//! kept bit for bit, and a non-finite distance is refused by name.
//! Provenance: our own tests; rigid-body transform algebra.

use camcraft_app::commands::ImportModel;
use camcraft_app::geometry::{Point3, cube};
use camcraft_app::model;

use crate::interact::project_uv;
use crate::move_drag::{self, arms_px, handles};
use crate::state::{CommandKind, State};

fn run(state: &mut State, k: CommandKind) {
    state.pending.push_back(k);
    state.drain_actions();
}

/// A 40 x 20 x 10 part, selected, Move mode on, framed in an 800 x 600 pane.
fn part() -> State {
    let mut state = State::new();
    let mesh = cube(Point3::new(0.0, 0.0, 0.0), Point3::new(40.0, 20.0, 10.0));
    state.history.execute(&state.world, ImportModel::from_mesh("part", mesh));
    state.viewport_rect = ([0.0, 0.0], [800.0, 600.0]);
    run(&mut state, CommandKind::FrameAll);
    run(&mut state, CommandKind::Select(Some("part".into())));
    state.move_handles = true;
    state
}

fn pose(state: &State) -> camcraft_app::schema::Location {
    let e = model::find_by_name(&state.world, "part").expect("part");
    model::transform_of(&state.world, &e).unwrap_or_default()
}

fn shown(state: &State) -> camcraft_app::schema::Location {
    let e = model::find_by_name(&state.world, "part").expect("part");
    camcraft_app::setup_align::view_pose(&state.world, &e).unwrap_or_default()
}

/// The px where the shown-frame point `p` lands.
fn px(state: &State, p: [f64; 3]) -> [f32; 2] {
    let mvp = state.cam.mvp(800.0 / 600.0);
    let uv = project_uv(&mvp, p).expect("in front of the camera");
    [uv[0] * 800.0, uv[1] * 600.0]
}

#[test]
fn the_drag_distance_is_the_axis_point_under_the_cursor() {
    let mut state = part();
    let h = handles(&state).expect("handles on the selected part");
    let ball = arms_px(&state).iter().find(|a| a.0 == 1).map(|a| a.2).expect("a Y ball");
    assert!(move_drag::press(&mut state, ball), "the Y ball takes the press");
    let target = [h.center[0], h.center[1] + h.len + 12.5, h.center[2]];
    let to = px(&state, target);
    move_drag::motion(&mut state, to);
    let d = state.body_drag.as_ref().map(|d| d.delta).expect("dragging");
    assert!((d - 12.5).abs() < 0.05, "12.5 mm along Y, got {d}");
    assert_eq!(move_drag::readout(&state).as_deref().map(|s| s.starts_with("Y +12.")), Some(true));
    move_drag::release(&mut state);
    assert!(matches!(state.pending.front(), Some(CommandKind::MoveModelBy { delta, .. }) if delta[0] == 0.0 && delta[2] == 0.0 && (delta[1] - d).abs() < 1e-12));
    assert_eq!(state.pending.len(), 1, "one command per drag");
}

#[test]
fn a_press_off_the_balls_is_not_a_drag() {
    let mut state = part();
    assert!(!move_drag::press(&mut state, [3.0, 3.0]));
    assert!(state.body_drag.is_none());
    state.move_handles = false;
    assert!(handles(&state).is_none(), "Move off: nothing to grab");
}

#[test]
fn a_turned_setup_moves_the_part_along_the_axis_it_shows() {
    let mut state = part();
    run(&mut state, CommandKind::CreateSetup { name: None, datum_from: None });
    run(&mut state, CommandKind::ActivateSetup(Some("Setup1".into())));
    run(&mut state, CommandKind::AlignPart { setup: "Setup1".into(), angles: [0.0, 0.0, 90.0], pivot: Default::default() });
    let (doc0, view0) = (pose(&state), shown(&state));
    run(&mut state, CommandKind::MoveModelBy { name: "part".into(), delta: [10.0, 0.0, 0.0] });
    let (doc1, view1) = (pose(&state), shown(&state));
    assert!((view1.pos.x - view0.pos.x - 10.0).abs() < 1e-9, "shown +X 10: {view0:?} -> {view1:?}");
    assert!((view1.pos.y - view0.pos.y).abs() < 1e-9 && (view1.pos.z - view0.pos.z).abs() < 1e-9);
    // The document frame is turned -90 about Z from the shown one.
    assert!((doc1.pos.y - doc0.pos.y + 10.0).abs() < 1e-9 && (doc1.pos.x - doc0.pos.x).abs() < 1e-9, "{doc0:?} -> {doc1:?}");
}

#[test]
fn the_rotation_is_kept_bit_for_bit() {
    let mut state = part();
    run(&mut state, CommandKind::MoveModel { name: "part".into(), pos: [1.0, 2.0, 3.0], roll_deg: 10.0, pitch_deg: 20.0, yaw_deg: 33.0 });
    let rot = pose(&state).rot;
    run(&mut state, CommandKind::MoveModelBy { name: "part".into(), delta: [0.0, 0.0, -4.0] });
    let after = pose(&state);
    assert_eq!(after.rot.map(f64::to_bits), rot.map(f64::to_bits), "a slide never re-derives the turn");
    assert!((after.pos.z - -1.0).abs() < 1e-12);
}

#[test]
fn a_non_finite_distance_is_refused_by_name() {
    let mut state = part();
    state.history.mark_clean();
    run(&mut state, CommandKind::MoveModelBy { name: "part".into(), delta: [f64::NAN, 0.0, 0.0] });
    assert!(!state.history.is_dirty(), "refused: no step");
    assert!(state.status.contains("part not moved"), "{}", state.status);
    run(&mut state, CommandKind::MoveModelBy { name: "ghost".into(), delta: [1.0, 0.0, 0.0] });
    assert!(!state.history.is_dirty());
    assert!(state.status.contains("ghost"), "{}", state.status);
}

fn computes(state: &State) -> u64 {
    state.move_center.lock().map(|c| c.computes).unwrap_or(u64::MAX)
}

#[test]
fn a_still_frame_walks_no_facets_and_a_move_walks_them_again() {
    let mut state = part();
    assert!(handles(&state).is_some());
    let once = computes(&state);
    for _ in 0..10 {
        assert!(handles(&state).is_some());
        assert_eq!(arms_px(&state).len(), 3);
        let _ = move_drag::ball_at(&state, [3.0, 3.0]);
    }
    assert_eq!(computes(&state), once, "an unchanged document walks the facets no more");
    let before = handles(&state).map(|h| h.center).expect("handles");
    run(&mut state, CommandKind::MoveModelBy { name: "part".into(), delta: [5.0, 0.0, 0.0] });
    let after = handles(&state).map(|h| h.center).expect("handles");
    assert_eq!(computes(&state), once + 1, "a document change walks them once again");
    assert!((after[0] - before[0] - 5.0).abs() < 1e-9, "the new centre, not a stale one: {before:?} -> {after:?}");
    assert!(handles(&state).is_some());
    assert_eq!(computes(&state), once + 1);
}

/// A ray from (0, 0, -100) at `deg` degrees to the X axis, in the XZ plane.
fn ray_at_deg(deg: f64) -> Option<f64> {
    let r = deg.to_radians();
    move_drag::ray_axis_t([0.0, 0.0, -100.0], [r.cos(), 0.0, r.sin()], [0.0; 3], [1.0, 0.0, 0.0])
}

#[test]
fn an_axis_seen_almost_end_on_holds_the_drag() {
    assert_eq!(ray_at_deg(0.0), None, "along the axis");
    assert_eq!(ray_at_deg(1.0), None, "1 degree: end-on, held");
    assert_eq!(ray_at_deg(2.9), None);
    let t = ray_at_deg(10.0).expect("10 degrees: seen across");
    // The ray meets the X axis where z = 0: t = 100 / tan(10 deg).
    assert!((t - 100.0 / 10f64.to_radians().tan()).abs() < 1e-9, "{t}");
    assert!(ray_at_deg(90.0).is_some_and(|t| t.abs() < 1e-9));
    assert_eq!(move_drag::ray_axis_t([0.0; 3], [f64::NAN, 0.0, 1.0], [0.0; 3], [1.0, 0.0, 0.0]), None, "NaN refused");
    assert_eq!(move_drag::ray_axis_t([0.0; 3], [0.0; 3], [0.0; 3], [1.0, 0.0, 0.0]), None, "no direction refused");
}

#[test]
fn one_motion_step_drags_at_most_one_view_width() {
    assert_eq!(move_drag::clamp_step(2.0, 5.0, 10.0), 5.0, "inside the limit: exact");
    assert_eq!(move_drag::clamp_step(2.0, 1e6, 10.0), 12.0);
    assert_eq!(move_drag::clamp_step(2.0, -1e6, 10.0), -8.0);
    assert_eq!(move_drag::clamp_step(2.0, 5.0, f64::NAN), 2.0, "no limit: held");
    assert_eq!(move_drag::clamp_step(2.0, 5.0, 0.0), 2.0);
    // Live: an axis point ten view widths past the press (what a near
    // end-on axis gives for a px of cursor) moves the part by one view
    // width in that event, no more. The press anchor is shifted to stand in
    // for that far point, since a framed view cannot show it.
    let mut state = part();
    let h = handles(&state).expect("handles");
    let width_mm = 800.0 * h.len / move_drag::ARM_PX;
    let ball = arms_px(&state).iter().find(|a| a.0 == 1).map(|a| a.2).expect("a Y ball");
    assert!(move_drag::press(&mut state, ball));
    if let Some(d) = state.body_drag.as_mut() {
        d.start_t -= 10.0 * width_mm;
    }
    move_drag::motion(&mut state, ball);
    let d = state.body_drag.as_ref().map(|d| d.delta).expect("dragging");
    assert!((d - width_mm).abs() < 1e-6 * width_mm, "clamped to one view width {width_mm}: {d}");
}
