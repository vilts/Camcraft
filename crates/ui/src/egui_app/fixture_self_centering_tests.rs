//! #787 in the shell, structurally: the built-in vise dialog lists the
//! 5-axis self-centering sizes in their own row; picking one swaps the
//! screw-housing fields for the clamping range floor and the dovetail
//! lip, and Place fits it centred on the stock in one undoable step; a
//! stock outside the clamping range is refused by name in the dialog;
//! the placed vise's row closes its jaws on the stock through `SetVise`.
//! Driven through the real dialog and page code in a kittest harness - no
//! pixels (the snapshot lane is release).
//! Provenance: our own tests over our own shell.

use egui::Vec2;
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;

use camcraft_app::commands::SetStock;
use camcraft_app::fixture;
use camcraft_app::model;
use camcraft_app::schema::Location;
use camcraft_app::stock::StockShape;

use crate::state::{CommandKind, FixtureLibDialog, State};

use super::fixture_library_dialog::window;

const NAME: &str = "Vise 80 mm self-centering";

fn with_stock(dy: f64) -> State {
    let mut state = State::new();
    let cmd = SetStock::new(&state.world, StockShape::Box { dx: 60.0, dy, dz: 20.0 }, Location::default());
    state.history.execute(&state.world, cmd.expect("stock"));
    state.doc_version += 1;
    state
}

fn dialog(state: State) -> Harness<'static, State> {
    Harness::builder().with_size(Vec2::new(900.0, 1100.0)).build_ui_state(
        |ui, st: &mut State| {
            st.drain_actions();
            window(ui.ctx(), st);
        },
        state,
    )
}

fn opened(mut state: State) -> Harness<'static, State> {
    state.pending.push_back(CommandKind::BuiltinViseDialog(String::new()));
    let mut h = dialog(state);
    h.run();
    h
}

fn opening(state: &State, name: &str) -> Option<f64> {
    model::find_by_name(&state.world, name)
        .and_then(|e| fixture::def_of(&state.world, &e))
        .and_then(|d| d.vise)
        .map(|v| v.opening)
}

#[test]
fn the_dialog_lists_the_self_centering_sizes_and_their_own_fields() {
    let mut h = opened(with_stock(40.0));
    for label in ["5-axis self-centering", "52 mm self-centering", "75 mm self-centering", "80 mm self-centering", "125 mm self-centering", "150 mm"] {
        assert!(h.query_by_label(label).is_some(), "the dialog shows {label}");
    }
    assert!(h.query_by_label("Housing length").is_some(), "a standard size has its housing");
    assert!(h.query_by_label("Min opening").is_none(), "and no clamping-range floor");
    h.get_by_label("80 mm self-centering").click();
    h.run();
    let Some(FixtureLibDialog::Vise(d)) = &h.state().fixture_lib else { panic!("still open") };
    assert_eq!(d.params.size, "80 mm self-centering");
    for label in ["Min opening", "Dovetail lip height", "Dovetail lip depth", "Fit to the stock (centred, jaws closed on it)"] {
        assert!(h.query_by_label(label).is_some(), "a self-centering size shows {label}");
    }
    assert!(h.query_by_label("Housing length").is_none(), "and no screw housing");
    h.get_by_label("Fit to the stock (centred, jaws closed on it)").click();
    h.run();
    assert!(h.query_by_label("Centre X").is_some(), "a typed pose places its centre");
}

#[test]
fn the_dialog_places_a_centred_self_centering_vise_in_one_undoable_step() {
    let mut h = opened(with_stock(40.0));
    h.get_by_label("80 mm self-centering").click();
    h.run();
    h.get_by_label("Place").click();
    h.run();
    h.run();
    assert!(h.state().fixture_lib.is_none(), "placed and closed: {:?}", h.state().fixture_lib);
    assert_eq!(opening(h.state(), NAME), Some(40.0), "jaws closed on the 40 mm stock");
    let st = h.state();
    let pos = model::find_by_name(&st.world, NAME).and_then(|e| model::transform_of(&st.world, &e)).map(|l| l.pos);
    assert_eq!(pos.map(|p| [p.x, p.y, p.z]), Some([0.0, 0.0, 0.0]), "centred on the stock");
    let st = h.state_mut();
    assert!(st.history.undo(&st.world));
    assert!(model::find_by_name(&st.world, NAME).is_none(), "one undo takes the vise");
}

#[test]
fn a_stock_outside_the_clamping_range_is_named_in_the_dialog() {
    let mut h = opened(with_stock(40.0));
    h.get_by_label("52 mm self-centering").click();
    h.run();
    h.run();
    // #1105: said up front, before Place (once), and again after it.
    assert_eq!(h.query_all_by_label_contains("wider than its clamping range").count(), 1, "said before Place");
    h.get_by_label("Place").click();
    h.run();
    h.run();
    let Some(FixtureLibDialog::Vise(d)) = &h.state().fixture_lib else { panic!("stays open") };
    assert!(d.error.as_deref().is_some_and(|e| e.contains("wider than its clamping range")), "{:?}", d.error);
    assert!(h.query_all_by_label_contains("wider than its clamping range").count() >= 1, "said in the dialog");
}

#[test]
fn a_self_centering_row_closes_its_jaws_on_the_stock() {
    let mut state = with_stock(40.0);
    state.pending.push_back(CommandKind::BuiltinViseDialog(String::new()));
    state.drain_actions();
    let Some(FixtureLibDialog::Vise(d)) = state.fixture_lib.as_mut() else { panic!("open") };
    let k = camcraft_app::vise::presets().position(|r| r.label == "80 mm self-centering").expect("row");
    d.pick(k);
    let cmd = super::fixture_vise_dialog::place_command(d);
    state.pending.push_back(cmd);
    state.drain_actions();
    assert_eq!(opening(&state, NAME), Some(40.0));

    let mut h = Harness::builder().with_size(Vec2::new(600.0, 700.0)).build_ui_state(
        |ui, st: &mut State| {
            st.drain_actions();
            super::pages::fixtures_section(ui, st, None);
        },
        state,
    );
    h.run();
    assert!(h.query_by_label_contains("Built-in vise 80 mm self-centering").is_some(), "the row names it");
    *h.state_mut().vise_opening_draft(NAME, 40.0) = 100.0;
    h.get_by_label("Set opening").click();
    h.run();
    h.run();
    assert_eq!(opening(h.state(), NAME), Some(100.0), "Set lands through SetVise");
    h.get_by_label("Open to stock").click();
    h.run();
    h.run();
    assert_eq!(opening(h.state(), NAME), Some(40.0), "closed on the stock again");
    let st = h.state_mut();
    assert!(st.history.undo(&st.world));
    assert_eq!(opening(st, NAME), Some(100.0), "each edit is one undo");
}
