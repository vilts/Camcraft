//! #2047 / #2624: turn a body by its rings. With the bar's Move
//! gizmo on, the selected part wears arrows and three rings; a ring drag is a
//! preview while the button is down and ONE undoable `MoveModel` on
//! release, with the dragged angle shown above the bar. The turn makes the
//! generated operation stale, so the post refuses until a regenerate; the
//! stock follows the part. Escape mid-drag and a drag that ends at its
//! start angle add no history step; a focus loss or a pointer that leaves
//! the window drops the turn; empty space still orbits.
//! Provenance: our own viewport tests over our own drag law; no vendor
//! material consulted.
//! Gate map: covers ui::turn_drag, ui::egui_app::turn_handles, ui::egui_app::shell.

use std::f64::consts::FRAC_PI_4;

use camcraft_app::schema::Location;
use egui_kittest::kittest::NodeT;

use super::*;
use crate::interact::project_uv;
use crate::move_drag;
use crate::turn_drag;

fn block(sh: &ShellUi) -> Location {
    let e = model::find_by_name(&sh.state.world, "Block").expect("seed has Block");
    model::transform_of(&sh.state.world, &e).unwrap_or_default()
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

/// The combined Move gizmo on, the Block selected by a viewport click.
fn turning_block() -> Harness<'static, ShellUi> {
    let mut h = shell_harness();
    h.run();
    let nav = hud(h.state()).nav_rect(viewcube::Nav::Move).center();
    click_viewport(&mut h, nav);
    assert!(h.state().state.rotate_handles && h.state().state.move_handles, "Move shows rings and arrows together");
    assert!(h.state().state.pending.is_empty(), "a mode toggle is view state, not a command");
    let c = vp_center(h.state());
    click_viewport(&mut h, c);
    assert_eq!(h.state().state.status, "selected Block");
    assert_eq!(turn_drag::rings_px(&h.state().state).len(), 3, "one ring per axis");
    assert_eq!(move_drag::arms_px(&h.state().state).len(), 3, "arrows stay visible beside the rings");
    h.get_by_label("Rotate value").click();
    h.run();
    assert!(h.state().state.rotate_handles && h.state().state.move_handles, "choosing a value does not hide handles");
    h.state_mut().state.history.mark_clean();
    h
}

/// The px of the point at in-plane angle `phi` on the Z ring (X toward Y).
fn z_ring(h: &Harness<'static, ShellUi>, phi: f64) -> egui::Pos2 {
    let st = &h.state().state;
    let r = turn_drag::rings(st).expect("rings on the Block");
    let p = [r.center[0] + r.radius * phi.cos(), r.center[1] + r.radius * phi.sin(), r.center[2]];
    let mvp = st.cam.mvp(move_drag::aspect(st).expect("a pane"));
    let px = move_drag::to_px(st, project_uv(&mvp, p).expect("in front of the camera"));
    egui::pos2(px[0], px[1])
}

fn world_box(st: &State, name: &str) -> ([f64; 3], [f64; 3]) {
    let e = model::find_by_name(&st.world, name).expect("named body");
    let tris = model::world_facets(&st.world, &e).expect("facets");
    let mut lo = [f64::INFINITY; 3];
    let mut hi = [f64::NEG_INFINITY; 3];
    for p in tris.iter().flat_map(|t| [t.a, t.b, t.c]) {
        for (i, v) in [p.x, p.y, p.z].into_iter().enumerate() {
            lo[i] = lo[i].min(v);
            hi[i] = hi[i].max(v);
        }
    }
    (lo, hi)
}

#[test]
fn rings_show_only_in_rotate_mode_at_a_constant_size() {
    let mut h = shell_harness();
    h.run();
    let c = vp_center(h.state());
    click_viewport(&mut h, c);
    assert!(turn_drag::rings_px(&h.state().state).is_empty(), "Rotate off: no rings");
    let nav = hud(h.state()).nav_rect(viewcube::Nav::Rotate).center();
    click_viewport(&mut h, nav);
    let rings = turn_drag::rings_px(&h.state().state);
    assert_eq!(rings.iter().map(|r| r.0).collect::<Vec<_>>(), vec![0, 1, 2], "a ring per axis");
    assert!(move_drag::arms_px(&h.state().state).is_empty(), "no move balls in Rotate mode");
    let span = |rs: &[(usize, Vec<[f32; 2]>)]| {
        let xs = rs.iter().flat_map(|(_, p)| p.iter().map(|q| q[0]));
        let (lo, hi) = xs.fold((f32::MAX, f32::MIN), |(l, u), x| (l.min(x), u.max(x)));
        hi - lo
    };
    let near = span(&rings);
    assert!(near > 40.0 && near <= 2.0 * turn_drag::RING_PX as f32 + 2.0, "rings on screen: {near}");
    // Zoom far out: the rings keep their screen size.
    h.state_mut().state.cam.dist *= 4.0;
    h.run();
    let far = span(&turn_drag::rings_px(&h.state().state));
    assert!((far - near).abs() < 4.0, "constant size: {far} vs {near}");
    click_viewport(&mut h, nav);
    assert!(turn_drag::rings_px(&h.state().state).is_empty(), "Rotate off again: gone");
}

#[test]
fn a_ring_drag_is_one_undoable_turn_that_stales_the_path_and_carries_the_stock() {
    let mut h = turning_block();
    assert!(camcraft_app::posting::program_ops(&h.state().state.world).is_ok(), "the seed posts before the turn");
    let before = block(h.state());
    let entity = model::find_by_name(&h.state().state.world, "Block").expect("Block");
    let centre_before = move_drag::shown_center(&h.state().state, &entity).expect("part centre");
    let from = z_ring(&h, FRAC_PI_4);
    assert_eq!(turn_drag::ring_at(&h.state().state, [from.x, from.y]), Some(2), "the Z ring is under the cursor");
    set_pos(&mut h, from);
    button_at(&mut h, egui::PointerButton::Primary, from, true);
    assert!(h.state().state.body_turn.is_some(), "the ring takes the press");
    let to = z_ring(&h, FRAC_PI_4 + 20f64.to_radians());
    set_pos(&mut h, to);
    let turned = h.state().state.body_turn.as_ref().map(|d| d.angle).expect("still turning");
    assert!((turned.to_degrees() - 20.0).abs() < 0.5, "about 20 degrees: {}", turned.to_degrees());
    assert_eq!(h.state().state.drag, None, "the ring drag never orbits");
    assert_eq!(block(h.state()), before, "the preview never writes the world");
    assert!(!h.state().state.history.is_dirty(), "no history while dragging");
    // The angle is painted above the bar while the button is down.
    h.run();
    let texts = super::structural::painted(&h);
    assert!(texts.iter().any(|(t, _)| t.starts_with("Turn about Z: +")), "the bar shows the angle");
    button_at(&mut h, egui::PointerButton::Primary, to, false);
    h.run_steps(2);
    let st = &h.state().state;
    assert!(st.body_turn.is_none());
    assert!(st.history.is_dirty(), "the release is a history step");
    assert!(st.status.starts_with("turned Block +"), "{}", st.status);
    assert_ne!(block(h.state()).rot, before.rot, "the Block turned");
    let want = camcraft_app::geometry::quat_mul(
        camcraft_app::geometry::quat_from_axis_angle(2, 20f64.to_radians()), before.rot,
    );
    let actual = block(h.state()).rot;
    assert!(actual.iter().zip(want).all(|(a, b)| (a - b).abs() < 0.005), "drag turns about Z by 20 degrees: {actual:?}");
    let centre_after = move_drag::shown_center(st, &entity).expect("turned part centre");
    assert!(centre_before.iter().zip(centre_after).all(|(a, b)| (a - b).abs() < 1e-8), "the ring drag preserves the part centre");
    // The turn made the generated path stale: the post refuses by name.
    let refusal = camcraft_app::posting::program_ops(&st.world).err().unwrap_or_default();
    assert!(refusal.contains("regenerate"), "stale after the turn: {refusal:?}");
    // The stock follows: it still holds the turned part.
    let stock = camcraft_app::setup::stock_entity_name(&st.world, None);
    let ((plo, phi), (slo, shi)) = (world_box(st, "Block"), world_box(st, &stock));
    assert!((0..3).all(|i| slo[i] <= plo[i] + 1e-6 && shi[i] >= phi[i] - 1e-6), "stock {slo:?}..{shi:?} holds part {plo:?}..{phi:?}");
    // Exactly ONE step: one undo brings the history back to clean.
    h.state_mut().state.pending.push_back(CommandKind::Undo);
    h.run_steps(2);
    let st = &h.state().state;
    assert!(!st.history.is_dirty(), "one drag, one undo step");
    assert_eq!(block(h.state()), before, "undo puts it back");
    assert!(camcraft_app::posting::program_ops(&st.world).is_ok(), "undo makes the path fresh again");
}

#[test]
fn escape_mid_turn_cancels_with_no_history_step() {
    let mut h = turning_block();
    let before = block(h.state());
    let from = z_ring(&h, FRAC_PI_4);
    set_pos(&mut h, from);
    button_at(&mut h, egui::PointerButton::Primary, from, true);
    let to = z_ring(&h, FRAC_PI_4 + 0.5);
    set_pos(&mut h, to);
    assert!(h.state().state.body_turn.is_some());
    press_escape(&mut h);
    assert!(h.state().state.body_turn.is_none(), "Escape drops the turn");
    button_at(&mut h, egui::PointerButton::Primary, to, false);
    h.run_steps(2);
    assert!(!h.state().state.history.is_dirty(), "Escape queues nothing");
    assert_eq!(block(h.state()), before, "the part never turned");
}

#[test]
fn focus_loss_mid_turn_cancels_and_a_later_release_sends_no_turn() {
    let mut h = turning_block();
    let before = block(h.state());
    let from = z_ring(&h, FRAC_PI_4);
    set_pos(&mut h, from);
    button_at(&mut h, egui::PointerButton::Primary, from, true);
    let to = z_ring(&h, FRAC_PI_4 + 0.5);
    set_pos(&mut h, to);
    assert!(h.state().state.body_turn.is_some());
    h.input_mut().events.push(Event::WindowFocused(false));
    h.run();
    assert!(h.state().state.body_turn.is_none(), "focus loss drops the turn");
    h.input_mut().events.push(Event::WindowFocused(true));
    button_at(&mut h, egui::PointerButton::Primary, to, false);
    h.run_steps(2);
    assert!(!h.state().state.history.is_dirty(), "the later release queues nothing");
    assert_eq!(block(h.state()), before, "the part never turned");
    // The pointer that leaves the window mid-turn drops it too.
    set_pos(&mut h, from);
    button_at(&mut h, egui::PointerButton::Primary, from, true);
    set_pos(&mut h, to);
    assert!(h.state().state.body_turn.is_some());
    h.input_mut().events.push(Event::PointerGone);
    h.run();
    assert!(h.state().state.body_turn.is_none(), "pointer gone drops the turn");
    button_at(&mut h, egui::PointerButton::Primary, to, false);
    h.run_steps(2);
    assert!(!h.state().state.history.is_dirty());
    assert_eq!(block(h.state()), before);
}

#[test]
fn a_turn_that_ends_at_its_start_adds_nothing() {
    let mut h = turning_block();
    let before = block(h.state());
    let from = z_ring(&h, FRAC_PI_4);
    h.state_mut().state.status = "before".into();
    set_pos(&mut h, from);
    button_at(&mut h, egui::PointerButton::Primary, from, true);
    button_at(&mut h, egui::PointerButton::Primary, from, false);
    h.run_steps(2);
    let st = &h.state().state;
    assert!(!st.history.is_dirty(), "a zero turn is no step");
    assert_eq!(st.status, "before", "a press on a ring is not a pick either");
    assert_eq!(block(h.state()).rot.map(f64::to_bits), before.rot.map(f64::to_bits), "no rotation noise");
}

#[test]
fn empty_space_still_orbits_in_rotate_mode() {
    let mut h = turning_block();
    let ([x0, y0], _) = h.state().state.viewport_rect;
    let p = egui::pos2(x0 + 60.0, y0 + 60.0);
    assert!(turn_drag::ring_at(&h.state().state, [p.x, p.y]).is_none());
    set_pos(&mut h, p);
    button_at(&mut h, egui::PointerButton::Primary, p, true);
    set_pos(&mut h, p + egui::vec2(25.0, 0.0));
    assert_eq!(h.state().state.drag, Some(DragMode::Orbit), "away from a ring the drag orbits");
    assert!(h.state().state.body_turn.is_none());
    button_at(&mut h, egui::PointerButton::Primary, p + egui::vec2(25.0, 0.0), false);
    h.run_steps(2);
    assert!(!h.state().state.history.is_dirty());
}

#[test]
fn shift_drag_snaps_to_the_bar_angle_step() {
    let mut h = turning_block();
    assert!(h.query_by_label("Rotate snap step").is_some(), "the bar shows the rotate snap step");
    assert!(h.query_by_label("Rotate angle").is_some(), "the bar shows the angle field");
    let before = block(h.state());
    let from = z_ring(&h, FRAC_PI_4);
    h.input_mut().events.push(Event::ModifiersChanged(Modifiers::SHIFT));
    h.input_mut().events.push(Event::PointerMoved(from));
    h.run();
    h.input_mut().events.push(Event::PointerButton {
        pos: from,
        button: egui::PointerButton::Primary,
        pressed: true,
        modifiers: Modifiers::SHIFT,
    });
    h.run();
    let to = z_ring(&h, FRAC_PI_4 + 0.9);
    h.input_mut().events.push(Event::PointerMoved(to));
    h.run();
    let turned = h.state().state.body_turn.as_ref().map(|d| d.angle).expect("still turning");
    assert!(
        (turned.to_degrees() - 90.0).abs() < 1e-9,
        "Shift snaps ~51.6 deg to the default 90 deg step, got {}",
        turned.to_degrees()
    );
    h.input_mut().events.push(Event::PointerButton {
        pos: to,
        button: egui::PointerButton::Primary,
        pressed: false,
        modifiers: Modifiers::SHIFT,
    });
    h.run();
    h.input_mut().events.push(Event::ModifiersChanged(Modifiers::NONE));
    h.run_steps(2);
    assert_ne!(block(h.state()).rot, before.rot);
    assert!(h.state().state.status.contains("+90.0\u{b0}"), "{}", h.state().state.status);
    assert_eq!(h.state().state.handle_bar.rotate_draft, "+90.0", "the angle stays in the bar");
    assert!(h.state().state.history.is_dirty());
    h.state_mut().state.pending.push_back(CommandKind::Undo);
    h.run_steps(2);
    assert!(!h.state().state.history.is_dirty());
    assert_eq!(block(h.state()), before);
}

#[test]
fn changing_snap_step_in_the_bar_changes_shift_drag_snap() {
    let mut h = turning_block();
    h.get_by_label("Rotate snap step").click();
    h.run();
    h.get_by_label("Rotate snap step").type_text("15");
    h.run();
    h.input_mut().events.push(Event::Key {
        key: Key::Enter,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: Modifiers::NONE,
    });
    h.run();
    assert!((h.state().state.handle_bar.rotate_snap_deg - 15.0).abs() < 1e-6);
    let from = z_ring(&h, FRAC_PI_4);
    h.input_mut().events.push(Event::ModifiersChanged(Modifiers::SHIFT));
    h.input_mut().events.push(Event::PointerMoved(from));
    h.run();
    h.input_mut().events.push(Event::PointerButton {
        pos: from,
        button: egui::PointerButton::Primary,
        pressed: true,
        modifiers: Modifiers::SHIFT,
    });
    h.run();
    let to = z_ring(&h, FRAC_PI_4 + 0.9);
    h.input_mut().events.push(Event::PointerMoved(to));
    h.run();
    let turned = h.state().state.body_turn.as_ref().map(|d| d.angle.to_degrees()).expect("turning");
    assert!((turned - 45.0).abs() < 1e-6, "15 deg step snaps ~51.6 deg to 45.0, got {turned}");
}

#[test]
fn typing_an_angle_in_the_bar_turns_the_part_about_the_chosen_axis_as_one_undo_step() {
    let mut h = turning_block();
    let before = block(h.state());
    h.get_by_label("Axis Z").click();
    h.run();
    h.get_by_label("Rotate angle").click();
    h.run();
    h.get_by_label("Rotate angle").type_text("45");
    h.run();
    h.get_by_label("Set").click();
    h.run_steps(2);
    assert_ne!(block(h.state()).rot, before.rot, "the Block turned");
    let refusal = camcraft_app::posting::program_ops(&h.state().state.world).err().unwrap_or_default();
    assert!(refusal.contains("regenerate"), "stale after typed turn: {refusal:?}");
    assert!(h.state().state.history.is_dirty());
    h.state_mut().state.pending.push_back(CommandKind::Undo);
    h.run_steps(2);
    assert!(!h.state().state.history.is_dirty(), "one typed turn, one undo step");
    assert_eq!(block(h.state()), before);
    assert!(camcraft_app::posting::program_ops(&h.state().state.world).is_ok());
}

#[test]
fn combined_gizmo_exposes_each_handle_and_keeps_both_value_editors_reachable() {
    let mut h = turning_block();
    for axis in ["X", "Y", "Z"] {
        for label in [format!("Move along {axis}"), format!("Rotate about {axis}")] {
            let node = h.get_by_role_and_label(egui::accesskit::Role::Button, &label);
            assert!(!node.accesskit_node().is_disabled(), "{label} is available in the isometric view");
        }
    }
    assert!(h.query_by_label("Rotate angle").is_some());
    h.get_by_label("Move value").click();
    h.run();
    assert!(h.query_by_label("Move distance").is_some());
    assert!(h.query_by_label("Move snap step").is_some());
    assert_eq!(turn_drag::rings_px(&h.state().state).len(), 3);
    assert_eq!(move_drag::arms_px(&h.state().state).len(), 3);
}

#[test]
fn edge_on_ring_accessibility_is_disabled_while_the_visible_ring_stays_enabled() {
    let mut h = turning_block();
    let st = &mut h.state_mut().state;
    st.cam = st.cam.toward([0.0, 0.0, 1.0]);
    st.cam.projection = crate::viewport::ProjectionMode::Orthographic;
    h.run();
    for axis in ["X", "Y"] {
        let label = format!("Rotate about {axis}");
        let node = h.get_by_role_and_label(egui::accesskit::Role::Button, &label);
        assert!(node.accesskit_node().is_disabled(), "edge-on ring {axis} cannot start a turn");
    }
    let z = h.get_by_role_and_label(egui::accesskit::Role::Button, "Rotate about Z");
    assert!(!z.accesskit_node().is_disabled(), "the face-on Z ring still works");
}
