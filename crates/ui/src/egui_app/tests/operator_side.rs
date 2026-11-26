//! #2632: the work envelope shows an arrow on the operator side and the
//! text "Operator side". The side is the machine's: the default of its
//! kind, or the side that its file names. The View door's "Show operator
//! side" box hides and shows them (view state: no command, no history).
//! The arrow is built when its input changes, not on each frame.
//! Provenance: our own tests over our own shell; no HSMWorks material.

use super::*;
use crate::operator_side::{builds, LABEL, SWITCH};
use camcraft_app::machine::envelope::EnvelopeShape;
use camcraft_app::machine::{self, AxisLimits, Kind, Machine, OperatorSide};
use egui_kittest::kittest::NodeT;

const ENVELOPE: &str = "Show work envelope";

fn ax(name: &str, travel: (f64, f64)) -> AxisLimits {
    AxisLimits { name: name.into(), travel: Some(travel), ..AxisLimits::default() }
}

fn machine_of(kind: Option<Kind>, side: Option<OperatorSide>) -> Machine {
    Machine {
        model: "Test machine".into(),
        kind: kind.map(|k| k.word().to_owned()),
        axes: vec![ax("X", (-100.0, 200.0)), ax("Y", (-50.0, 50.0)), ax("Z", (-30.0, 0.0))],
        operator_side: side,
        ..Machine::default()
    }
}

/// A state with `m` mounted, the envelope shown and the camera on it.
fn shown(m: &Machine) -> State {
    let mut st = State::new();
    machine::set(&st.world, "test.mch", Some(m));
    st.show_work_envelope = true;
    // The whole envelope is in view: a point behind the camera has no text.
    st.cam = crate::viewport::Camera::frame([50.0, 0.0, 0.0], 400.0);
    st
}

/// The arrow vertices: what the scene draws after the envelope itself.
fn arrow(st: &State) -> Vec<[f32; 6]> {
    let all = crate::work_envelope::overlay_lines(st);
    let envelope = crate::work_envelope::envelope_lines(st).len();
    all[envelope..].to_vec()
}

fn texts(st: &State) -> Vec<String> {
    interact::overlay_labels(st, 1.5).into_iter().map(|(_, _, t)| t).filter(|t| t == LABEL).collect()
}

/// The arrow leaves the box of `st` on `side`: it starts on that face, in
/// the middle of its lower edge, and its tip is outside the box.
fn assert_arrow_on(st: &State, side: OperatorSide, what: &str) {
    let Ok(EnvelopeShape::Box { min, max }) = machine::work_envelope(&st.world) else {
        panic!("{what}: a box envelope");
    };
    let a = arrow(st);
    assert_eq!(a.len(), 8, "{what}: a shaft and a three-stroke head");
    let (axis, sign) = side.outward();
    let across = 1 - axis;
    let face = if sign < 0.0 { min[axis] } else { max[axis] } as f32;
    let (start, tip) = (a[0], a[1]);
    assert_eq!(start[axis], face, "{what}: the arrow starts at the envelope edge");
    assert_eq!(start[across], (0.5 * (min[across] + max[across])) as f32, "{what}: in the middle of the edge");
    assert_eq!(start[2], crate::floor::floor_z(st) as f32, "{what}: on the floor, at the lower edge");
    assert!((tip[axis] - face) * sign as f32 > 1.0, "{what}: the tip points out of the envelope: {tip:?}");
    for v in &a {
        assert!((v[axis] - face) * sign as f32 >= 0.0, "{what}: no stroke is inside the envelope: {v:?}");
        assert_eq!(v[3..], a[0][3..], "{what}: one colour");
    }
    let scene = interact::collect_overlay(st);
    assert!(scene.contains(&tip), "{what}: the scene draws the arrow");
    assert_eq!(texts(st), [LABEL], "{what}: the text is at the arrow, one time");
}

#[test]
fn a_mill_shows_the_arrow_and_the_text_on_its_default_side() {
    let st = shown(&machine_of(None, None));
    assert_eq!(OperatorSide::of(&machine_of(None, None)), OperatorSide::MinusY, "a mill is seen from the front");
    assert_arrow_on(&st, OperatorSide::MinusY, "mill");
    assert_eq!(LABEL, "Operator side");
}

#[test]
fn each_machine_kind_shows_the_arrow_on_its_default_side() {
    for kind in [Kind::Mill, Kind::MillTurn, Kind::Plasma, Kind::Waterjet, Kind::WireEdm, Kind::SinkerEdm] {
        let st = shown(&machine_of(Some(kind), None));
        assert_arrow_on(&st, OperatorSide::default_for(kind), kind.word());
    }
}

#[test]
fn a_lathe_shows_the_arrow_on_the_viewer_side_of_the_turning_plane() {
    let lathe = Machine {
        model: "Test lathe".into(),
        kind: Some("lathe".into()),
        axes: vec![ax("X", (-5.0, 60.0)), ax("Z", (-197.0, 3.0))],
        ..Machine::default()
    };
    // Check the default +Y side and each explicit side on the cylinder.
    for side in [None].into_iter().chain(OperatorSide::ALL.map(Some)) {
        let lathe = Machine { operator_side: side, ..lathe.clone() };
        let st = shown(&lathe);
        let a = arrow(&st);
        assert_eq!(a.len(), 8);
        let (axis, sign) = OperatorSide::of(&lathe).outward();
        let down = 1 - axis;
        let (start, tip) = (a[0], a[1]);
        assert!((start[0].hypot(start[1]) - 60.0).abs() < 1e-4, "the start is on the cylinder: {start:?}");
        assert!(start[axis] * sign as f32 > 0.0 && start[down] < 0.0, "the start is in the lower-side quadrant: {start:?}");
        assert_eq!(start[2], -97.0, "the start is in the middle of Z");
        assert!((tip[axis] - start[axis]) * sign as f32 > 1.0, "the tip points outwards: {tip:?}");
        assert_eq!(tip[down], start[down]);
        assert_eq!(tip[2], start[2]);
        assert!(a.iter().all(|v| v[0].hypot(v[1]) >= 60.0 - 1e-4), "all strokes are outside the cylinder");
        assert!(interact::collect_overlay(&st).contains(&start));
        assert_eq!(texts(&st), [LABEL]);
    }
}

#[test]
fn a_machine_that_names_a_side_moves_the_arrow_to_it() {
    for side in OperatorSide::ALL {
        let st = shown(&machine_of(None, Some(side)));
        assert_arrow_on(&st, side, side.word());
    }
    // The file form: `general.operator_side` reaches the viewport.
    let text = r#"{ "general": { "model": "T", "vendor": "V", "operator_side": "+X" },
        "kinematics": { "default": { "units": { "length": "mm" }, "parts": [
            { "type": "linear", "name": "X", "travel": { "min": -100, "max": 200 } },
            { "type": "linear", "name": "Y", "travel": { "min": -50, "max": 50 } },
            { "type": "linear", "name": "Z", "travel": { "min": -30, "max": 0 } } ] } } }"#;
    let m = machine::parse_mch(text).expect("the file loads");
    assert_arrow_on(&shown(&m), OperatorSide::PlusX, "file");
}

#[test]
fn no_envelope_means_no_arrow_and_no_text() {
    let mut st = shown(&machine_of(None, None));
    st.show_work_envelope = false;
    assert!(arrow(&st).is_empty() && interact::collect_overlay(&st).iter().all(|v| v[3..] != crate::work_envelope::ENVELOPE_RGB));
    assert!(texts(&st).is_empty(), "the envelope box is off");
    // A machine without travel has no envelope to mark.
    let mut bare = machine_of(None, None);
    bare.axes.iter_mut().for_each(|a| a.travel = None);
    let st = shown(&bare);
    let _ = interact::collect_overlay(&st);
    assert!(arrow(&st).is_empty() && texts(&st).is_empty());
}

fn click(h: &mut Harness<'static, ShellUi>, label: &str) {
    if h.query_by_label(label).is_none() {
        open_view_menu(h);
    }
    h.get_by_label(label).click();
    h.run();
}

#[test]
fn the_switch_hides_and_shows_the_arrow_and_the_text_as_view_state() {
    let mut h = shell_harness();
    machine::set(&h.state().state.world, "test.mch", Some(&machine_of(None, None)));
    let undo_before = h.state().state.history.can_undo();
    open_view_menu(&mut h);
    assert!(h.state().state.show_operator_side, "shown by default");
    assert!(h.get_by_label(SWITCH).accesskit_node().is_disabled(), "nothing to switch while the envelope is off");
    click(&mut h, ENVELOPE);
    if h.query_by_label(SWITCH).is_none() {
        open_view_menu(&mut h);
    }
    assert!(!h.get_by_label(SWITCH).accesskit_node().is_disabled());
    assert_eq!(arrow(&h.state().state).len(), 8, "the envelope comes with its arrow");
    assert_eq!(texts(&h.state().state), [LABEL]);

    click(&mut h, SWITCH);
    let st = &h.state().state;
    assert!(!st.show_operator_side && st.show_work_envelope);
    assert!(arrow(st).is_empty(), "the switch hides the arrow");
    assert!(texts(st).is_empty(), "and the text");
    assert_eq!(crate::work_envelope::envelope_lines(st).len(), 24, "the envelope stays");
    assert!(st.pending.is_empty(), "a view switch queues nothing");
    assert_eq!(st.history.can_undo(), undo_before, "no command history entry");

    click(&mut h, SWITCH);
    let st = &h.state().state;
    assert!(st.show_operator_side);
    assert_eq!(arrow(st).len(), 8, "the switch shows them again");
    assert_eq!(texts(st), [LABEL]);
}

#[test]
fn a_still_frame_builds_no_arrow_and_a_changed_input_builds_one() {
    let mut st = shown(&machine_of(None, None));
    assert_eq!(builds(&st), 0);
    let first = arrow(&st);
    assert_eq!(builds(&st), 1, "the first scene builds the arrow");
    let uv = |st: &State| interact::overlay_labels(st, 1.5).into_iter().find(|(_, _, t)| t == LABEL).map(|(uv, _, _)| uv);
    let at = uv(&st).expect("the text is on screen");
    for _ in 0..5 {
        let _ = interact::collect_overlay(&st);
        let _ = interact::overlay_labels(&st, 1.5);
    }
    assert_eq!(builds(&st), 1, "a still frame builds nothing");
    // The camera is no input: the text follows the view without a build.
    st.cam.yaw += 0.4;
    let _ = interact::collect_overlay(&st);
    assert_ne!(uv(&st).expect("still on screen"), at, "the text follows the view");
    assert_eq!(builds(&st), 1, "an orbit builds nothing");
    // A new side is a new input.
    machine::set(&st.world, "test.mch", Some(&machine_of(None, Some(OperatorSide::PlusX))));
    assert_ne!(arrow(&st), first, "the arrow moved");
    assert_eq!(builds(&st), 2, "one build for the new side");
    // A moved envelope is a new input too.
    let mut wider = machine_of(None, Some(OperatorSide::PlusX));
    wider.axes[0].travel = Some((-100.0, 400.0));
    machine::set(&st.world, "test.mch", Some(&wider));
    assert_eq!(arrow(&st)[0][0], 400.0);
    assert_eq!(builds(&st), 3);
    let _ = interact::collect_overlay(&st);
    assert_eq!(builds(&st), 3);
}
