//! #2608: the transport's operation steps, read back through AccessKit:
//! the two buttons, their enabled state at the first and the last
//! operation and while the stock removal is calculated, a click and a key
//! that move the playhead to an operation's start and pause there, and
//! the readout that names the operation. Behaviour only: labels, roles,
//! enabled state and the position, no pixels.
//! Provenance: our own tests over our own transport bar.

use egui::accesskit::Role;
use egui_kittest::kittest::NodeT;

use super::*;

use camcraft_app::cl::{Move, MoveKind, Toolpath};
use camcraft_app::geometry::Point3;
use camcraft_app::sim::{OpStarts, Script, Traverse, script};

use crate::egui_app::sim_steps::{NEXT_KEY, NEXT_OP, PREV_KEY, PREV_OP, cached_op_starts, op_step_buttons, step_tip};
use crate::state::Sim;

fn path(x: f64) -> Toolpath {
    let p = Point3::new;
    let mut t = Toolpath::default();
    t.moves.push(Move::new(MoveKind::Rapid, p(x, 0.0, 10.0), p(x, 0.0, 5.0), None));
    t.moves.push(Move::new(MoveKind::Plunge, p(x, 0.0, 5.0), p(x, 0.0, 0.0), Some(300.0)));
    t.moves.push(Move::new(MoveKind::Feed, p(x, 0.0, 0.0), p(x + 20.0, 0.0, 0.0), Some(600.0)));
    t
}

fn three() -> Script {
    let (a, b, c) = (path(0.0), path(40.0), path(80.0));
    script([("Face1", &a), ("Contour2", &b), ("Pocket3", &c)], &Traverse::default())
}

struct Bar {
    sim: Sim,
    sc: Script,
    starts: OpStarts,
    held: bool,
}

fn bar(t: f64, held: bool) -> Harness<'static, Bar> {
    Harness::builder().with_size(Vec2::new(600.0, 40.0)).build_ui_state(
        |ui, b: &mut Bar| {
            ui.horizontal(|ui| op_step_buttons(ui, &mut b.sim, &b.sc, &b.starts, b.held));
        },
        Bar { sim: Sim { t, playing: true, speed: 1.0 }, starts: OpStarts::of(&three()), sc: three(), held },
    )
}

fn enabled(h: &Harness<'_, Bar>, label: &str) -> bool {
    !h.get_by_role_and_label(Role::Button, label).accesskit_node().is_disabled()
}

#[test]
fn next_moves_the_playhead_to_the_next_operation_and_pauses() {
    let starts = three().op_starts();
    let mut h = bar(starts[1].1 * 0.5, false);
    h.run();
    assert!(enabled(&h, PREV_OP) && enabled(&h, NEXT_OP), "both steps are live inside the first operation");
    h.get_by_role_and_label(Role::Button, NEXT_OP).click();
    h.run();
    assert!((h.state().sim.t - starts[1].1).abs() < 1e-9, "the playhead is at the start of Contour2");
    assert!(!h.state().sim.playing, "a step pauses the playback");
    h.get_by_role_and_label(Role::Button, PREV_OP).click();
    h.run();
    assert!(h.state().sim.t.abs() < 1e-9, "previous goes back to the start of Face1");
}

#[test]
fn previous_is_off_at_the_start_and_next_is_off_in_the_last_operation() {
    let mut h = bar(0.0, false);
    h.run();
    assert!(!enabled(&h, PREV_OP), "previous is off at the start of the first operation");
    assert!(enabled(&h, NEXT_OP));
    let last = three().op_starts()[2].1;
    let mut h = bar(last, false);
    h.run();
    assert!(!enabled(&h, NEXT_OP), "next is off in the last operation");
    assert!(enabled(&h, PREV_OP));
}

#[test]
fn both_steps_wait_while_the_stock_removal_is_calculated() {
    let mut h = bar(1.0, true);
    h.run();
    assert!(!enabled(&h, PREV_OP) && !enabled(&h, NEXT_OP), "the steps hold with the scrub");
}

#[test]
fn page_down_and_page_up_step_like_the_buttons() {
    let starts = three().op_starts();
    let mut h = bar(0.0, false);
    h.run();
    h.key_press(egui::Key::PageDown);
    h.run();
    assert!((h.state().sim.t - starts[1].1).abs() < 1e-9, "Page Down goes to the next operation");
    h.key_press(egui::Key::PageDown);
    h.run();
    assert!((h.state().sim.t - starts[2].1).abs() < 1e-9);
    h.key_press(egui::Key::PageUp);
    h.run();
    assert!((h.state().sim.t - starts[1].1).abs() < 1e-9, "Page Up goes back one operation");
}

#[test]
fn the_tips_say_what_each_step_does_and_name_the_key() {
    assert_eq!(step_tip(true, Some("Contour2"), false), format!("Go to the start of the next operation: Contour2\t{NEXT_KEY}"));
    assert_eq!(step_tip(false, Some("Face1"), false), format!("Go to the start of Face1\t{PREV_KEY}"));
    assert_eq!(step_tip(true, None, false), "This is the last operation");
    assert_eq!(step_tip(false, None, false), "This is the start of the first operation");
    assert!(step_tip(true, Some("Contour2"), true).contains("stock removal is calculated"));
    let mut h = bar(0.0, false);
    h.run();
    let next = h.get_by_role_and_label(Role::Button, NEXT_OP);
    let desc = next.accesskit_node().description().map(|d| d.to_string()).unwrap_or_default();
    assert!(desc.contains("Contour2"), "the AccessKit description names the next operation: {desc}");
}

#[test]
fn the_shell_transport_has_the_steps_and_names_the_operation() {
    let mut h = shell_harness();
    h.state_mut().state.sim = Some(Sim { t: 0.0, playing: false, speed: 1.0 });
    h.run_steps(3);
    let prev = h.get_by_role_and_label(Role::Button, PREV_OP);
    assert!(prev.accesskit_node().is_disabled(), "the playhead is at the start of the first operation");
    let _ = h.get_by_role_and_label(Role::Button, NEXT_OP);
    let first = crate::egui_app::shell::cached_script(&mut h.state_mut().state).ops.first().cloned().unwrap_or_default();
    assert!(!first.is_empty(), "the seeded document has an operation on the timeline");
    assert!(h.query_by_label_contains(&format!("{first} · ")).is_some(), "the readout names the current operation");
}

/// Review of PR #2763 and the #2238 frame-cost rule: the transport walks
/// a timeline's segments one time, not in each frame. Still frames give
/// the same kept starts (a counter: the same `Arc`, no new walk). A
/// document change gives a new timeline, and its starts are the ones a
/// fresh walk gives. The kept `Weak` does not hold the old script.
#[test]
fn the_starts_are_walked_once_per_timeline() {
    let mut h = shell_harness();
    h.state_mut().state.sim = Some(Sim { t: 0.0, playing: false, speed: 1.0 });
    h.run_steps(2);
    let first = cached_op_starts(&mut h.state_mut().state);
    let old_script = std::sync::Arc::downgrade(&crate::egui_app::shell::cached_script(&mut h.state_mut().state));
    h.run_steps(3);
    let still = cached_op_starts(&mut h.state_mut().state);
    assert!(std::sync::Arc::ptr_eq(&first, &still), "a still frame walks no segment");
    assert!(!first.as_slice().is_empty(), "the seeded document has operations on the timeline");
    h.state_mut().state.doc_version += 1;
    h.run_steps(2);
    let st = &mut h.state_mut().state;
    let changed = cached_op_starts(st);
    assert!(!std::sync::Arc::ptr_eq(&first, &changed), "a document change walks the new timeline");
    let fresh = OpStarts::of(&crate::egui_app::shell::cached_script(st));
    assert_eq!(*changed, fresh, "the kept starts are the fresh walk's");
    assert!(old_script.upgrade().is_none(), "the old timeline is freed");
}

