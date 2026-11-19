//! #1198 slice b: drag a body by its move handles. With the bar's Move
//! mode on, the selected part wears three axis balls; a drag on a ball is a
//! preview while the button is down and ONE undoable `MoveModel` on
//! release, with the dragged distance. Escape mid-drag and a drag that
//! ends where it began add no history step; empty space still orbits.
//! Provenance: our own viewport tests over our own drag law; no vendor
//! material consulted.

use camcraft_app::geometry::Point3;

use super::*;
use crate::move_drag;

fn block_pos(sh: &ShellUi) -> Point3 {
    let e = model::find_by_name(&sh.state.world, "Block").expect("seed has Block");
    model::transform_of(&sh.state.world, &e).unwrap_or_default().pos
}

fn press_escape(h: &mut Harness<'static, ShellUi>) {
    h.input_mut().events.push(Event::Key {
        key: Key::Escape,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: Modifiers::NONE,
    });
    h.run();
}

/// Move mode on (the bar button), the Block selected (a viewport click).
fn moving_block() -> Harness<'static, ShellUi> {
    let mut h = shell_harness();
    h.run();
    let nav = hud(h.state()).nav_rect(viewcube::Nav::Move).center();
    click_viewport(&mut h, nav);
    assert!(h.state().state.move_handles, "the bar's Move button turns the mode on");
    assert!(h.state().state.pending.is_empty(), "a mode toggle is view state, not a command");
    let c = vp_center(h.state());
    click_viewport(&mut h, c);
    assert_eq!(h.state().state.status, "selected Block");
    h.state_mut().state.history.mark_clean();
    h
}

/// The X ball and the gizmo centre, px.
fn x_ball(h: &Harness<'static, ShellUi>) -> (egui::Pos2, egui::Pos2) {
    let arms = move_drag::arms_px(&h.state().state);
    let (_, c, b) = arms.iter().copied().find(|(a, _, _)| *a == 0).expect("an X arm");
    (egui::pos2(c[0], c[1]), egui::pos2(b[0], b[1]))
}

#[test]
fn handles_show_only_in_move_mode_on_a_selected_part() {
    let mut h = shell_harness();
    h.run();
    let c = vp_center(h.state());
    click_viewport(&mut h, c);
    assert!(move_drag::arms_px(&h.state().state).is_empty(), "Move off: no handles");
    let nav = hud(h.state()).nav_rect(viewcube::Nav::Move).center();
    click_viewport(&mut h, nav);
    let arms = move_drag::arms_px(&h.state().state);
    assert_eq!(arms.iter().map(|a| a.0).collect::<Vec<_>>(), vec![0, 1, 2], "a ball per axis");
    for (_, c, b) in &arms {
        let len = (b[0] - c[0]).hypot(b[1] - c[1]);
        assert!(len > 10.0 && len <= move_drag::ARM_PX as f32 + 1.0, "arm on screen: {len}");
    }
    // Zoom far out: the arms keep their screen size.
    h.state_mut().state.cam.dist *= 4.0;
    h.run();
    let far = move_drag::arms_px(&h.state().state);
    let longest = |a: &[(usize, [f32; 2], [f32; 2])]| a.iter().map(|(_, c, b)| (b[0] - c[0]).hypot(b[1] - c[1])).fold(0.0f32, f32::max);
    assert!((longest(&far) - longest(&arms)).abs() < 3.0, "constant size: {} vs {}", longest(&far), longest(&arms));
    click_viewport(&mut h, nav);
    assert!(move_drag::arms_px(&h.state().state).is_empty(), "Move off again: gone");
}

#[test]
fn a_ball_drag_is_one_undoable_move_by_the_dragged_distance() {
    let mut h = moving_block();
    let before = block_pos(h.state());
    let (c, b) = x_ball(&h);
    set_pos(&mut h, b);
    button_at(&mut h, egui::PointerButton::Primary, b, true);
    assert!(h.state().state.body_drag.is_some(), "the ball takes the press");
    let to = b + (b - c) * 0.5;
    set_pos(&mut h, to);
    let dragged = h.state().state.body_drag.as_ref().map(|d| d.delta).expect("still dragging");
    assert!(dragged > 0.0, "dragging outward along +X: {dragged}");
    assert_eq!(h.state().state.drag, None, "the ball drag never orbits");
    let mid = block_pos(h.state());
    assert!((mid.x - before.x).abs() < 1e-12, "the preview never writes the world");
    assert!(!h.state().state.history.is_dirty(), "no history while dragging");
    button_at(&mut h, egui::PointerButton::Primary, to, false);
    h.run_steps(2);
    let st = &h.state().state;
    assert!(st.body_drag.is_none());
    let after = block_pos(h.state());
    assert!((after.x - before.x - dragged).abs() < 1e-9, "moved by the dragged {dragged}: {before:?} -> {after:?}");
    assert!((after.y - before.y).abs() < 1e-12 && (after.z - before.z).abs() < 1e-12, "along X only");
    assert!(st.history.is_dirty(), "the release is a history step");
    assert!(st.status.starts_with("moved Block by X +"), "{}", st.status);
    // Exactly ONE step: one undo brings the history back to clean.
    h.state_mut().state.pending.push_back(CommandKind::Undo);
    h.run_steps(2);
    let st = &h.state().state;
    assert!(!st.history.is_dirty(), "one drag, one undo step");
    let back = block_pos(h.state());
    assert!((back.x - before.x).abs() < 1e-12, "undo puts it back");
}

#[test]
fn escape_mid_drag_cancels_with_no_history_step() {
    let mut h = moving_block();
    let before = block_pos(h.state());
    let (c, b) = x_ball(&h);
    set_pos(&mut h, b);
    button_at(&mut h, egui::PointerButton::Primary, b, true);
    let to = b + (b - c) * 0.5;
    set_pos(&mut h, to);
    assert!(h.state().state.body_drag.is_some());
    press_escape(&mut h);
    assert!(h.state().state.body_drag.is_none(), "Escape drops the drag");
    button_at(&mut h, egui::PointerButton::Primary, to, false);
    h.run_steps(2);
    let st = &h.state().state;
    assert!(!st.history.is_dirty(), "Escape queues nothing");
    assert!((block_pos(h.state()).x - before.x).abs() < 1e-12, "the part never moved");
}

#[test]
fn a_drag_that_ends_where_it_began_adds_nothing() {
    let mut h = moving_block();
    let (_, b) = x_ball(&h);
    h.state_mut().state.status = "before".into();
    set_pos(&mut h, b);
    button_at(&mut h, egui::PointerButton::Primary, b, true);
    button_at(&mut h, egui::PointerButton::Primary, b, false);
    h.run_steps(2);
    let st = &h.state().state;
    assert!(!st.history.is_dirty(), "a zero drag is no step");
    assert_eq!(st.status, "before", "a press on a ball is not a pick either");
}

#[test]
fn empty_space_still_orbits_in_move_mode() {
    let mut h = moving_block();
    let ([x0, y0], _) = h.state().state.viewport_rect;
    let p = egui::pos2(x0 + 60.0, y0 + 60.0);
    assert!(move_drag::ball_at(&h.state().state, [p.x, p.y]).is_none());
    set_pos(&mut h, p);
    button_at(&mut h, egui::PointerButton::Primary, p, true);
    set_pos(&mut h, p + egui::vec2(25.0, 0.0));
    assert_eq!(h.state().state.drag, Some(DragMode::Orbit), "away from a ball the drag orbits");
    assert!(h.state().state.body_drag.is_none());
    button_at(&mut h, egui::PointerButton::Primary, p + egui::vec2(25.0, 0.0), false);
    h.run_steps(2);
    assert!(!h.state().state.history.is_dirty());
}

#[test]
fn focus_loss_mid_drag_cancels_and_a_later_release_sends_no_move() {
    let mut h = moving_block();
    let before = block_pos(h.state());
    let (c, b) = x_ball(&h);
    set_pos(&mut h, b);
    button_at(&mut h, egui::PointerButton::Primary, b, true);
    let to = b + (b - c) * 0.5;
    set_pos(&mut h, to);
    assert!(h.state().state.body_drag.is_some());
    h.input_mut().events.push(Event::WindowFocused(false));
    h.run();
    assert!(h.state().state.body_drag.is_none(), "focus loss drops the drag");
    h.input_mut().events.push(Event::WindowFocused(true));
    button_at(&mut h, egui::PointerButton::Primary, to, false);
    h.run_steps(2);
    let st = &h.state().state;
    assert!(!st.history.is_dirty(), "the later release queues nothing");
    assert!((block_pos(h.state()).x - before.x).abs() < 1e-12, "the part never moved");
}
