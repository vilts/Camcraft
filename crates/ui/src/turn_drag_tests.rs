//! #2047 (#1198 slice c), the turn law without a window: the dragged angle
//! is the cursor's angle on the ring plane, the turn keeps the part's
//! bounding-box centre in place, a setup that holds the part turned turns
//! it about the axis the user SEES, a press and release in place queues
//! nothing, and a bad axis or angle is refused by name with no step.
//! Provenance: our own tests; rigid-body transform algebra.
//! Gate map: covers ui::turn_drag, ui::state_move_by.

use std::f64::consts::{FRAC_PI_2, FRAC_PI_4, TAU};

use camcraft_app::commands::ImportModel;
use camcraft_app::geometry::{Point3, cube, quat_from_axis_angle, quat_mul};
use camcraft_app::model;
use camcraft_app::schema::Location;

use crate::interact::project_uv;
use crate::move_drag::{self, shown_center};
use crate::state::{CommandKind, State};
use crate::turn_drag::{self, rings, rings_px};

fn run(state: &mut State, k: CommandKind) {
    state.pending.push_back(k);
    state.drain_actions();
}

/// A 40 x 20 x 10 part, selected, Rotate mode on, framed in an 800 x 600 pane.
fn part() -> State {
    let mut state = State::new();
    let mesh = cube(Point3::new(0.0, 0.0, 0.0), Point3::new(40.0, 20.0, 10.0));
    state.history.execute(&state.world, ImportModel::from_mesh("part", mesh));
    state.viewport_rect = ([0.0, 0.0], [800.0, 600.0]);
    run(&mut state, CommandKind::FrameAll);
    run(&mut state, CommandKind::Select(Some("part".into())));
    state.rotate_handles = true;
    state
}

fn entity(state: &State) -> flecs_ecs::prelude::Entity {
    model::find_by_name(&state.world, "part").expect("part")
}

fn shown(state: &State) -> Location {
    camcraft_app::setup_align::view_pose(&state.world, &entity(state)).unwrap_or_default()
}

/// The px of the point at in-plane angle `phi` on ring `axis`.
fn ring_px(state: &State, axis: usize, phi: f64) -> [f32; 2] {
    let r = rings(state).expect("rings on the selected part");
    let (e1, e2) = ([[0.0, 1.0, 0.0], [0.0, 0.0, 1.0], [1.0, 0.0, 0.0]][axis], [[0.0, 0.0, 1.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]][axis]);
    let p: [f64; 3] = std::array::from_fn(|i| r.center[i] + r.radius * (phi.cos() * e1[i] + phi.sin() * e2[i]));
    let uv = project_uv(&state.cam.mvp(800.0 / 600.0), p).expect("in front of the camera");
    [uv[0] * 800.0, uv[1] * 600.0]
}

fn near(a: [f64; 3], b: [f64; 3], tol: f64) -> bool {
    a.iter().zip(b).all(|(x, y)| (x - y).abs() < tol)
}

#[test]
fn the_dragged_angle_is_the_ring_angle_under_the_cursor() {
    let mut state = part();
    assert_eq!(rings_px(&state).iter().map(|r| r.0).collect::<Vec<_>>(), vec![0, 1, 2], "a ring per axis");
    let from = ring_px(&state, 2, FRAC_PI_4);
    assert_eq!(turn_drag::ring_at(&state, from), Some(2), "the Z ring is under the cursor");
    assert!(turn_drag::press(&mut state, from), "the Z ring takes the press");
    let to = ring_px(&state, 2, FRAC_PI_4 + 30f64.to_radians());
    turn_drag::motion(&mut state, to);
    let a = state.body_turn.as_ref().map(|d| d.angle).expect("turning");
    assert!((a.to_degrees() - 30.0).abs() < 0.05, "30 degrees about Z, got {}", a.to_degrees());
    assert_eq!(turn_drag::readout(&state).as_deref().map(|s| s.starts_with("Turn about Z: +30.")), Some(true));
    turn_drag::release(&mut state);
    assert!(matches!(state.pending.front(), Some(CommandKind::TurnModelBy { axis: 2, angle_rad, .. }) if (*angle_rad - a).abs() < 1e-12));
    assert_eq!(state.pending.len(), 1, "one command per drag");
}

#[test]
fn a_drag_past_half_a_turn_keeps_counting() {
    let mut state = part();
    let from = ring_px(&state, 2, 0.1);
    assert!(turn_drag::press(&mut state, from));
    for k in 1..=10 {
        let to = ring_px(&state, 2, 0.1 + f64::from(k) * 0.4);
        turn_drag::motion(&mut state, to);
    }
    let a = state.body_turn.as_ref().map(|d| d.angle).expect("turning");
    assert!((a - 4.0).abs() < 1e-3, "4 rad, past pi, got {a}");
}

#[test]
fn a_press_off_the_rings_or_a_turn_in_place_queues_nothing() {
    let mut state = part();
    assert!(!turn_drag::press(&mut state, [3.0, 3.0]));
    assert!(state.body_turn.is_none());
    let at = ring_px(&state, 0, 1.0);
    assert!(turn_drag::press(&mut state, at));
    turn_drag::motion(&mut state, at);
    turn_drag::release(&mut state);
    assert!(state.pending.is_empty(), "no turn, no command, no rotation noise");
    state.rotate_handles = false;
    assert!(rings(&state).is_none(), "Rotate off: nothing to grab");
    assert!(move_drag::handles(&state).is_none(), "and no move handles either");
}

#[test]
fn a_turn_keeps_the_part_centre_in_place() {
    let mut state = part();
    let c0 = shown_center(&state, &entity(&state)).expect("centre");
    run(&mut state, CommandKind::TurnModelBy { name: "part".into(), axis: 2, angle_rad: FRAC_PI_2 });
    let c1 = shown_center(&state, &entity(&state)).expect("centre");
    assert!(near(c0, c1, 1e-9), "turned about its centre: {c0:?} -> {c1:?}");
    let rot = shown(&state).rot;
    let want = quat_from_axis_angle(2, FRAC_PI_2);
    assert!(rot.iter().zip(want).all(|(a, b)| (a - b).abs() < 1e-12), "{rot:?}");
    assert!(state.status.starts_with("turned part +90.0\u{b0} about Z"), "{}", state.status);
}

#[test]
fn a_turned_setup_turns_the_part_about_the_axis_it_shows() {
    let mut state = part();
    run(&mut state, CommandKind::CreateSetup { name: None, datum_from: None });
    run(&mut state, CommandKind::ActivateSetup(Some("Setup1".into())));
    run(&mut state, CommandKind::AlignPart { setup: "Setup1".into(), angles: [0.0, 0.0, 90.0], pivot: Default::default() });
    let (view0, c0) = (shown(&state), shown_center(&state, &entity(&state)).expect("centre"));
    run(&mut state, CommandKind::TurnModelBy { name: "part".into(), axis: 0, angle_rad: FRAC_PI_2 });
    let (view1, c1) = (shown(&state), shown_center(&state, &entity(&state)).expect("centre"));
    assert!(near(c0, c1, 1e-9), "the shown centre stays: {c0:?} -> {c1:?}");
    let want = quat_mul(quat_from_axis_angle(0, FRAC_PI_2), view0.rot);
    let same = |s: f64| view1.rot.iter().zip(want).all(|(a, b)| (a - s * b).abs() < 1e-9);
    assert!(same(1.0) || same(-1.0), "a quarter turn about the SHOWN X: {view0:?} -> {view1:?}");
}

fn computes(state: &State) -> u64 {
    state.move_center.lock().map(|c| c.computes).unwrap_or(u64::MAX)
}

#[test]
fn a_still_frame_walks_no_facets_and_a_turn_walks_them_again() {
    let mut state = part();
    assert!(rings(&state).is_some());
    let once = computes(&state);
    for _ in 0..10 {
        assert_eq!(rings_px(&state).len(), 3);
        let _ = turn_drag::ring_at(&state, [3.0, 3.0]);
    }
    assert_eq!(computes(&state), once, "an unchanged document walks the facets no more");
    run(&mut state, CommandKind::TurnModelBy { name: "part".into(), axis: 2, angle_rad: FRAC_PI_2 });
    assert!(rings(&state).is_some());
    assert_eq!(computes(&state), once + 1, "a turn walks them once again");
    assert_eq!(rings_px(&state).len(), 3);
    assert_eq!(computes(&state), once + 1, "and then holds again");
}

#[test]
fn a_cursor_on_the_pivot_holds_the_turn() {
    let mut state = part();
    let (from, to) = (ring_px(&state, 2, 0.3), ring_px(&state, 2, 0.8));
    assert!(turn_drag::press(&mut state, from));
    turn_drag::motion(&mut state, to);
    let held = state.body_turn.clone().expect("turning");
    let c = rings(&state).map(|r| r.center).expect("rings");
    let uv = project_uv(&state.cam.mvp(800.0 / 600.0), c).expect("in front of the camera");
    turn_drag::motion(&mut state, [uv[0] * 800.0, uv[1] * 600.0]);
    assert_eq!(state.body_turn.as_ref(), Some(&held), "no angle at the pivot: the turn holds");
    assert!(held.angle.is_finite() && (held.angle - 0.5).abs() < 1e-3, "{}", held.angle);
}

#[test]
fn a_bad_axis_or_angle_is_refused_by_name() {
    let mut state = part();
    state.history.mark_clean();
    run(&mut state, CommandKind::TurnModelBy { name: "part".into(), axis: 2, angle_rad: f64::NAN });
    assert!(!state.history.is_dirty(), "refused: no step");
    assert!(state.status.contains("part not turned"), "{}", state.status);
    run(&mut state, CommandKind::TurnModelBy { name: "part".into(), axis: 3, angle_rad: 1.0 });
    assert!(!state.history.is_dirty());
    assert!(state.status.contains("axis 3"), "{}", state.status);
    run(&mut state, CommandKind::TurnModelBy { name: "ghost".into(), axis: 2, angle_rad: TAU / 8.0 });
    assert!(!state.history.is_dirty());
    assert!(state.status.contains("ghost"), "{}", state.status);
}
