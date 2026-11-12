//! #329 in the viewport: the work-offset chip on a height plane and the
//! plane's Z both read the SHOWN operation's own setup - two setups, two
//! program zeros, never the first setup's word on the second's op.
//!
//! Provenance: our own test over RS-274 work-offset semantics.

use camcraft_app::commands::{AddTool, CreateOperation};
use camcraft_app::operation::{Height, HeightReference, OpParams, SrcRef};
use camcraft_app::stock::StockShape;
use camcraft_app::tools::catalog;

use crate::interact::{height_planes, overlay_labels};
use crate::state::{CommandKind, State};

const TOOL: &str = "EM-0400-2FL";

fn run(state: &mut State, k: CommandKind) {
    state.pending.push_back(k);
    state.drain_actions();
}

fn face() -> OpParams {
    OpParams::Face {
        holes: Default::default(),
        blend_turns: false,
        tilt: None,
        stepover: 3.0,
        clearance_z: Height::of(HeightReference::WorkOffset, 25.0),
        cut_z: Height::of(HeightReference::WorkOffset, 19.5),
        top_z: None,
        stepdown: None,
        feed: 1200.0,
        plunge_feed: 300.0,
        climb: true,
    }
}

/// Place the active setup's one WCS (#342).
fn place(state: &mut State, z: f64) {
    let k = CommandKind::SetWcs { pos: [0.0, 0.0, z], roll_deg: 0.0, pitch_deg: 0.0, yaw_deg: 0.0 };
    run(state, k);
}

/// A box stock for the ACTIVE setup.
fn stock(state: &mut State) {
    let shape = StockShape::Box { dx: 40.0, dy: 40.0, dz: 20.0 };
    run(state, CommandKind::SetStock { shape, pos: [0.0; 3], yaw_deg: 0.0 });
}

fn create_in(state: &mut State, setup: &str, name: &str) {
    run(state, CommandKind::ActivateSetup(Some(setup.into())));
    let op = CreateOperation::new(&state.world, name, TOOL, SrcRef::Stock, face());
    state.history.execute(&state.world, op.expect("operation"));
}

/// Setup1 posts at G54 (origin), Setup2 picked G55 placed 2 mm low; one
/// face op in each, every height measured from the work offset.
fn two_setups() -> State {
    let mut state = State::new();
    let em = catalog().into_iter().find(|(n, _)| *n == TOOL).map(|(_, t)| t).expect("catalog entry");
    state.history.execute(&state.world, AddTool::new(&state.world, TOOL, em).expect("tool"));
    run(&mut state, CommandKind::CreateSetup { name: None, datum_from: None });
    run(&mut state, CommandKind::CreateSetup { name: None, datum_from: None });
    let names = camcraft_app::setup::names(&state.world);
    assert_eq!(names, ["Setup1", "Setup2"], "two setups");
    run(&mut state, CommandKind::ActivateSetup(Some("Setup1".into())));
    stock(&mut state);
    place(&mut state, 0.0);
    run(&mut state, CommandKind::ActivateSetup(Some("Setup2".into())));
    stock(&mut state);
    place(&mut state, -2.0);
    run(&mut state, CommandKind::SetSetupWorkOffset { setup: "Setup2".into(), slot: Some(1) });
    create_in(&mut state, "Setup1", "F1");
    create_in(&mut state, "Setup2", "F2");
    // Leave the OTHER setup active, so a chip reading the active (or the
    // first) setup instead of the op's own would show the wrong word.
    run(&mut state, CommandKind::ActivateSetup(Some("Setup1".into())));
    state
}

fn show(state: &mut State, name: &str) {
    state.op_ui.name = name.into();
    state.op_ui.tool = TOOL.into();
    state.op_ui.params = face();
    run(state, CommandKind::Select(Some(name.into())));
}

fn chips(state: &State) -> Vec<String> {
    overlay_labels(state, 800.0 / 600.0).into_iter().map(|(_, _, t)| t).collect()
}

fn clearance_z(state: &State) -> Option<f64> {
    height_planes(state).into_iter().find(|p| p.name == "Clearance").map(|p| p.z)
}

#[test]
fn the_chip_and_planes_read_the_shown_operations_setup() {
    let mut state = two_setups();
    show(&mut state, "F2");
    let texts = chips(&state);
    assert!(texts.iter().any(|t| t == "F2 · Clearance 23.00 (WCS G55 +25.00)"), "Setup2's word: {texts:?}");
    // Chips only: #727 puts Setup1's own name on its origin gizmo, and
    // that is legitimate - the law here is that no *plane chip* on
    // Setup2's op carries Setup1's word.
    assert!(!texts.iter().any(|t| t.contains(" · ") && t.contains("G54")), "never Setup1's word on Setup2's op: {texts:?}");
    assert_eq!(clearance_z(&state), Some(23.0), "the plane stands on Setup2's G55");

    show(&mut state, "F1");
    let texts = chips(&state);
    assert!(texts.iter().any(|t| t == "F1 · Clearance 25.00 (WCS G54 +25.00)"), "Setup1's word: {texts:?}");
    assert_eq!(clearance_z(&state), Some(25.0), "the plane stands on Setup1's G54");

    // Activating Setup2 changes nothing for Setup1's op: the op's own
    // setup wins over the active one.
    run(&mut state, CommandKind::ActivateSetup(Some("Setup2".into())));
    assert!(chips(&state).iter().any(|t| t.contains("(WCS G54 ")), "F1 stays at G54");
}

#[test]
fn a_draft_name_reads_the_active_setup() {
    let mut state = two_setups();
    run(&mut state, CommandKind::ActivateSetup(Some("Setup2".into())));
    state.op_ui.name = "Draft".into();
    assert_eq!(crate::interact::shown_setup(&state).as_deref(), Some("Setup2"), "the active setup");
}
