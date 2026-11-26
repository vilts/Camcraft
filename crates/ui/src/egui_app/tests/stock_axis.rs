//! #2576: the Stock page's axis choice - Along Z / X / Y / Custom - lands
//! the whole turn through one undoable `SetStock`, reads a turned stock
//! back, and says before any simulation when the simulation cannot cut
//! the drafted stock.
//! Provenance: our own page law over the real shell.

use super::*;
use camcraft_app::geometry::stock_axis::{StockAxis, stock_axis_rot};
use camcraft_app::stock::StockShape;
use crate::state::SIM_NOTE_TILTED;

type H = Harness<'static, ShellUi>;

/// The seeded shell on the Stock page, with a 40 x 100 cylinder set
/// upright through the page's command.
fn stock_page_with_cylinder() -> H {
    // A tall window, so Apply is on screen under the axis rows.
    let mut h = Harness::builder().with_size(Vec2::new(1280.0, 1000.0)).build_ui_state(frame, ShellUi::seeded());
    h.state_mut().state.ui_target = UiTarget::Stock;
    let cyl = StockShape::Cylinder { diameter: 40.0, height: 100.0 };
    h.state_mut().state.pending.push_back(CommandKind::SetStockPose { shape: cyl, pos: [0.0; 3], rot: [0.0, 0.0, 0.0, 1.0] });
    h.run_steps(3);
    h
}

/// The stored turn of the active setup's stock.
fn stock_rot(h: &H) -> [f64; 4] {
    let world = &h.state().state.world;
    let name = camcraft_app::setup::stock_entity_name(world, camcraft_app::setup::active(world).as_deref());
    let Some(stock) = camcraft_app::model::find_by_name(world, &name) else {
        panic!("no stock entity {name}");
    };
    camcraft_app::model::transform_of(world, &stock).unwrap_or_default().rot
}

fn same_turn(a: [f64; 4], b: [f64; 4]) -> bool {
    let dot: f64 = a.iter().zip(b).map(|(x, y)| x * y).sum();
    1.0 - dot.abs() <= 1e-9
}

/// The axis radios and the custom angle rows are labelled widgets; the
/// angle rows show only for Custom.
#[test]
fn the_stock_page_offers_the_stock_axis_and_custom_angles() {
    let mut h = stock_page_with_cylinder();
    for label in ["Stock axis", "Along Z", "Along X", "Along Y", "Custom"] {
        assert!(h.query_by_label(label).is_some(), "the page shows {label:?}");
    }
    assert!(h.query_by_label("Turn about Z (yaw)").is_none(), "no angle rows for Along Z");
    h.get_by_label("Custom").click();
    h.run_steps(2);
    assert_eq!(h.state().state.stock_ui.axis, StockAxis::Custom);
    for label in ["Turn about Z (yaw)", "Tilt about Y (pitch)", "Tilt about X (roll)"] {
        assert!(h.query_by_label(label).is_some(), "Custom shows {label:?}");
    }
}

/// Apply sends the axis choice as the stock's turn; undo restores the
/// previous turn, and the page reads each one back.
#[test]
fn apply_lays_the_stock_along_x_and_undo_stands_it_up_again() {
    let mut h = stock_page_with_cylinder();
    assert!(same_turn(stock_rot(&h), [0.0, 0.0, 0.0, 1.0]));
    h.get_by_label("Along X").click();
    h.run_steps(2);
    h.get_by_label("Apply").click();
    h.run_steps(3);
    let Some(x) = stock_axis_rot(StockAxis::X, [0.0; 3]) else { panic!("the X preset has a turn") };
    assert!(same_turn(stock_rot(&h), x), "Apply stored the X turn: {:?}", stock_rot(&h));
    assert_eq!(h.state().state.stock_ui.axis, StockAxis::X, "the page reads the X turn back");
    h.state_mut().state.pending.push_back(CommandKind::Undo);
    h.run_steps(3);
    assert!(same_turn(stock_rot(&h), [0.0, 0.0, 0.0, 1.0]), "undo restores the upright turn");
    assert_eq!(h.state().state.stock_ui.axis, StockAxis::Z, "the page reads the upright turn back");
}

/// A custom turn stored in the document reads back as Custom with its
/// angles.
#[test]
fn a_custom_turn_reads_back_with_its_angles() {
    let mut h = stock_page_with_cylinder();
    let Some(rot) = stock_axis_rot(StockAxis::Custom, [30.0, 10.0, 0.0]) else { panic!("finite angles") };
    let box_ = StockShape::Box { dx: 50.0, dy: 40.0, dz: 20.0 };
    h.state_mut().state.pending.push_back(CommandKind::SetStockPose { shape: box_, pos: [0.0; 3], rot });
    h.run_steps(3);
    let s = &h.state().state.stock_ui;
    assert_eq!(s.axis, StockAxis::Custom);
    for (got, want) in s.angles_deg.iter().zip([30.0, 10.0, 0.0]) {
        assert!((got - want).abs() < 1e-6, "angles {:?}", s.angles_deg);
    }
    assert!(h.query_by_label(SIM_NOTE_TILTED).is_some(), "a tilted stock gets the plain note");
}

/// A cylinder or a box laid along Y gets no note, since the simulation
/// cuts both (#2758); a tilted draft gets the plain note.
#[test]
fn a_laid_stock_shows_no_simulation_note_and_a_tilted_one_does() {
    let mut h = stock_page_with_cylinder();
    assert!(h.query_by_label(SIM_NOTE_TILTED).is_none(), "an upright cylinder has no note");
    h.get_by_label("Along Y").click();
    h.run_steps(2);
    assert!(h.query_by_label(SIM_NOTE_TILTED).is_none(), "a cylinder along Y is cut");
    h.state_mut().state.stock_ui.shape = 0;
    h.run_steps(2);
    assert!(h.query_by_label(SIM_NOTE_TILTED).is_none(), "a box along Y is cut");
    h.state_mut().state.stock_ui.axis = StockAxis::Custom;
    h.state_mut().state.stock_ui.angles_deg = [0.0, 20.0, 0.0];
    h.run_steps(2);
    assert!(h.query_by_label(SIM_NOTE_TILTED).is_some(), "a tilted draft has the note");
}
