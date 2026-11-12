//! #820 in the shell, structurally: a plate's row on the Fixtures page
//! offers "Add mod vise pair…" (presets, the fixed jaw's row and column,
//! the way its jaw faces, soft jaws); "Place pair" queues one
//! `PlaceModVisePair` that lands both jaw blocks on the plate (one undo
//! takes both), and each block's row says which side it is and how far
//! it slid; a refusal is named in the status. #832: a block's row edits
//! its figures ("Edit jaw block…", "Set figures" - one `SetModJaw`) and
//! the adjustable block's row re-fits it to a changed stock ("Re-fit to
//! stock" - one `RefitModVise`). Driven through the real page code in a
//! kittest harness - no pixels (the snapshot lane is release).
//! Provenance: our own tests over our own shell.

use egui::Vec2;
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;

use camcraft_app::commands::{SetStock, SetUnits};
use camcraft_app::fixture;
use camcraft_app::model;
use camcraft_app::plate;
use camcraft_app::schema::Location;
use camcraft_app::stock::StockShape;
use camcraft_app::units::Unit;

use crate::state::{CommandKind, State};

fn page(state: State) -> Harness<'static, State> {
    Harness::builder().with_size(Vec2::new(700.0, 1600.0)).build_ui_state(
        |ui, st: &mut State| {
            st.drain_actions();
            egui::ScrollArea::vertical().show(ui, |ui| super::pages::fixtures_section(ui, st, None));
        },
        state,
    )
}

/// A 12 in plate laid so a 1 in preset fixed jaw at C3 has its face on
/// the -Y side of a 60 mm stock; with or without the stock.
fn plated(stock: bool) -> State {
    let mut state = State::new();
    if stock {
        let cmd = SetStock::new(&state.world, StockShape::Box { dx: 60.0, dy: 60.0, dz: 20.0 }, Location::default()).expect("stock");
        state.history.execute(&state.world, cmd);
    }
    let p = plate::preset("12 x 12 in").map(plate::PlatePreset::params).expect("preset");
    let pos = [-25.4 - 2.5 * 25.4, -30.0 - 1.75 * 25.4 - 2.5 * 25.4, 0.0];
    state.pending.push_back(CommandKind::InsertFixturePlate { setup: None, params: p, on_table: false, pos, yaw_deg: 0.0 });
    state.drain_actions();
    let d = state.mod_vise_draft("Plate 12 x 12 in");
    d.row = 2; // C
    d.col = 2; // 3
    state
}

fn names(state: &State) -> Vec<String> {
    model::named_entities(&state.world).into_iter().map(|(n, _)| n).filter(|n| n.starts_with("Mod vise")).collect()
}

#[test]
fn a_plate_row_places_a_mod_vise_pair_in_one_undoable_step() {
    let mut h = page(plated(true));
    h.run();
    h.get_by_label("Add mod vise pair…").click();
    h.run();
    for label in ["1 in grid 3/8-16", "2 in grid 1/2-13", "50 mm grid M12", "Fixed jaw at row", "+Y", "-X", "Soft jaws", "Place pair"] {
        assert!(h.query_by_label(label).is_some(), "the section shows {label}");
    }
    h.get_by_label("1 in grid 3/8-16").click();
    h.run();
    h.get_by_label("Soft jaws").click();
    h.run();
    h.get_by_label("Place pair").click();
    h.run();
    h.run();
    let st = h.state();
    assert!(st.status.contains("fixed at C3, adjustable at I5"), "{}", st.status);
    assert!(st.status.ends_with(" mm to the stock"), "the slide in document units: {}", st.status);
    let mut placed = names(st);
    placed.sort();
    assert_eq!(placed, ["Mod vise adjustable 1 in grid 3/8-16", "Mod vise fixed 1 in grid 3/8-16"]);
    let def = model::find_by_name(&st.world, &placed[0]).and_then(|e| fixture::def_of(&st.world, &e)).expect("block");
    assert_eq!(def.mod_jaw.map(|j| j.insert), Some(camcraft_app::modvise::JawInsert::Soft));
    assert!(h.query_by_label_contains("Mod vise adjustable jaw block (1 in grid 3/8-16), soft jaws, slid").is_some());
    assert!(h.query_by_label_contains("Mod vise fixed jaw block").is_some());
    assert!(h.query_by_label_contains("Carries: Mod vise").is_some(), "the plate carries them");
    let st = h.state_mut();
    assert!(st.history.undo(&st.world));
    assert!(names(st).is_empty(), "one undo takes both blocks");
}

#[test]
fn a_refused_pair_is_named_in_the_status() {
    let mut h = page(plated(false));
    h.run();
    h.get_by_label("Add mod vise pair…").click();
    h.run();
    h.get_by_label("Place pair").click();
    h.run();
    h.run();
    assert!(h.state().status.contains("mod vise not placed") && h.state().status.contains("no stock"), "{}", h.state().status);
    assert!(names(h.state()).is_empty());
}

#[test]
fn the_placed_status_states_the_slide_in_document_units() {
    let mut state = plated(true);
    let cmd = SetUnits::new(&state.world, Unit::Inches).expect("units");
    state.history.execute(&state.world, cmd);
    let mut h = page(state);
    h.run();
    h.get_by_label("Add mod vise pair…").click();
    h.run();
    h.get_by_label("1 in grid 3/8-16").click();
    h.run();
    h.get_by_label("Place pair").click();
    h.run();
    h.run();
    let st = h.state();
    assert!(st.status.contains("fixed at C3, adjustable at I5"), "{}", st.status);
    assert!(st.status.ends_with(" in to the stock") && !st.status.contains("mm"), "an inch document reads inches: {}", st.status);
}

/// The pair placed by the page (fixed at C3, adjustable at I5).
fn placed() -> Harness<'static, State> {
    let mut h = page(plated(true));
    h.run();
    h.get_by_label("Add mod vise pair…").click();
    h.run();
    h.get_by_label("1 in grid 3/8-16").click();
    h.run();
    h.get_by_label("Place pair").click();
    h.run();
    h.run();
    assert!(h.state().status.contains("fixed at C3, adjustable at I5"), "{}", h.state().status);
    h
}

fn jaw(st: &State, name: &str) -> camcraft_app::modvise::JawParams {
    model::find_by_name(&st.world, name).and_then(|e| fixture::def_of(&st.world, &e)).and_then(|d| d.mod_jaw).expect("a jaw block")
}

#[test]
fn a_jaw_block_row_edits_its_figures_in_one_undoable_step() {
    let mut h = placed();
    let headers = h.query_all_by_label("Edit jaw block…").count();
    assert_eq!(headers, 2, "each block's row offers the edit");
    for k in 0..2 {
        h.query_all_by_label("Edit jaw block…").nth(k).expect("header").click();
        h.run();
    }
    for label in ["Body height", "Jaw height", "Jaw thickness", "Face offset", "Slide", "Soft jaw", "Set figures"] {
        assert!(h.query_all_by_label(label).next().is_some(), "the editor shows {label}");
    }
    assert_eq!(h.query_all_by_label("Slide").count(), 1, "only the adjustable block slides");
    for n in names(h.state()) {
        let cur = jaw(h.state(), &n);
        h.state_mut().mod_jaw_draft(&n, &cur).jaw_height = 20.0;
    }
    h.query_all_by_label("Set figures").next().expect("button").click();
    h.run();
    h.run();
    let st = h.state();
    let set: Vec<String> = names(st).into_iter().filter(|n| (jaw(st, n).jaw_height - 20.0).abs() < 1e-12).collect();
    assert_eq!(set.len(), 1, "one block set: {}", st.status);
    assert!(st.status.contains(&format!("{}: jaw block figures set", set[0])), "{}", st.status);
    let st = h.state_mut();
    assert!(st.history.undo(&st.world));
    assert!((jaw(st, &set[0]).jaw_height - 1.25 * 25.4).abs() < 1e-12, "one undo puts the figures back");
}

#[test]
fn the_adjustable_row_re_fits_the_block_to_a_changed_stock() {
    let mut h = placed();
    assert_eq!(h.query_all_by_label("Re-fit to stock").count(), 1, "only the adjustable block re-fits");
    // A 40 mm deep stock, its near side kept on the fixed jaw (y -30).
    let st = h.state_mut();
    let cmd = SetStock::new(&st.world, StockShape::Box { dx: 60.0, dy: 40.0, dz: 20.0 }, Location { pos: camcraft_app::geometry::Point3::new(0.0, -10.0, 0.0), ..Location::default() }).expect("stock");
    st.history.execute(&st.world, cmd);
    h.get_by_label("Re-fit to stock").click();
    h.run();
    h.run();
    let st = h.state();
    assert!(st.status.contains("mod vise re-fitted: Mod vise adjustable 1 in grid 3/8-16 at H5"), "{}", st.status);
    assert!(st.status.ends_with(" mm to the stock"), "{}", st.status);
    let st = h.state_mut();
    assert!(st.history.undo(&st.world));
    let at = model::find_by_name(&st.world, "Mod vise adjustable 1 in grid 3/8-16").and_then(|e| fixture::def_of(&st.world, &e)).and_then(|d| d.plate.mount).map(|m| m.at);
    assert_eq!(at, Some(camcraft_app::plate::MountAt::Hole("I5".into())), "one undo puts it back");
}

#[test]
fn a_refused_refit_is_named_in_the_status() {
    let mut h = placed();
    // The stock pushed into the fixed jaw.
    let st = h.state_mut();
    let cmd = SetStock::new(&st.world, StockShape::Box { dx: 60.0, dy: 60.0, dz: 20.0 }, Location { pos: camcraft_app::geometry::Point3::new(0.0, -5.0, 0.0), ..Location::default() }).expect("stock");
    st.history.execute(&st.world, cmd);
    h.get_by_label("Re-fit to stock").click();
    h.run();
    h.run();
    let s = &h.state().status;
    assert!(s.contains("mod vise not re-fitted") && s.contains("into the fixed jaw block at C3"), "{s}");
}
