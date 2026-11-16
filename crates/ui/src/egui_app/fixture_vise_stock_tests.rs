//! #1105 in the vise dialog, structurally: with no stock body the dialog
//! says so before Place (the stock is the block the part is cut from, not
//! the imported model), greys "Fit to the stock" with its reason, offers a
//! button to the Stock page and shows the position rows; with a stock it
//! says where the vise lands and how far the jaws open, and a stock wider
//! than the vise opens is named up front. The browser's Stock row reads
//! "not set" while the setup has no stock body. No pixels.
//! Provenance: our own tests over our own shell.

use egui::Vec2;
use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT, Queryable};

use camcraft_app::commands::SetStock;
use camcraft_app::schema::Location;
use camcraft_app::stock::StockShape;

use crate::state::{CommandKind, FixtureLibDialog, State, UiTarget};

use super::fixture_library_dialog::window;

const FIT: &str = "Fit to the stock (jaws open to it)";

fn stocked(dy: f64) -> State {
    let mut state = State::new();
    let cmd = SetStock::new(&state.world, StockShape::Box { dx: 60.0, dy, dz: 20.0 }, Location::default());
    state.history.execute(&state.world, cmd.expect("stock"));
    state.doc_version += 1;
    state
}

fn dialog(mut state: State) -> Harness<'static, State> {
    state.pending.push_back(CommandKind::BuiltinViseDialog(String::new()));
    let mut h = Harness::builder().with_size(Vec2::new(900.0, 1000.0)).build_ui_state(
        |ui, st: &mut State| {
            st.drain_actions();
            window(ui.ctx(), st);
        },
        state,
    );
    h.run();
    h.run();
    h
}

#[test]
fn without_a_stock_the_dialog_explains_greys_fit_and_offers_the_stock_page() {
    let mut h = dialog(State::new());
    assert!(h.query_by_label_contains("This setup has no stock yet").is_some(), "the notice");
    assert!(h.query_by_label_contains("not the imported model").is_some(), "the stock, not the model");
    assert!(h.query_by_label_contains("Set the stock on the Stock page").is_some());
    assert!(h.get_by_label(FIT).accesskit_node().is_disabled(), "Fit is greyed");
    assert!(h.query_by_label("Fixed jaw X").is_some(), "the position rows show");
    // Place by position works with no stock.
    h.get_by_label("Place").click();
    h.run();
    h.run();
    assert!(h.state().fixture_lib.is_none(), "placed by position: {:?}", h.state().fixture_lib);
    assert!(camcraft_app::model::find_by_name(&h.state().world, "Vise 150 mm").is_some());
}

#[test]
fn the_stock_page_button_takes_the_browsers_stock_path() {
    let mut h = dialog(State::new());
    h.get_by_label("Open the Stock page").click();
    h.run();
    assert!(h.state().fixture_lib.is_none(), "the dialog closes");
    assert!(matches!(h.state().ui_target, UiTarget::Stock), "{:?}", h.state().ui_target);
}

#[test]
fn with_a_stock_the_dialog_says_where_the_vise_lands() {
    let h = dialog(stocked(40.0));
    assert!(h.query_by_label_contains("no stock yet").is_none());
    assert!(!h.get_by_label(FIT).accesskit_node().is_disabled());
    assert!(h.query_by_label_contains("not the imported model").is_some());
    assert!(h.query_by_label_contains("Opens to 40.000 mm on the stock's Y width").is_some());
    assert!(h.query_by_label("Fixed jaw X").is_none(), "Fit hides the position rows");
    assert!(h.query_by_label("Open the Stock page").is_none());
}

#[test]
fn a_stock_wider_than_the_vise_is_named_before_place() {
    let h = dialog(stocked(400.0));
    assert!(h.query_by_label_contains("cannot be fitted to the stock").is_some());
    assert!(h.query_by_label_contains("opens").is_some());
}

#[test]
fn a_refused_fit_from_the_queue_is_named_in_the_dialog_in_plain_english() {
    let mut h = dialog(State::new());
    let params = match &h.state().fixture_lib {
        Some(FixtureLibDialog::Vise(d)) => d.params.clone(),
        _ => panic!("vise dialog open"),
    };
    h.state_mut().pending.push_back(CommandKind::InsertBuiltinVise {
        setup: None,
        params,
        fit_stock: true,
        pos: [0.0; 3],
        yaw_deg: 0.0,
    });
    h.run();
    h.run();
    let Some(FixtureLibDialog::Vise(d)) = &h.state().fixture_lib else { panic!("stays open") };
    let e = d.error.as_deref().unwrap_or_default();
    assert!(e.contains("has no stock yet") && e.contains("Stock page") && !e.contains("invalid parameters"), "{e}");
}

#[test]
fn the_browsers_stock_row_reads_not_set_until_the_stock_has_a_body() {
    let row = |state: &State| {
        let (rows, _) = super::panes::browser_rows(state);
        rows.iter().find(|r| r.label == "Stock").map(|r| r.suffix.clone())
    };
    assert_eq!(row(&State::new()).as_deref(), Some("not set"));
    assert_eq!(row(&stocked(40.0)).as_deref(), Some(""));
}
