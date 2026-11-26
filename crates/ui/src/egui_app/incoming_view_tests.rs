//! #406 in the surface: while Setup2's settings page is open and Setup2
//! takes what Setup1 leaves behind, the viewport scene carries the
//! incoming body's sheet as a teal stock-slot part; it is measured once
//! per document version (a second frame measures nothing), and the
//! page's Close drops it from the scene and from the cache.
//! Provenance: our own page law; no vendor material consulted.

use egui::Vec2;
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;

use camcraft_app::commands::{
    AddTool, CreateOperation, CreateSetup, GenerateOperation, ImportModel, SetSetupStockSource, SetStock,
};
use camcraft_app::geometry::{Point3, cube};
use camcraft_app::operation::{Height, OpParams, SrcRef};
use camcraft_app::schema::Location;
use camcraft_app::setup_stock::StockSource;
use camcraft_app::stock::StockShape;

use crate::incoming_view::{self, INCOMING_CLASS};
use crate::state::{SetupPage, State, UiTarget};

const TOOL: &str = "EM-0400-2FL";

fn face(cut_z: f64) -> OpParams {
    OpParams::Face {
        holes: Default::default(),
        tilt: None,
        blend_turns: false, blend_radius: None,
        stepover: 3.0,
        clearance_z: Height::fixed(30.0),
        cut_z: Height::fixed(cut_z),
        top_z: None,
        stepdown: None,
        feed: 1200.0,
        plunge_feed: 300.0,
        climb: true,
        picks: Default::default(),
    }
}

fn add_face(state: &mut State, name: &str, cut_z: f64) {
    let op = CreateOperation::new(&state.world, name, TOOL, SrcRef::Stock, face(cut_z)).expect("create");
    state.history.execute(&state.world, op);
    let gen_op = GenerateOperation::new(&state.world, name).expect("generate");
    state.history.execute(&state.world, gen_op);
}

/// A 60 x 60 x 16 part in a stock Z -2..18; Setup1 faces the top to 16,
/// Setup2 takes what Setup1 leaves behind and faces to 15. Setup2 is
/// active (the browser activates a setup before it opens its page).
fn carried_setups() -> State {
    let mut state = State::new();
    let w = &state.world;
    let part = ImportModel::from_mesh("part", cube(Point3::new(-30.0, -30.0, 0.0), Point3::new(30.0, 30.0, 16.0)));
    state.history.execute(w, part);
    let at = Location { pos: Point3::new(0.0, 0.0, -2.0), ..Default::default() };
    let stock = SetStock::new(w, StockShape::Box { dx: 60.0, dy: 60.0, dz: 20.0 }, at).expect("stock");
    state.history.execute(w, stock);
    let tool = camcraft_app::tools::catalog().into_iter().find(|(n, _)| *n == TOOL).map(|(_, t)| t).expect("tool");
    let add = AddTool::new(w, TOOL, tool).expect("add tool");
    state.history.execute(w, add);
    let s1 = CreateSetup::new(w, None, None).expect("Setup1");
    state.history.execute(w, s1);
    add_face(&mut state, "Face1", 16.0);
    let s2 = CreateSetup::new(&state.world, None, Some("Setup1")).expect("Setup2");
    state.history.execute(&state.world, s2);
    let pick = SetSetupStockSource::new(&state.world, "Setup2", StockSource::FromPrevious).expect("carry");
    state.history.execute(&state.world, pick);
    add_face(&mut state, "Face2", 15.0);
    state.doc_version += 1;
    state
}

/// The scene's incoming-stock parts this frame.
fn incoming_parts(state: &State) -> Vec<Vec<[f32; 3]>> {
    super::world::collect(state)
        .parts
        .into_iter()
        .filter(|p| p.stock && !p.classes.is_empty() && p.classes.iter().all(|&c| c == INCOMING_CLASS))
        .map(|p| p.verts)
        .collect()
}

fn frame(ui: &mut egui::Ui, st: &mut State) {
    st.drain_actions();
    super::pages::properties(ui, st, &super::theme::current());
}

#[test]
fn the_incoming_stock_shows_while_the_carried_setups_page_is_open() {
    let mut state = carried_setups();
    state.ui_target = UiTarget::SetupNode("Setup2".into(), SetupPage::Settings);
    let mut h = Harness::builder().with_size(Vec2::new(700.0, 900.0)).build_ui_state(frame, state);
    h.run();
    assert!(h.query_by_label_contains("Incoming from Setup1").is_some(), "the page measured the incoming stock");
    let parts = incoming_parts(h.state());
    assert_eq!(parts.len(), 1, "one incoming sheet in the scene");
    let verts = &parts[0];
    assert!(!verts.is_empty() && verts.len().is_multiple_of(3), "a triangle soup");
    // Setup1 faced the block to Z 16 over its whole 60 x 60: the sheet
    // Setup2 starts from stands at 16, inside the block's footprint.
    let top = verts.iter().map(|v| v[2]).fold(f32::MIN, f32::max);
    assert!((top - 16.0).abs() < 1e-3, "the sheet stands where Setup1 left it, got top {top}");
    assert!(verts.iter().all(|v| v[0].abs() <= 30.5 && v[1].abs() <= 30.5), "inside the stock footprint");
    let once = h.state().incoming_view.measured;
    assert_eq!(once, 1, "measured once, and the page's box and volume read the same measurement");

    // More frames, no document change: nothing is measured again.
    h.run_steps(3);
    assert_eq!(h.state().incoming_view.measured, once, "never measured per frame");
    assert_eq!(incoming_parts(h.state()).len(), 1, "still drawn");

    // A document change re-measures once.
    h.state_mut().doc_version += 1;
    h.run_steps(3);
    assert_eq!(h.state().incoming_view.measured, once + 1, "re-measured once per document version");

    // The page's Close: the sheet leaves the scene and the cache.
    h.get_by_label("Close").click();
    h.run_steps(2);
    assert!(!matches!(h.state().ui_target, UiTarget::SetupNode(..)), "the page closed");
    assert!(incoming_parts(h.state()).is_empty(), "no incoming sheet once the page is closed");
    assert!(incoming_view::mesh(h.state()).is_none(), "nothing cached to draw");
    assert!(incoming_view::measured(h.state(), "Setup2").is_none(), "the cached mesh is dropped");
}

#[test]
fn no_incoming_stock_for_own_stock_or_another_page() {
    let mut state = carried_setups();
    // Setup1's page: the first setup starts from its own stock.
    state.ui_target = UiTarget::SetupNode("Setup1".into(), SetupPage::Settings);
    state.drain_actions();
    assert!(incoming_parts(&state).is_empty(), "the first setup carries nothing");
    // Setup2's Machine page: not the settings page.
    state.ui_target = UiTarget::SetupNode("Setup2".into(), SetupPage::Machine);
    state.drain_actions();
    assert!(incoming_parts(&state).is_empty(), "only the settings page shows it");
    // Setup2 back on its own stock: nothing carried to show.
    state.ui_target = UiTarget::SetupNode("Setup2".into(), SetupPage::Settings);
    let own = SetSetupStockSource::new(&state.world, "Setup2", StockSource::Own).expect("own");
    state.history.execute(&state.world, own);
    state.doc_version += 1;
    state.drain_actions();
    assert!(incoming_parts(&state).is_empty(), "its own stock is the Stock node, not a carried body");
    assert_eq!(state.incoming_view.measured, 0, "nothing was measured");
}
