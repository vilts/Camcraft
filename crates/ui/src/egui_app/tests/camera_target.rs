//! #1997: the orbit pivot (`Camera::target`) comes from the part and the
//! stock, never from the machine work envelope (#1567). Pinned after `frame`
//! and after a document open, each with the envelope display on and off:
//! the readings are equal and lie on the part, not at the envelope centre.
//! Provenance: our own tests over our own shell; no HSMWorks material.

use super::*;
use camcraft_app::machine::{self, AxisLimits, Machine};

fn ax(name: &str, travel: (f64, f64)) -> AxisLimits {
    AxisLimits { name: name.into(), travel: Some(travel), ..AxisLimits::default() }
}

/// A mill whose travel box is far from symmetric about the seed's block
/// (0..60, 0..60, 0..30): its centre is (650, -150, -250).
fn mill() -> Machine {
    Machine {
        model: "Pivot mill".into(),
        axes: vec![ax("X", (-100.0, 1400.0)), ax("Y", (-700.0, 400.0)), ax("Z", (-500.0, 0.0))],
        ..Machine::default()
    }
}

/// The seeded demo (block, fitted stock, WCS) with the mill mounted.
fn seeded_with_mill() -> State {
    let state = ShellUi::seeded().state;
    machine::set(&state.world, "pivot.mch", Some(&mill()));
    state
}

fn frame(state: &mut State) -> [f64; 3] {
    state.run(CommandKind::FrameAll);
    state.cam.target
}

/// The pivot sits on the seed's part and stock (the block plus a 2 mm
/// fitted margin), well away from the envelope centre.
fn assert_on_part(t: [f64; 3], what: &str) {
    let inside = (0..3).all(|k| (-2.0..=[62.0, 62.0, 32.0][k]).contains(&t[k]));
    assert!(inside, "{what}: the pivot {t:?} is not on the part and stock");
}

#[test]
fn frame_target_is_the_same_with_the_envelope_on_and_off() {
    let mut state = seeded_with_mill();
    assert!(machine::work_envelope(&state.world).is_ok(), "the mill states its travel");
    state.show_work_envelope = false;
    let off = frame(&mut state);
    state.show_work_envelope = true;
    assert!(!crate::work_envelope::envelope_lines(&state).is_empty(), "the envelope draws");
    let on = frame(&mut state);
    assert_eq!(on, off, "the envelope moved the frame target");
    assert_on_part(on, "frame");
    let dist = state.cam.dist;
    state.show_work_envelope = false;
    frame(&mut state);
    assert_eq!(state.cam.dist, dist, "the envelope changed the frame distance");
}

#[test]
fn open_frames_the_part_with_the_envelope_on_and_off() {
    let path = std::env::temp_dir().join(format!("camcraft-1997-pivot-{}.camproj", std::process::id()));
    let mut state = seeded_with_mill();
    state.run(CommandKind::SaveProjectFile(path.clone()));
    assert!(state.status.starts_with("saved"), "{}", state.status);
    let framed = frame(&mut state);

    let mut readings = Vec::new();
    for shown in [false, true] {
        // A camera parked far away (the last document's view, or the
        // default at the world origin) must not survive the open.
        state.cam.target = [400.0, 400.0, 400.0];
        state.show_work_envelope = shown;
        state.run(CommandKind::OpenProjectFile(path.clone()));
        assert!(state.status.starts_with("opened"), "{}", state.status);
        let lines = crate::work_envelope::envelope_lines(&state);
        assert_eq!(lines.is_empty(), !shown, "the reopened mill's envelope draws only when shown");
        let t = state.cam.target;
        assert_on_part(t, if shown { "open, envelope on" } else { "open, envelope off" });
        readings.push((t, state.cam.dist));
    }
    let _ = std::fs::remove_file(&path);
    assert_eq!(readings[0], readings[1], "the envelope moved the open target");
    assert_eq!(readings[0].0, framed, "open frames as the F key does");
}
