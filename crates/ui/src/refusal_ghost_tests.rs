//! #261 part 3: a refused generation's ghost on screen. The moves the
//! generator had built ride the refusal into the state and draw dashed
//! in the dimmed gouge colour while the refusal stands; they retire with
//! it (the next command, a draft edit), and the operation itself never
//! holds them - no cache, nothing to post or simulate.
//!
//! Provenance: our own tests over our own refusal model; no vendor
//! material consulted.

use camcraft_app::commands::{AddTool, CreateOperation};
use camcraft_app::geo2d::Point2;
use camcraft_app::machining3d::Entry3D;
use camcraft_app::model;
use camcraft_app::operation::{Height, OpParams, SrcRef};
use camcraft_app::stock::StockShape;
use camcraft_app::tools::{Tool, catalog};

use crate::interact::toolpath_lines;
use crate::refusal_ghost::ghost_colour;
use crate::state::{CommandKind, ErrorScope, State, UiTarget};

/// A box and a slot as wide as its 10 mm end mill, which is not
/// centre-cutting: the slot ladder refuses at a pass start with the
/// moves it had built; the Generate click is queued and run.
fn refused() -> State {
    let mut state = State::new();
    state.pending.push_back(CommandKind::SetStock {
        shape: StockShape::Box { dx: 60.0, dy: 60.0, dz: 20.0 },
        pos: [0.0, 0.0, 0.0],
        yaw_deg: 0.0,
    });
    state.drain_actions();
    let em = catalog().into_iter().find(|(n, _)| *n == "EM-1000-4FL").map(|(_, t)| t).expect("catalog entry");
    state.history.execute(&state.world, AddTool::new(&state.world, "EM", Tool { center_cutting: false, ..em }).expect("tool"));
    let params = OpParams::Slot {
        a: Point2::new(10.0, 10.0),
        b: Point2::new(50.0, 20.0),
        width: 10.0,
        stock_to_leave: 0.0,
        clearance_z: Height::fixed(25.0),
        retract_z: None,
        top_z: Height::fixed(20.0),
        bottom_z: Height::fixed(14.0),
        stepdown: 3.0,
        stepover: 5.0,
        feed: 500.0,
        plunge_feed: 100.0,
        climb: true,
        entry: Entry3D::Plunge,
        helix_radius: 1.5,
        links: camcraft_app::links::Links::PLANAR,
        tilt: None,
        extend_start: 0.0,
        extend_end: 0.0,
    };
    state.history.execute(&state.world, CreateOperation::new(&state.world, "SLT", "EM", SrcRef::Stock, params).expect("create"));
    state.ui_target = UiTarget::Operation("SLT".into());
    state.sync_ops_ui();
    state.generate_operation("SLT");
    state.drain_actions();
    state
}

#[test]
fn a_refused_generation_draws_its_ghost_and_caches_nothing() {
    let state = refused();
    let msg = state.error_for(ErrorScope::Operation).expect("the refusal is on the page").to_owned();
    assert!(msg.contains("not centre-cutting"), "{msg}");
    let n = state.refusal_ghost().map(|g| g.len()).expect("the ghost rides the refusal");
    assert!(n > 0);
    // The operation holds nothing: the ghost is the refusal's, not the op's.
    let e = model::find_by_name(&state.world, "SLT").expect("op");
    assert!(model::toolpath_of(&state.world, &e).is_none(), "a ghost is never a toolpath");
    // Everything the viewport draws is the ghost's two tones - no
    // toolpath colour anywhere, since there is no toolpath.
    let pal = crate::egui_app::colorvision::palette_of(&state.world);
    let tones = [ghost_colour(pal, camcraft_app::cl::MoveKind::Feed), ghost_colour(pal, camcraft_app::cl::MoveKind::Rapid)];
    let lines = toolpath_lines(&state);
    assert!(!lines.is_empty(), "the ghost draws");
    assert!(lines.iter().all(|v| tones.contains(&[v[3], v[4], v[5]])), "only the ghost's dashes");
}

#[test]
fn the_ghost_retires_with_the_refusal() {
    // The next command retires the sentence, and the ghost with it.
    let mut state = refused();
    state.pending.push_back(CommandKind::Select(None));
    state.drain_actions();
    assert!(state.refusal_ghost().is_none());
    assert!(state.feedback_ghost.is_none(), "freed, not only hidden");
    assert!(toolpath_lines(&state).is_empty(), "no lingering ghost");

    // An edit of the page draft hides it at once: the sentence was
    // about the values before the edit.
    let mut state = refused();
    if let OpParams::Slot { feed, .. } = &mut state.op_ui.params {
        *feed = 400.0;
    }
    assert!(state.refusal_ghost().is_none(), "the draft moved");
    assert!(toolpath_lines(&state).is_empty());

    // A later refusal with no ghost never shows the old one.
    let mut state = refused();
    state.pending.push_back(CommandKind::GenerateOperation("no-such-op".into()));
    state.drain_actions();
    assert!(state.error_for(ErrorScope::Operation).is_some(), "refused again");
    assert!(state.refusal_ghost().is_none());
}
