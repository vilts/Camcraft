//! #375 in the viewport: the origin triad notes an inverted machine axis
//! beside its letter, once, and keeps its right-handed arrows; a
//! standard machine adds no label.
//!
//! Provenance: our own test over ISO 841's right-handed program frame.

use camcraft_app::commands::LoadMachine;
use camcraft_app::machine::{AxisDirection, AxisLimits, Machine};

use crate::interact::{gizmo_segments, overlay_labels};
use crate::state::State;

fn mounted(inverted: &[&str]) -> State {
    let axis = |n: &str| AxisLimits {
        name: n.into(),
        travel: Some((-100.0, 100.0)),
        direction: if inverted.contains(&n) { AxisDirection::Inverted } else { AxisDirection::Standard },
        ..Default::default()
    };
    let m = Machine { model: "Flip 1".into(), axes: vec![axis("X"), axis("Y"), axis("Z")], ..Default::default() };
    let mut state = State::new();
    let load = LoadMachine::new(&state.world, "flip.mch".into(), Some(m)).expect("load");
    state.history.execute(&state.world, load);
    state
}

fn texts(state: &State) -> Vec<String> {
    overlay_labels(state, 800.0 / 600.0).into_iter().map(|(_, _, t)| t).collect()
}

#[test]
fn an_inverted_axis_is_noted_beside_its_triad_letter() {
    let state = mounted(&["X"]);
    let labels = overlay_labels(&state, 800.0 / 600.0);
    let x = labels.iter().find(|(_, _, t)| t == "X").expect("the X letter stays");
    let note = labels
        .iter()
        .find(|(_, _, t)| t == "Machine X+ moves left (inverted axis)")
        .expect("the inverted X is noted");
    assert_eq!(note.1, x.1, "the note wears the axis colour");
    assert!(note.0[1] > x.0[1] && (note.0[0] - x.0[0]).abs() < 1e-6, "just under the letter");
    assert_eq!(texts(&state).iter().filter(|t| t.contains("inverted")).count(), 1, "one note, X only");
    // The arrows are the program frame: identical to a standard machine's.
    assert_eq!(gizmo_segments(&state, 1.0), gizmo_segments(&mounted(&[]), 1.0));
}

#[test]
fn a_standard_machine_adds_no_triad_label() {
    assert_eq!(texts(&mounted(&[])), texts(&State::new()));
    // #727: the fresh world's set is the dimmed world axes, named.
    assert_eq!(texts(&State::new()), ["World", "X", "Y", "Z"]);
    let both = texts(&mounted(&["X", "Y"]));
    assert!(both.iter().any(|t| t == "Machine Y+ moves toward the front (inverted axis)"), "{both:?}");
}
