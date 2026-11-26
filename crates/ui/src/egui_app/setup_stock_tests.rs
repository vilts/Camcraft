//! #320 in the surface: the setup page's stock source. The first setup
//! shows no choice; a later one offers "Its own stock" and "What Setup1
//! leaves behind", a click queues `SetSetupStockSource` (never a world
//! poke), Ctrl+Z / Ctrl+Y walk it, the incoming stock's box and volume
//! show once it is picked, and a carry the sim refuses disables the
//! option with its reason.
//! Provenance: our own page law; no vendor material consulted.

use egui::accesskit::Role;
use egui::{Event, Key, Modifiers, Vec2};
use egui_kittest::kittest::{NodeT, Queryable};
use egui_kittest::Harness;

use camcraft_app::commands::{
    AddTool, AlignPartToSetup, CreateOperation, CreateSetup, GenerateOperation, ImportModel, SetStock,
};
use camcraft_app::geometry::{Point3, cube};
use camcraft_app::operation::{Height, OpParams, SrcRef};
use camcraft_app::schema::Location;
use camcraft_app::setup_stock::{self, StockSource};
use camcraft_app::stock::StockShape;
use camcraft_app::stock_sim::CARRY_TURN_REFUSAL;

use crate::state::State;

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

fn add_face(state: &mut State, name: &str) {
    let op = CreateOperation::new(&state.world, name, TOOL, SrcRef::Stock, face(16.0)).expect("create");
    state.history.execute(&state.world, op);
    let gen_op = GenerateOperation::new(&state.world, name).expect("generate");
    state.history.execute(&state.world, gen_op);
}

/// A 60 x 60 x 16 part in a stock Z -2..18, Setup1 facing the top, and
/// Setup2 (from Setup1's datum, holding the part alike) facing too.
fn two_setups() -> State {
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
    add_face(&mut state, "Face1");
    let s2 = CreateSetup::new(&state.world, None, Some("Setup1")).expect("Setup2");
    state.history.execute(&state.world, s2);
    add_face(&mut state, "Face2");
    state
}

fn frame(ui: &mut egui::Ui, st: &mut State) {
    super::panes::shortcuts(ui, st);
    st.drain_actions();
    super::setup_stock_pick::picker(ui, st, "Setup2");
}

fn key(h: &mut Harness<'static, State>, k: Key) {
    h.input_mut().events.push(Event::ModifiersChanged(Modifiers::COMMAND));
    h.input_mut().events.push(Event::Key {
        key: k,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: Modifiers::COMMAND,
    });
    h.run_steps(2);
}

fn harness(state: State) -> Harness<'static, State> {
    Harness::builder().with_size(Vec2::new(900.0, 400.0)).build_ui_state(frame, state)
}

#[test]
fn the_first_setup_offers_no_choice() {
    let mut h = Harness::builder()
        .with_size(Vec2::new(900.0, 300.0))
        .build_ui_state(|ui, st: &mut State| super::setup_stock_pick::picker(ui, st, "Setup1"), two_setups());
    h.run();
    assert_eq!(h.query_all_by(|n| n.role() == Role::RadioButton).count(), 1, "only its own stock");
    assert!(h.query_by_label_contains("leaves behind").is_none());
    assert!(h.query_by_label_contains("no setup before it").is_some());
}

#[test]
fn picking_what_setup1_leaves_shows_the_incoming_stock_and_undoes() {
    let mut h = harness(two_setups());
    h.run();
    assert!(h.state().pending.is_empty(), "drawing the choice queues nothing");
    assert_eq!(setup_stock::effective(&h.state().world, "Setup2"), Some(StockSource::Own), "same hold: its own stock");
    assert!(h.query_by_label_contains("Incoming from").is_none(), "no incoming stock for its own block");
    h.get_by_label("What Setup1 leaves behind").click();
    h.run();
    assert_eq!(setup_stock::stored(&h.state().world, "Setup2"), Some(StockSource::FromPrevious), "the drain lands the pick");
    assert_eq!(h.state().status, "Setup2 stock: What Setup1 leaves behind");
    h.run();
    assert!(
        h.query_by_label_contains("Incoming stock not known yet: Face2").is_some(),
        "the pick moved Face2's inputs: the page says so rather than guess"
    );
    let regen = GenerateOperation::new(&h.state().world, "Face2").expect("regenerate");
    let st = h.state_mut();
    st.history.execute(&st.world, regen);
    st.doc_version += 1;
    h.run();
    // Setup1 faced the block (Z -2..18) to 16: 60 x 60 x 18 reaches Setup2.
    assert!(h.query_by_label_contains("Z -2.000 .. 16.000 mm").is_some(), "the incoming box");
    assert!(h.query_by_label_contains("Incoming volume: 64800.0").is_some(), "the incoming volume");
    key(&mut h, Key::Z); // the regeneration
    key(&mut h, Key::Z); // the pick
    assert_eq!(setup_stock::stored(&h.state().world, "Setup2"), None, "undo restores the unpicked default");
    key(&mut h, Key::Y);
    assert_eq!(setup_stock::stored(&h.state().world, "Setup2"), Some(StockSource::FromPrevious), "redo picks again");
}

#[test]
fn a_refused_carry_disables_the_option_with_its_reason() {
    let mut state = two_setups();
    let skew = AlignPartToSetup::angles(&state.world, "Setup2", [0.0, 0.0, 30.0]).expect("30 degrees about Z");
    state.history.execute(&state.world, skew);
    let mut h = harness(state);
    h.run();
    let carried =
        h.get_by(|n| n.role() == Role::RadioButton && n.label().as_deref() == Some("What Setup1 leaves behind"));
    assert!(carried.accesskit_node().is_disabled(), "a turn the lattice cannot carry cannot be picked");
    assert!(h.query_by_label_contains(CARRY_TURN_REFUSAL).is_some(), "the reason is named on the page");
    // #866: the page says which turn and what to do about it.
    assert!(h.query_by_label_contains("'Setup2' holds the part turned X 0°, Y 0°, Z 30° from how 'Setup1' holds it").is_some());
    assert!(h.query_by_label_contains("To go on, choose \"Its own stock\" for this setup").is_some(), "what to do");
    h.get_by_label("Its own stock (the Stock node)").click();
    h.run();
    assert_eq!(setup_stock::stored(&h.state().world, "Setup2"), Some(StockSource::Own), "a new block can still be picked");
    assert!(h.query_by_label_contains("Cannot take what Setup1 leaves behind").is_some());
}

/// #866: the setup's Settings page speaks the user's words - the
/// active-setup radio no longer says "furniture" (our in-code word for a
/// setup's stock, WCS and fixtures).
#[test]
fn the_settings_page_has_no_in_code_jargon() {
    let mut h = Harness::builder().with_size(Vec2::new(900.0, 1200.0)).build_ui_state(
        |ui, st: &mut State| super::setup_pages::page(ui, st, "Setup2", crate::state::SetupPage::Settings),
        two_setups(),
    );
    h.run();
    let radio = h.get_by(|n| n.role() == Role::RadioButton && n.label().as_deref() == Some(super::setup_pages::ACTIVE_SETUP_LABEL));
    assert!(!radio.accesskit_node().is_disabled(), "the active-setup radio is live");
    assert!(h.query_by_label_contains("furniture").is_none(), "no in-code jargon on the page");
}
