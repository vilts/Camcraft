//! #2181: the transport line of the stock removal calculated before
//! playback, and its stamp - an edit of the operation retires the
//! stored calculation and starts a new one. Provenance: our own
//! fixtures (a staged face) over the M7.1 field engine.
//!
//! Gate map (M4.4): covers ui::play_precalc, ui::actions::ensure_field_view (precalc stamp, #2280 stop line).

use camcraft_app::commands::{AddTool, CreateOperation, EditOperationParams, GenerateOperation, SetStock};
use camcraft_app::geometry::{Point3, IDENTITY_Q};
use camcraft_app::operation::{Height, OpParams, SrcRef};
use camcraft_app::schema::Location;
use camcraft_app::stock::StockShape;
use camcraft_app::stock_play::Precalc;
use camcraft_app::tools::catalog;

use crate::actions::ensure_field_view;
use crate::play_precalc::{playback_stop, precalc_readout, stop_readout};
use crate::state::{Sim, State};

fn face(stepover: f64) -> OpParams {
    OpParams::Face {
        holes: Default::default(),
        tilt: None,
        stepover,
        clearance_z: Height::fixed(25.0),
        cut_z: Height::fixed(14.0),
        top_z: Some(Height::fixed(19.0)),
        stepdown: Some(2.5),
        feed: 1200.0,
        plunge_feed: 300.0,
        climb: true,
        blend_turns: false,
    }
}

fn faced_state() -> State {
    let mut state = State::new();
    let w = &state.world;
    let loc = Location { pos: Point3::ZERO, rot: IDENTITY_Q };
    let cmd = SetStock::new(w, StockShape::Box { dx: 12.0, dy: 12.0, dz: 20.0 }, loc).expect("box stock");
    state.history.execute(w, cmd);
    let tool = catalog().into_iter().find(|(n, _)| *n == "EM-0400-2FL").map(|(_, t)| t).expect("tool");
    state.history.execute(w, AddTool::new(w, "EM-0400-2FL", tool).expect("add tool"));
    let cmd = CreateOperation::new(w, "Face1", "EM-0400-2FL", SrcRef::Stock, face(1.5)).expect("create face");
    state.history.execute(w, cmd);
    state.history.execute(w, GenerateOperation::new(w, "Face1").expect("generate"));
    state.doc_version += 1;
    state
}

#[test]
fn the_readout_says_progress_and_incomplete_and_nothing_when_done() {
    assert_eq!(precalc_readout(&Precalc::Off), None);
    let ready = Precalc::Ready { keys: 4, bytes: 1, stride: 64, secs: 0.5 };
    assert_eq!(precalc_readout(&ready), None, "a finished calculation is not news");
    let running = precalc_readout(&Precalc::Running { done: 50, total: 200 }).expect("a line");
    assert!(running.contains("25%"), "{running}");
    let stopped = Precalc::Incomplete { done: 100, total: 200, why: "cancelled".into() };
    let line = precalc_readout(&stopped).expect("a line");
    assert!(line.contains("incomplete") && line.contains("cancelled") && line.contains("50%"), "{line}");
    let empty = precalc_readout(&Precalc::Running { done: 0, total: 0 }).expect("a line");
    assert!(empty.contains("100%"), "an empty program does not divide by zero: {empty}");
}

#[test]
fn an_edit_of_the_operation_retires_the_stored_calculation() {
    let mut state = faced_state();
    state.sim = Some(Sim::new());
    ensure_field_view(&mut state);
    let play = state.field_play.as_ref().expect("the transport starts a playback worker");
    assert_ne!(play.precalc(), Precalc::Off, "the worker calculates before playback");
    let before = play.index().total();
    // A frame later, the same document: the same worker stays.
    ensure_field_view(&mut state);
    assert_eq!(state.field_play.as_ref().map(|p| p.index().total()), Some(before));
    // Halve the stepover: more passes, a different program.
    let w = &state.world;
    state.history.execute(w, EditOperationParams::new(w, "Face1", face(0.75)).expect("edit"));
    state.history.execute(w, GenerateOperation::new(w, "Face1").expect("regenerate"));
    state.doc_version += 1;
    ensure_field_view(&mut state);
    let play = state.field_play.as_ref().expect("a new worker for the new program");
    let after = play.index().total();
    assert!(after > before, "the stored calculation was for {before} moves; the edited program has {after}");
    assert_ne!(play.precalc(), Precalc::Off, "and it calculates again");
}

/// #2280: an operation with no toolpath after a generated one. The
/// playback starts and plays the generated one; the stop is its own
/// plain-English line, not a refusal on the status line.
#[test]
fn an_op_without_a_toolpath_stops_the_playback_and_says_so() {
    let mut state = faced_state();
    state.sim = Some(Sim::new());
    ensure_field_view(&mut state);
    let whole = state.field_play.as_ref().map(|p| p.index().total()).expect("Face1 alone plays");
    assert_eq!(playback_stop(&state), None, "a whole program has no stop line");

    let w = &state.world;
    let cmd = CreateOperation::new(w, "2D Contour1", "EM-0400-2FL", SrcRef::Stock, face(1.5)).expect("create");
    state.history.execute(w, cmd);
    state.doc_version += 1;
    state.status = "generated Face1: 40 moves, 0 warning(s)".into();
    ensure_field_view(&mut state);
    assert!(!matches!(state.field_view, Some(Err(_))), "the playback is not refused");
    let play = state.field_play.as_ref().expect("the playback runs");
    assert_eq!(play.index().total(), whole, "Face1's moves play");
    assert_eq!(
        playback_stop(&state).as_deref(),
        Some("Stock simulation stops before 2D Contour1: it has no toolpath. Generate it to see the rest.")
    );
    assert_eq!(stop_readout("X"), "Stock simulation stops before X: it has no toolpath. Generate it to see the rest.");
    assert_eq!(state.status, "generated Face1: 40 moves, 0 warning(s)", "the status line is not changed");
}
