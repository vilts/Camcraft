//! #261 part 2: a refused generation shows where. The refusal that
//! names a spot keeps it beside the banner sentence; the viewport marks
//! that spot in the gouge colour while the sentence stands, and the mark
//! retires with it - the next command, or an edit of the page draft.
//! Nothing is generated or cached, so nothing refused can post.
//!
//! Provenance: our own tests over our own refusal model; no vendor
//! material consulted.

use camcraft_app::commands::{AddTool, CreateOperation};
use camcraft_app::drilling::Hole;
use camcraft_app::geo2d::Point2;
use camcraft_app::model;
use camcraft_app::operation::{Height, OpParams, SrcRef};
use camcraft_app::stock::StockShape;
use camcraft_app::tools::catalog;

use crate::interact::toolpath_lines;
use crate::state::{CommandKind, ErrorScope, State, UiTarget};

const TOOL: &str = "EM-0800-4FL";

/// A box, an 8 mm end mill, and a Bore op whose one hole (7 mm, at
/// (40, 15)) the cutter cannot enter; the Generate click is queued and
/// run, so the state holds the refusal.
fn refused() -> State {
    let mut state = State::new();
    state.pending.push_back(CommandKind::SetStock {
        shape: StockShape::Box { dx: 60.0, dy: 60.0, dz: 20.0 },
        pos: [0.0, 0.0, 0.0],
        yaw_deg: 0.0,
    });
    state.drain_actions();
    let tool = catalog().into_iter().find(|(n, _)| *n == TOOL).map(|(_, t)| t).expect("catalog entry");
    state.history.execute(&state.world, AddTool::new(&state.world, TOOL, tool).expect("tool"));
    let params = OpParams::Bore {
        break_through: None,
        links: camcraft_app::links::Links::PLANAR,
        clearance_z: Height::fixed(25.0),
        retract_z: None,
        top_z: Height::fixed(20.0),
        bottom_z: None,
        stock_to_leave: 0.0,
        stepdown: 5.0,
        feed: 500.0,
        plunge_feed: 150.0,
        climb: true,
        tolerance: 0.01,
        holes: vec![Hole { xy: Point2::new(40.0, 15.0), radius: 3.5, entry_z: 20.0, floor_z: 5.0, through: false, stepped: false }],
    };
    state.history.execute(&state.world, CreateOperation::new(&state.world, "BOR", TOOL, SrcRef::Stock, params).expect("create"));
    state.ui_target = UiTarget::Operation("BOR".into());
    state.sync_ops_ui();
    state.generate_operation("BOR");
    state.drain_actions();
    state
}

fn gouge(state: &State) -> [f32; 3] {
    crate::egui_app::colorvision::palette_of(&state.world).gouge
}

#[test]
fn a_refused_generation_marks_its_spot_and_caches_nothing() {
    let state = refused();
    let msg = state.error_for(ErrorScope::Operation).expect("the refusal is on the page").to_owned();
    assert!(msg.contains("no circle left inside"), "{msg}");
    assert_eq!(state.refusal_focus(), Some([40.0, 15.0, 20.0]), "the hole's mouth");
    let e = model::find_by_name(&state.world, "BOR").expect("op");
    assert!(model::toolpath_of(&state.world, &e).is_none(), "a refusal caches no path - nothing can post");
    let lines = toolpath_lines(&state);
    let g = gouge(&state);
    assert_eq!(lines.len(), 6, "only the refusal's cross draws: {lines:?}");
    assert!(lines.iter().all(|v| v[3..6] == g), "in the gouge colour");
    // The cross is centred on the spot: each arm's two ends average to it.
    for arm in lines.chunks(2) {
        let mid = [(arm[0][0] + arm[1][0]) / 2.0, (arm[0][1] + arm[1][1]) / 2.0, (arm[0][2] + arm[1][2]) / 2.0];
        assert_eq!(mid, [40.0, 15.0, 20.0]);
    }
}

#[test]
fn the_mark_retires_with_the_refusal() {
    // The next command retires the sentence, and the mark with it.
    let mut state = refused();
    state.pending.push_back(CommandKind::Select(None));
    state.drain_actions();
    assert_eq!(state.refusal_focus(), None);
    assert!(toolpath_lines(&state).is_empty(), "no lingering mark");

    // An edit of the page draft retires it too: the sentence was about
    // the values before the edit.
    let mut state = refused();
    if let OpParams::Bore { feed, .. } = &mut state.op_ui.params {
        *feed = 400.0;
    }
    assert_eq!(state.refusal_focus(), None, "the draft moved");

    // A later refusal that names no spot never shows the old one.
    let mut state = refused();
    state.pending.push_back(CommandKind::GenerateOperation("no-such-op".into()));
    state.drain_actions();
    assert!(state.error_for(ErrorScope::Operation).is_some(), "refused again");
    assert_eq!(state.refusal_focus(), None, "an unlocated refusal marks nothing");
}
