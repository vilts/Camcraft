//! #739's WCS page tests: the layout law in structure (three sections,
//! label and field on one row with the fields in one column, the quarter
//! turns as equal buttons filling the pane, the anchor points as a centred
//! 3x3 compass of equal cells, every button live), and the turn tests that
//! moved here with `turn_wcs` from pages.rs (verbatim from the retired
//! imgui setup.rs at M-g: a turn moves the world, not just the drafts).
//! No pixels: structural assertions only (no snapshot PNG changes).
//! Provenance: our own page law; no vendor material consulted.

use egui::accesskit::Role;
use egui::{Rect, Vec2};
use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT, Queryable};

use camcraft_app::geometry::{Point3, quat_from_euler, transform_point};
use camcraft_app::model;

use super::wcs_page::turn_wcs;
use crate::state::{CommandKind, State};

fn frame(ui: &mut egui::Ui, st: &mut State) {
    st.drain_actions();
    st.sync_setup_ui();
    super::wcs_page::wcs_page(ui, st);
}

/// A state with a stock box, so the anchor compass has points.
fn with_stock() -> State {
    let mut state = State::new();
    state.pending.push_back(CommandKind::SetStock {
        shape: camcraft_app::stock::StockShape::Box { dx: 40.0, dy: 30.0, dz: 10.0 },
        pos: [0.0, 0.0, 0.0],
        yaw_deg: 0.0,
    });
    state.drain_actions();
    state
}

/// The page alone at a properties pane's width.
pub(super) fn page(state: State) -> Harness<'static, State> {
    let mut h = Harness::builder().with_size(Vec2::new(320.0, 900.0)).build_ui_state(frame, state);
    h.run();
    h
}

fn rect(h: &Harness<'static, State>, label: &str) -> Rect {
    h.get_by_label(label).rect()
}

fn live(h: &Harness<'static, State>, label: &str) -> bool {
    !h.get_by_label(label).accesskit_node().is_disabled()
}

fn same_size(a: Rect, b: Rect) -> bool {
    (a.width() - b.width()).abs() < 0.5 && (a.height() - b.height()).abs() < 0.5
}

#[test]
fn the_page_has_three_sections_and_one_row_fields_in_one_column() {
    let h = page(with_stock());
    assert!(h.query_by_label("Origin and orientation").is_some());
    assert!(h.query_by_label("Quarter turns").is_some());
    assert!(h.query_all_by_label("Anchor on").count() >= 2, "the section and its choice row");
    let sections = [rect(&h, "Origin and orientation"), rect(&h, "Quarter turns"), rect(&h, "Turn X+")];
    assert!(sections[0].max.y < sections[1].min.y && sections[1].max.y < sections[2].min.y, "{sections:?}");

    let spins: Vec<Rect> = h.query_all_by(|n| n.role() == Role::SpinButton).map(|n| n.rect()).collect();
    let mut column: Vec<f32> = Vec::new();
    for label in [
        "X origin",
        "Y origin",
        "Z origin",
        "Tilt about X (roll)",
        "Tilt about Y (pitch)",
        "Turn about Z (yaw)",
    ] {
        let l = rect(&h, label);
        let field = spins
            .iter()
            .find(|s| (s.center().y - l.center().y).abs() <= l.height() * 0.5 + 1.0 && s.min.x > l.max.x)
            .unwrap_or_else(|| panic!("the {label} field is not on its label's row ({l:?} vs {spins:?})"));
        column.push(field.min.x);
    }
    assert!(column.iter().all(|x| (x - column[0]).abs() < 0.5), "the fields share one column: {column:?}");
}

#[test]
fn quarter_turns_are_equal_buttons_filling_the_pane() {
    let h = page(with_stock());
    let turns = ["Turn X+", "Turn Y+", "Turn Z+", "Turn X-", "Turn Y-", "Turn Z-"];
    let r: Vec<Rect> = turns.iter().map(|l| rect(&h, l)).collect();
    for (i, l) in turns.iter().enumerate() {
        assert!(live(&h, l), "{l} is live");
        assert!(same_size(r[i], r[0]), "{l} is the size of Turn X+: {:?} vs {:?}", r[i], r[0]);
    }
    for k in 0..3 {
        assert!((r[k].min.x - r[k + 3].min.x).abs() < 0.5, "the rows line up in columns");
        assert!(r[k].max.y <= r[k + 3].min.y, "X+ over X-, and so on");
    }
    let (level, set) = (rect(&h, "Level"), rect(&h, "Set Origin"));
    assert!(live(&h, "Level") && live(&h, "Set Origin"));
    assert!(same_size(level, set), "{level:?} vs {set:?}");
    // One width for every row: the action row spans exactly the turns'.
    assert!((level.min.x - r[0].min.x).abs() < 1.0 && (set.max.x - r[2].max.x).abs() < 1.0, "{level:?} {set:?}");
    let span = r[2].max.x - r[0].min.x;
    assert!(span > 0.85 * 320.0, "the turns fill the pane's width ({span} px)");
}

#[test]
fn the_anchor_points_are_a_centred_compass_of_equal_cells() {
    let h = page(with_stock());
    let names = ["NW", "N", "NE", "W", "[C]", "E", "SW", "S", "SE"];
    let r: Vec<Rect> = names.iter().map(|l| rect(&h, l)).collect();
    for (i, l) in names.iter().enumerate() {
        assert!(live(&h, l), "{l} is live");
        assert!(same_size(r[i], r[0]), "{l}: {:?} vs {:?}", r[i], r[0]);
        let (row, col) = (i / 3, i % 3);
        assert!((r[i].min.x - r[col].min.x).abs() < 0.5, "{l} in column {col}");
        assert!((r[i].min.y - r[row * 3].min.y).abs() < 0.5, "{l} in row {row}");
    }
    assert!(r[0].max.x < r[1].min.x && r[0].max.y < r[3].min.y, "three distinct columns and rows");
    assert!(r[0].height() >= 28.0, "cells a bit larger than a plain button: {:?}", r[0]);
    // Centred on the turns' span.
    let turns = rect(&h, "Turn X+").min.x..=rect(&h, "Turn Z+").max.x;
    let mid = 0.5 * (turns.start() + turns.end());
    assert!((r[4].center().x - mid).abs() < 2.0, "the compass is centred: {} vs {mid} ({:?} {:?} / {:?})", r[4].center().x, r[3], r[5], turns);
    assert!(r[8].max.x <= 320.0, "the compass stays inside the pane: {:?}", r[8]);
    assert!(live(&h, "Pick markers in viewport"));
}

fn place(state: &mut State, pos: [f64; 3], yaw_deg: f64) {
    state.pending.push_back(CommandKind::SetWcs {
        pos,
        roll_deg: 0.0,
        pitch_deg: 0.0,
        yaw_deg,
    });
    state.drain_actions();
}

fn location(state: &State, slot: &str) -> camcraft_app::schema::Location {
    let entity = model::find_by_name(&state.world, slot).expect("offset entity");
    model::transform_of(&state.world, &entity).expect("location")
}

#[test]
fn a_turn_moves_the_world_not_just_the_drafts() {
    let mut state = State::new();
    place(&mut state, [10.0, 0.0, 0.0], 0.0);
    state.sync_setup_ui();
    turn_wcs(&mut state, 0, 1.0); // X+ quarter about the world X
    state.drain_actions();
    let loc = location(&state, "G54");
    assert_eq!([loc.pos.x, loc.pos.y, loc.pos.z], [10.0, 0.0, 0.0]);
    // Right-hand +90 about X takes +Z to -Y.
    let p = transform_point(Point3::new(0.0, 0.0, 1.0), loc.pos, loc.rot);
    assert!(
        [(p.x - 10.0), (p.y + 1.0), p.z]
            .iter()
            .all(|v| v.abs() < 1e-9),
        "{p:?}"
    );
}

#[test]
fn four_turns_about_one_axis_return_to_the_start() {
    let mut state = State::new();
    place(&mut state, [0.0, 0.0, 0.0], 30.0);
    state.sync_setup_ui();
    for _ in 0..4 {
        turn_wcs(&mut state, 2, 1.0);
    }
    state.drain_actions();
    let loc = location(&state, "G54");
    // Same rotation as the 30-degree start: quaternion equal up to sign.
    let start = quat_from_euler(0.0, 0.0, 30.0f64.to_radians());
    let dot: f64 = start.iter().zip(loc.rot).map(|(a, b)| a * b).sum();
    assert!(
        (dot.abs() - 1.0).abs() < 1e-9,
        "closed the circle: {:?}",
        loc.rot
    );
}

#[test]
fn turning_an_unplaced_wcs_applies_the_typed_position() {
    let mut state = State::new();
    state.wcs_ui.x = 5.0;
    turn_wcs(&mut state, 1, -1.0); // Y- quarter
    state.drain_actions();
    let loc = location(&state, "G54");
    assert_eq!([loc.pos.x, loc.pos.y, loc.pos.z], [5.0, 0.0, 0.0]);
    // Right-hand -90 about Y takes +X to +Z: the drafts, not just a
    // rotation, rode along.
    let p = transform_point(Point3::new(1.0, 0.0, 0.0), loc.pos, loc.rot);
    assert!(
        [(p.x - 5.0), p.y, (p.z - 1.0)]
            .iter()
            .all(|v| v.abs() < 1e-9),
        "{p:?}"
    );
}

