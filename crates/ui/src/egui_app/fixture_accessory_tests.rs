//! #789 in the shell, structurally: the Fixtures node's menu and the
//! Fixtures page offer "Add accessory…"; the dialog lists the kinds and
//! their figures, and its Place queues one `InsertAccessory` (undo takes
//! it away); parallels fit under the stock in a built-in vise; a refusal
//! is named in the dialog; a lathe chuck offers "Fit to the stock" and
//! its grip direction (#1468); each placed accessory's row keeps its own size
//! draft and "Apply sizes" lands one `SetAccessory`. Driven through the
//! real dialog and page code in a kittest harness - no pixels (the
//! snapshot lane is release).
//! Provenance: our own tests over our own shell.

use egui::Vec2;
use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT, Queryable};

use camcraft_app::accessory::Accessory;
use camcraft_app::commands::{AccessoryPlacement, SetStock};
use camcraft_app::fixture;
use camcraft_app::model;
use camcraft_app::schema::Location;
use camcraft_app::stock::StockShape;

use crate::state::{CommandKind, FixtureLibDialog, State, TreeMenuTarget};

use super::fixture_library_dialog::window;
use super::fixture_menu::menu_rows;

fn with_stock() -> State {
    let mut state = State::new();
    let cmd = SetStock::new(&state.world, StockShape::Box { dx: 60.0, dy: 40.0, dz: 20.0 }, Location::default());
    state.history.execute(&state.world, cmd.expect("stock"));
    state.doc_version += 1;
    state
}

fn dialog(state: State) -> Harness<'static, State> {
    Harness::builder().with_size(Vec2::new(900.0, 1000.0)).build_ui_state(
        |ui, st: &mut State| {
            st.drain_actions();
            window(ui.ctx(), st);
        },
        state,
    )
}

fn params(state: &State, name: &str) -> Option<Accessory> {
    model::find_by_name(&state.world, name)
        .and_then(|e| fixture::def_of(&state.world, &e))
        .and_then(|d| d.accessory)
}

fn place(state: &mut State, kind: &str, x: f64) {
    let params = Accessory::default_of(kind).expect(kind);
    let placement = AccessoryPlacement::At(Location { pos: camcraft_app::geometry::Point3::new(x, 200.0, 0.0), ..Location::default() });
    state.pending.push_back(CommandKind::InsertAccessory { setup: None, params, placement });
    state.drain_actions();
}

#[test]
fn the_menu_and_the_page_offer_add_accessory() {
    let state = State::new();
    let rows = menu_rows(&state, &TreeMenuTarget::Fixtures("Setup2".into()));
    let row = rows.iter().find(|(l, _)| l == "Add accessory…").expect("menu row");
    assert!(matches!(&row.1, Some(CommandKind::AccessoryDialog(s)) if s == "Setup2"));

    let mut h = Harness::builder()
        .with_size(Vec2::new(600.0, 400.0))
        .build_ui_state(|ui, st: &mut State| super::pages::fixtures_section(ui, st, None), State::new());
    h.run();
    h.get_by_label("Add accessory…").click();
    h.run();
    assert!(matches!(h.state().pending.back(), Some(CommandKind::AccessoryDialog(s)) if s.is_empty()));
}

#[test]
fn the_dialog_places_a_strap_clamp_in_one_undoable_step() {
    let mut state = with_stock();
    state.pending.push_back(CommandKind::AccessoryDialog(String::new()));
    let mut h = dialog(state);
    h.run();
    for label in ["Add accessory", "Parallels", "1-2-3 block", "Step block", "Strap clamp", "Soft jaw", "Height", "Place"] {
        assert!(h.query_by_label(label).is_some(), "the dialog shows {label}");
    }
    h.get_by_label("Strap clamp").click();
    h.run();
    for label in ["Stud above the strap", "Clamp height", "Heel block length", "Origin X"] {
        assert!(h.query_by_label(label).is_some(), "the strap clamp shows {label}");
    }
    h.get_by_label("Place").click();
    h.run();
    h.run();
    assert!(h.state().fixture_lib.is_none(), "placed and closed: {:?}", h.state().fixture_lib);
    assert_eq!(params(h.state(), "Strap clamp").map(|a| a.kind()), Some("strap_clamp"));
    let st = h.state_mut();
    assert!(st.history.undo(&st.world));
    assert!(model::find_by_name(&st.world, "Strap clamp").is_none(), "one undo takes it");
}

#[test]
fn parallels_fit_under_the_stock_in_a_vise_from_the_dialog() {
    let mut state = with_stock();
    let six = camcraft_app::vise::size("6 in").map(camcraft_app::vise::ViseSize::half_open).expect("6 in");
    state.pending.push_back(CommandKind::InsertBuiltinVise { setup: None, params: six, fit_stock: true, pos: [0.0; 3], yaw_deg: 0.0 });
    state.pending.push_back(CommandKind::AccessoryDialog(String::new()));
    let mut h = dialog(state);
    h.run();
    let Some(FixtureLibDialog::Accessory(d)) = &h.state().fixture_lib else { panic!("open") };
    assert_eq!(d.in_vise.as_deref(), Some("Vise 6 in"), "parallels go into the setup's vise by default");
    assert!(h.query_by_label("Stock top over the jaws").is_some());
    h.get_by_label("Place").click();
    h.run();
    h.run();
    let Some(Accessory::Parallels { height, .. }) = params(h.state(), "Parallels") else { panic!("placed") };
    assert!((height - (1.75 * 25.4 + 6.35 - 20.0)).abs() < 1e-9, "{height}");
}

#[test]
fn the_fit_says_it_sinks_the_vise_and_holds_the_height() {
    // #812: in fit mode the height is the fit's - labelled so, not
    // editable - and the dialog says the vise drops under the stock.
    let mut state = with_stock();
    let six = camcraft_app::vise::size("6 in").map(camcraft_app::vise::ViseSize::half_open).expect("6 in");
    state.pending.push_back(CommandKind::InsertBuiltinVise { setup: None, params: six, fit_stock: true, pos: [0.0; 3], yaw_deg: 0.0 });
    state.pending.push_back(CommandKind::AccessoryDialog(String::new()));
    let mut h = dialog(state);
    h.run();
    let label = h.get_by_label("Height (set by the fit)");
    assert!(label.accesskit_node().is_disabled(), "the fit's height is not editable");
    assert!(h.query_by_label("Height").is_none(), "no plain, editable Height row while fitting");
    assert!(h.query_by_label_contains("the vise ends below the table").is_some(), "the sink is said before Place");
    // Out of fit mode the height is the user's again, and the hint goes.
    if let Some(FixtureLibDialog::Accessory(d)) = h.state_mut().fixture_lib.as_mut() {
        d.in_vise = None;
    }
    h.run();
    assert!(h.query_by_label("Height").is_some(), "the plain Height row is back");
    assert!(h.query_by_label("Height (set by the fit)").is_none());
    assert!(h.query_by_label_contains("the vise ends below the table").is_none());
}

#[test]
fn a_refused_accessory_is_named_in_the_dialog() {
    let mut state = State::new();
    state.pending.push_back(CommandKind::AccessoryDialog(String::new()));
    state.drain_actions();
    if let Some(FixtureLibDialog::Accessory(d)) = state.fixture_lib.as_mut() {
        d.pick("soft_jaw");
        d.params = d.params.clone().with_dim("height", 0.0).expect("dim");
    }
    let mut h = dialog(state);
    h.run();
    h.get_by_label("Place").click();
    h.run();
    h.run();
    let Some(FixtureLibDialog::Accessory(d)) = &h.state().fixture_lib else { panic!("stays open") };
    assert!(d.error.as_deref().is_some_and(|e| e.contains("height must be more than zero")), "{:?}", d.error);
    assert!(h.query_by_label_contains("height must be more than zero").is_some(), "said in the dialog");
}

#[test]
fn each_accessory_row_keeps_its_own_sizes_and_applies_them() {
    let mut state = with_stock();
    place(&mut state, "block", 200.0);
    place(&mut state, "soft_jaw", 400.0);
    let mut h = Harness::builder().with_size(Vec2::new(600.0, 1200.0)).build_ui_state(
        |ui, st: &mut State| {
            st.drain_actions();
            egui::ScrollArea::vertical().show(ui, |ui| super::pages::fixtures_section(ui, st, None));
        },
        state,
    );
    h.run();
    assert_eq!(h.get_all_by_label("Apply sizes").count(), 2, "each row offers its sizes");
    // Type into the block's draft; a frame of both rows keeps it.
    if let Some(d) = h.state_mut().accessory_sizes.get_mut("1-2-3 block") {
        d.value = d.value.clone().with_dim("height", 40.0).expect("dim");
    }
    h.run();
    let typed = h.state().accessory_sizes.get("1-2-3 block").map(|d| d.value.clone());
    assert_eq!(typed.map(|a| a.dims()[2].2), Some(40.0), "the soft jaw's row does not reset it");
    h.get_all_by_label("Apply sizes").next().expect("the block's row").click();
    h.run();
    h.run();
    let Some(Accessory::Block { height, .. }) = params(h.state(), "1-2-3 block") else { panic!("block") };
    assert_eq!(height, 40.0, "Apply lands through SetAccessory");
    assert_eq!(params(h.state(), "Soft jaw"), Accessory::default_of("soft_jaw"), "the other row is untouched");
    let st = h.state_mut();
    assert!(st.history.undo(&st.world));
    assert_eq!(params(st, "1-2-3 block"), Accessory::default_of("block"), "one undo each");
}

fn with_bar() -> State {
    let mut state = State::new();
    let at = Location { pos: camcraft_app::geometry::Point3::new(5.0, -3.0, -20.0), ..Location::default() };
    let cmd = SetStock::new(&state.world, StockShape::Cylinder { diameter: 30.0, height: 40.0 }, at);
    state.history.execute(&state.world, cmd.expect("bar"));
    state.doc_version += 1;
    state
}

fn draft(state: &State) -> &crate::state::AccessoryDraft {
    let Some(FixtureLibDialog::Accessory(d)) = &state.fixture_lib else { panic!("the dialog is open") };
    d
}

#[test]
fn chuck_dialog_offers_fit_to_the_stock() {
    use super::fixture_accessory_dialog::{FIT_STOCK_LABEL, fit_label};
    // #1468: on round stock the dialog fits a chuck by default.
    let mut state = with_bar();
    state.pending.push_back(CommandKind::AccessoryDialog(String::new()));
    let mut h = dialog(state);
    h.run();
    assert!(h.query_by_label(FIT_STOCK_LABEL).is_none(), "parallels have no stock fit");
    h.get_by_label("3-jaw chuck").click();
    h.run();
    h.run();
    let fit = h.get_by_label(FIT_STOCK_LABEL);
    assert_eq!(fit.accesskit_node().toggled(), Some(egui::accesskit::Toggled::True), "a chuck offers stock fitting, on for round stock");
    assert!(!fit.accesskit_node().is_disabled());
    // The fit sets the grip: labelled so, not editable, and no pose rows.
    assert!(h.get_by_label(fit_label("Grip diameter").as_str()).accesskit_node().is_disabled());
    assert!(h.query_by_label("Grip diameter").is_none() && h.query_by_label("Origin X").is_none());
    assert!(h.query_by_label_contains("closes on the setup's round stock").is_some(), "the fit is said before Place");
    for label in ["Grip", "Outside", "Inside (in a bore)", "Step height (0: plain jaws)", "Step depth (radial, under the work)"] {
        assert!(h.query_by_label(label).is_some(), "a jaw chuck shows {label}");
    }
    assert_eq!(draft(h.state()).placement(), AccessoryPlacement::OnStock);
    h.get_by_label("Place").click();
    h.run();
    h.run();
    assert!(h.state().fixture_lib.is_none(), "placed and closed: {:?}", h.state().fixture_lib);
    let Some(Accessory::ThreeJawChuck(c)) = params(h.state(), "3-jaw chuck") else { panic!("placed") };
    assert!((c.grip_diameter - 30.0).abs() < 1e-6, "closed on the 30 mm bar: {c:?}");
    let e = model::find_by_name(&h.state().world, "3-jaw chuck").expect("placed");
    let pos = model::transform_of(&h.state().world, &e).unwrap_or_default().pos;
    assert!((pos.x - 5.0).abs() < 1e-9 && (pos.y + 3.0).abs() < 1e-9 && (pos.z + 20.0).abs() < 1e-9, "on the bar's axis, at its underside: {pos:?}");
    let st = h.state_mut();
    assert!(st.history.undo(&st.world));
    assert!(model::find_by_name(&st.world, "3-jaw chuck").is_none(), "one undo takes it");
}

#[test]
fn chuck_fit_is_off_by_pose_for_an_inside_grip_and_square_stock() {
    use super::fixture_accessory_dialog::{FIT_STOCK_LABEL, fit_label};
    // Square stock: the box opens unticked (a fit would be refused); ticked,
    // Place names the refusal in the dialog.
    let mut state = with_stock();
    state.pending.push_back(CommandKind::AccessoryDialog(String::new()));
    let mut h = dialog(state);
    h.run();
    h.get_by_label("Collet chuck").click();
    h.run();
    h.run();
    let fit = h.get_by_label(FIT_STOCK_LABEL);
    assert_eq!(fit.accesskit_node().toggled(), Some(egui::accesskit::Toggled::False), "square stock: by pose");
    assert!(h.query_by_label("Collet bore").is_some() && h.query_by_label("Origin X").is_some());
    assert!(matches!(draft(h.state()).placement(), AccessoryPlacement::At(_)));
    fit.click();
    h.run();
    h.run();
    assert!(h.get_by_label(fit_label("Collet bore").as_str()).accesskit_node().is_disabled());
    h.get_by_label("Place").click();
    h.run();
    h.run();
    assert!(draft(h.state()).error.as_deref().is_some_and(|e| e.contains("only round stock")), "{:?}", draft(h.state()).error);
    assert!(h.query_by_label_contains("only round stock").is_some(), "said in the dialog");

    // An inside grip: the fit is unticked and disabled, with the reason,
    // and Place goes by pose.
    let mut state = with_bar();
    state.pending.push_back(CommandKind::AccessoryDialog(String::new()));
    let mut h = dialog(state);
    h.run();
    h.get_by_label("4-jaw chuck").click();
    h.run();
    h.run();
    assert_eq!(draft(h.state()).placement(), AccessoryPlacement::OnStock);
    h.get_by_label("Inside (in a bore)").click();
    h.run();
    h.run();
    let fit = h.get_by_label(FIT_STOCK_LABEL);
    assert!(fit.accesskit_node().is_disabled(), "an inside grip has no stock fit");
    assert_eq!(fit.accesskit_node().toggled(), Some(egui::accesskit::Toggled::False));
    assert!(h.query_by_label_contains("An inside grip opens into the work's bore").is_some());
    assert!(h.query_by_label("Grip diameter").is_some() && h.query_by_label("Origin X").is_some(), "by pose, the grip typed");
    assert!(matches!(draft(h.state()).placement(), AccessoryPlacement::At(_)));
    assert_eq!(draft(h.state()).params.grip(), Some(camcraft_app::accessory::chuck::Grip::Inside));
}
