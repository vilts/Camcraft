//! #2019: the world gizmo follows the machine kind. A mill draws X, Y and
//! Z. A lathe draws Z and X only, and Y only when its file states a Y
//! axis. A mill-turn keeps the milling frame and hides Y only when its
//! stated axes have none. Display only: no command, no history entry.
//! Provenance: our own tests over our own shell; no HSMWorks material.

use super::*;
use crate::view_frame::{ViewFrame, axes_shown, shown_for};
use camcraft_app::machine::{self, AxisLimits, Kind, Machine};

fn ax(name: &str) -> AxisLimits {
    AxisLimits { name: name.into(), travel: Some((-10.0, 10.0)), ..AxisLimits::default() }
}

fn machine_of(kind: &str, axes: &[&str]) -> Machine {
    Machine {
        model: "Test".into(),
        kind: Some(kind.into()),
        axes: axes.iter().map(|n| ax(n)).collect(),
        ..Machine::default()
    }
}

/// The axis letters the viewport labels, by the shell's own producer.
fn letters(h: &Harness<'static, ShellUi>) -> Vec<String> {
    crate::interact::overlay_labels(&h.state().state, 1.5)
        .into_iter()
        .map(|(_, _, t)| t)
        .filter(|t| crate::interact::is_axis_label(t))
        .collect()
}

fn lines(h: &Harness<'static, ShellUi>) -> usize {
    crate::interact::gizmo_segments(&h.state().state, 1.5).len()
}

fn mount(h: &mut Harness<'static, ShellUi>, m: &Machine) {
    machine::set(&h.state().state.world, "t.mch", Some(m));
    h.run();
}

#[test]
fn a_mill_shows_x_y_and_z() {
    let mut h = shell_harness();
    mount(&mut h, &machine_of("mill", &["X", "Y", "Z"]));
    assert_eq!(axes_shown(&h.state().state), [true, true, true]);
    let l = letters(&h);
    for want in ["X", "Y", "Z"] {
        assert!(l.iter().any(|t| t == want), "{want} drawn: {l:?}");
    }
    assert_eq!(lines(&h), 3, "one triad of three lines");
}

#[test]
fn a_lathe_without_y_shows_z_and_x_only() {
    let mut h = shell_harness();
    let undo = h.state().state.history.can_undo();
    mount(&mut h, &machine_of("lathe", &["X", "Z"]));
    assert_eq!(h.state().state.cam.frame, ViewFrame::Lathe);
    assert_eq!(axes_shown(&h.state().state), [true, false, true]);
    let l = letters(&h);
    assert!(l.iter().any(|t| t == "X") && l.iter().any(|t| t == "Z"), "{l:?}");
    assert!(!l.iter().any(|t| t == "Y"), "no Y letter on a lathe without Y: {l:?}");
    assert_eq!(lines(&h), 2, "two axis lines, no Y line");
    assert_eq!(h.state().state.history.can_undo(), undo, "display only, never a command");
}

#[test]
fn a_lathe_with_a_y_axis_shows_y_too() {
    let mut h = shell_harness();
    mount(&mut h, &machine_of("lathe", &["X", "Y", "Z"]));
    assert_eq!(axes_shown(&h.state().state), [true, true, true]);
    assert!(letters(&h).iter().any(|t| t == "Y"));
    assert_eq!(lines(&h), 3);
}

#[test]
fn a_mill_again_brings_y_back() {
    let mut h = shell_harness();
    mount(&mut h, &machine_of("lathe", &["X", "Z"]));
    assert_eq!(lines(&h), 2);
    mount(&mut h, &machine_of("mill", &["X", "Y", "Z"]));
    assert_eq!(lines(&h), 3);
}

#[test]
fn the_mill_turn_rule_keeps_the_milling_frame_and_follows_the_y_axis() {
    // #2019 decision: a mill-turn is viewed as a mill; Y hides only when
    // its stated axes have no linear Y.
    assert_eq!(ViewFrame::for_kind(Kind::MillTurn), ViewFrame::Mill);
    assert_eq!(shown_for(Kind::MillTurn, true, false), [true, true, true]);
    assert_eq!(shown_for(Kind::MillTurn, false, false), [true, false, true]);
    assert_eq!(shown_for(Kind::MillTurn, false, true), [true, true, true], "unstated hides nothing");
    assert_eq!(shown_for(Kind::Lathe, false, true), [true, false, true]);
    assert_eq!(shown_for(Kind::Mill, false, false), [true, true, true]);
}
