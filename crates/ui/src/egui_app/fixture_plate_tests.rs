//! #819 in the shell, structurally: the Fixtures node's menu and the
//! Fixtures page offer "Add fixture plate…"; the dialog lists the
//! presets and its Place queues one `InsertPlate` (undo takes it away);
//! a refusal is named in the dialog; a fixture row's "Place on plate…"
//! picker puts a vise on a grid location through `PlaceOnPlate`, and the
//! plate's row then names its grid and what it carries; an imported
//! fixture offers "Detect plate grid". Driven through the real dialog and
//! page code in a kittest harness - no pixels (the snapshot lane is release).
//! Provenance: our own tests over our own shell.

use egui::Vec2;
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;

use camcraft_app::fixture;
use camcraft_app::model;
use camcraft_app::plate::{self, MountAt};

use crate::state::{CommandKind, FixtureLibDialog, State, TreeMenuTarget};

use super::fixture_library_dialog::window;
use super::fixture_menu::menu_rows;

fn dialog(state: State) -> Harness<'static, State> {
    Harness::builder().with_size(Vec2::new(900.0, 1100.0)).build_ui_state(
        |ui, st: &mut State| {
            st.drain_actions();
            window(ui.ctx(), st);
        },
        state,
    )
}

fn page(state: State) -> Harness<'static, State> {
    Harness::builder().with_size(Vec2::new(700.0, 1100.0)).build_ui_state(
        |ui, st: &mut State| {
            st.drain_actions();
            egui::ScrollArea::vertical().show(ui, |ui| super::pages::fixtures_section(ui, st, None));
        },
        state,
    )
}

fn mount(state: &State, name: &str) -> Option<plate::PlateMount> {
    model::find_by_name(&state.world, name).and_then(|e| fixture::def_of(&state.world, &e)).and_then(|d| d.plate.mount)
}

#[test]
fn the_menu_and_the_page_offer_add_fixture_plate() {
    let state = State::new();
    let rows = menu_rows(&state, &TreeMenuTarget::Fixtures("Setup2".into()));
    let row = rows.iter().find(|(l, _)| l == "Add fixture plate…").expect("menu row");
    assert!(matches!(&row.1, Some(CommandKind::FixturePlateDialog(s)) if s == "Setup2"));

    let mut h = Harness::builder()
        .with_size(Vec2::new(600.0, 400.0))
        .build_ui_state(|ui, st: &mut State| super::pages::fixtures_section(ui, st, None), State::new());
    h.run();
    h.get_by_label("Add fixture plate…").click();
    h.run();
    assert!(matches!(h.state().pending.back(), Some(CommandKind::FixturePlateDialog(s)) if s.is_empty()));
}

#[test]
fn the_dialog_places_a_plate_in_one_undoable_step() {
    let mut state = State::new();
    state.pending.push_back(CommandKind::FixturePlateDialog(String::new()));
    let mut h = dialog(state);
    h.run();
    for label in ["Add fixture plate", "12 x 12 in", "300 x 300 mm", "770MX table 28 x 15 in", "Pitch X", "Thread", "Dowel holes", "Place"] {
        assert!(h.query_by_label(label).is_some(), "the dialog shows {label}");
    }
    let Some(FixtureLibDialog::Plate(d)) = &h.state().fixture_lib else { panic!("plate dialog open") };
    assert_eq!(d.params.size, "300 x 300 mm", "a mm document starts on 300 mm");
    assert!(!d.on_table, "no machine, so not on a table");
    assert!(d.dowels_on, "the 300 mm preset has dowels");
    h.get_by_label("12 x 12 in").click();
    h.run();
    h.get_by_label("Place").click();
    h.run();
    h.run();
    assert!(h.state().fixture_lib.is_none(), "placed and closed: {:?}", h.state().fixture_lib);
    let name = "Plate 12 x 12 in";
    let def = model::find_by_name(&h.state().world, name).and_then(|e| fixture::def_of(&h.state().world, &e)).expect("placed");
    assert_eq!(def.plate.plate.map(|p| (p.holes.rows, p.holes.cols)), Some((12, 12)));
    let st = h.state_mut();
    assert!(st.history.undo(&st.world));
    assert!(model::find_by_name(&st.world, name).is_none(), "one undo takes the plate");
}

#[test]
fn a_refused_plate_is_named_in_the_dialog() {
    let mut state = State::new();
    state.pending.push_back(CommandKind::FixturePlateDialog(String::new()));
    state.drain_actions();
    if let Some(FixtureLibDialog::Plate(d)) = state.fixture_lib.as_mut() {
        d.on_table = true;
    }
    let mut h = dialog(state);
    h.run();
    h.get_by_label("Place").click();
    h.run();
    h.run();
    let Some(FixtureLibDialog::Plate(d)) = &h.state().fixture_lib else { panic!("stays open") };
    assert!(d.error.as_deref().is_some_and(|e| e.contains("no table")), "{:?}", d.error);
    assert!(h.query_by_label_contains("no table").is_some(), "said in the dialog");
}

#[test]
fn a_fixture_row_places_a_vise_on_a_grid_location() {
    let mut state = State::new();
    let p = plate::preset("770MX table 28 x 15 in").map(plate::PlatePreset::params).expect("preset");
    state.pending.push_back(CommandKind::InsertFixturePlate { setup: None, params: p, on_table: false, pos: [0.0; 3], yaw_deg: 0.0 });
    let six = camcraft_app::vise::size("6 in").map(camcraft_app::vise::ViseSize::half_open).expect("6 in");
    state.pending.push_back(CommandKind::InsertBuiltinVise { setup: None, params: six, fit_stock: false, pos: [0.0, 0.0, 300.0], yaw_deg: 0.0 });
    state.drain_actions();
    let vise = "Vise 6 in";
    {
        let d = state.plate_place_draft(vise);
        d.row = 7; // H
        d.col = 9; // 10
        d.yaw_deg = 90.0;
    }
    let mut h = page(state);
    h.run();
    assert!(h.query_by_label_contains("Fixture plate: rows A-O, columns 1-28").is_some(), "the plate's row names its grid");
    assert!(h.query_by_label("Detect plate grid").is_none(), "a built-in plate needs no detection, a vise neither");
    h.get_by_label("Place on plate…").click();
    h.run();
    h.get_by_label("Place on grid").click();
    h.run();
    h.run();
    assert_eq!(mount(h.state(), vise).map(|m| m.at), Some(MountAt::Hole("H10".into())), "{}", h.state().status);
    assert!(h.query_by_label_contains("Carries: Vise 6 in").is_some());
    assert!(h.query_by_label_contains("On plate Plate 770MX table 28 x 15 in at H10").is_some());
    let st = h.state_mut();
    assert!(st.history.undo(&st.world));
    assert!(mount(st, vise).is_none(), "one undo");
}

#[test]
fn an_imported_fixture_offers_detect_plate_grid() {
    let mut state = State::new();
    state.history.execute(&state.world, camcraft_app::commands::ImportModel::from_mesh("Block", camcraft_app::geometry::Mesh::from_facets(plate::slab(10.0, 10.0, 10.0))));
    state.history.execute(&state.world, camcraft_app::commands::TagFixture::new(&state.world, "Block", None).expect("tag"));
    state.doc_version += 1;
    let mut h = page(state);
    h.run();
    h.get_by_label("Detect plate grid").click();
    h.run();
    h.run();
    assert!(h.state().status.contains("no plate grid") && h.state().status.contains("no holes found"), "{}", h.state().status);
    let def = model::find_by_name(&h.state().world, "Block").and_then(|e| fixture::def_of(&h.state().world, &e)).expect("fixture");
    assert!(def.plate.plate.is_none(), "a refused detection changes nothing");
}

/// #1119: every input in the fixture family carries an accessible name -
/// the label its row shows (`labelled_by`) or a plain name. Returns the
/// role and drawn value of each input that has neither, and how many
/// inputs were looked at.
pub(super) fn unnamed_inputs<S>(h: &Harness<'static, S>) -> (Vec<String>, usize) {
    use egui::accesskit::Role;
    use egui_kittest::kittest::NodeT;
    let (mut bad, mut seen) = (Vec::new(), 0);
    for n in h.root().children_recursive() {
        let node = n.accesskit_node();
        if !matches!(node.role(), Role::SpinButton | Role::TextInput | Role::MultilineTextInput | Role::ComboBox | Role::Slider) {
            continue;
        }
        seen += 1;
        let named = node.labelled_by().next().is_some() || node.data().label().is_some_and(|l| !l.is_empty());
        if !named {
            bad.push(format!("{:?} = {:?}", node.role(), node.value()));
        }
    }
    (bad, seen)
}

#[test]
fn the_fixture_dialogs_name_every_input() {
    for (what, cmd, at_least) in [
        ("plate dialog", CommandKind::FixturePlateDialog(String::new()), 6),
        ("vise dialog", CommandKind::BuiltinViseDialog(String::new()), 8),
        ("accessory dialog", CommandKind::AccessoryDialog(String::new()), 3),
        ("library save dialog", CommandKind::FixtureLibrarySaveDialog(String::new()), 0),
    ] {
        let mut state = State::new();
        state.pending.push_back(cmd);
        let mut h = dialog(state);
        h.run();
        h.run();
        let (bad, seen) = unnamed_inputs(&h);
        assert!(seen >= at_least, "{what}: only {seen} inputs found");
        assert!(bad.is_empty(), "{what}: unnamed inputs {bad:?}");
    }
}

#[test]
fn the_fixture_rows_name_every_input() {
    let mut state = State::new();
    let p = plate::preset("770MX table 28 x 15 in").map(plate::PlatePreset::params).expect("preset");
    state.pending.push_back(CommandKind::InsertFixturePlate { setup: None, params: p, on_table: false, pos: [0.0; 3], yaw_deg: 0.0 });
    let six = camcraft_app::vise::size("6 in").map(camcraft_app::vise::ViseSize::half_open).expect("6 in");
    state.pending.push_back(CommandKind::InsertBuiltinVise { setup: None, params: six, fit_stock: false, pos: [0.0, 0.0, 300.0], yaw_deg: 0.0 });
    state.drain_actions();
    let mut h = page(state);
    h.run();
    h.get_by_label("Place on plate…").click();
    h.run();
    h.get_by_label("Add mod vise pair…").click();
    h.run();
    h.run();
    let (bad, seen) = unnamed_inputs(&h);
    assert!(seen >= 4, "only {seen} inputs found");
    assert!(bad.is_empty(), "unnamed inputs {bad:?}");
}
