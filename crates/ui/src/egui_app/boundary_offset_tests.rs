//! #3338: the Boundary group's two offset rows - the inside boundary
//! (the picked edge or face) and the outside boundary (the stock side of
//! an open boundary). Each row has its own label, hint and draft field,
//! both start at 0, and Apply commits one through the undoable edit.
//!
//! Gate map: covers ui::egui_app (boundary), ui::boundary_summary.

use egui::accesskit::Role;
use egui_kittest::kittest::Queryable;

use camcraft_app::machining3d::Entry3D;
use camcraft_app::model;
use camcraft_app::operation::{AreaControls, Containment};

use super::boundary::{OFFSET_HINT, OUTER_OFFSET_HINT};
use super::dialog_tests::{opened, page, pocket};
use crate::boundary_summary;

fn live_area(state: &crate::state::State) -> AreaControls {
    model::find_by_name(&state.world, "Op").and_then(|e| model::operation_of(&state.world, &e)).map(|o| o.area).expect("the op")
}

#[test]
fn the_boundary_group_has_an_inside_and_an_outside_offset_row() {
    assert!(OFFSET_HINT.starts_with("Moves the picked edge or face loop"), "{OFFSET_HINT}");
    assert!(OUTER_OFFSET_HINT.contains("only the outer side of an open boundary"), "{OUTER_OFFSET_HINT}");
    assert!(OUTER_OFFSET_HINT.contains("The picked edge stays."), "{OUTER_OFFSET_HINT}");
    let mut state = opened(pocket(Entry3D::Plunge));
    // A set control opens the group.
    state.op_ui.area.containment = Some(Containment::On);
    assert_eq!((state.op_ui.area.offset, state.op_ui.area.outer_offset), (0.0, 0.0), "both start at 0");
    let mut harness = page(state);
    harness.run();
    assert!(harness.query_by_label("Boundary offset").is_none(), "the one shared row is gone");
    let row = |h: &egui_kittest::Harness<'static, crate::state::State>, label: &str| {
        let lab = h.get_by_label(label).rect();
        h.query_all_by(|n| n.role() == Role::SpinButton)
            .find(|n| (n.rect().center().y - lab.center().y).abs() < 12.0)
            .map(|n| n.rect())
            .expect("a number on the label's row")
    };
    let inner = row(&harness, "Inside boundary offset");
    let outer = row(&harness, "Outside boundary offset");
    assert!(outer.top() > inner.top(), "the outside row follows the inside row");
    harness.query_all_by(|n| n.role() == Role::SpinButton).find(|n| n.rect() == outer).expect("spin").click();
    harness.run();
    harness
        .query_all_by(|n| n.role() == Role::SpinButton)
        .find(|n| (n.rect().top() - outer.top()).abs() < 1.0)
        .expect("editing")
        .type_text("-10");
    harness.run();
    let a = &harness.state().op_ui.area;
    assert_eq!((a.offset, a.outer_offset), (0.0, -10.0), "only the outside offset took the value");
    assert_eq!(live_area(harness.state()).outer_offset, 0.0, "the world waits for Apply");
    harness.get_by_label("Apply").click();
    harness.run();
    harness.state_mut().drain_actions();
    assert_eq!(live_area(harness.state()).outer_offset, -10.0, "Apply committed it");
    let st = harness.state_mut();
    st.history.undo(&st.world);
    assert_eq!(live_area(harness.state()).outer_offset, 0.0, "and Ctrl+Z puts it back");
}

#[test]
fn the_summary_says_an_outside_offset_without_an_open_boundary_does_nothing() {
    let mut state = opened(pocket(Entry3D::Plunge));
    state.op_ui.area.outer_offset = -4.0;
    let s = boundary_summary::summarize(&state);
    assert!(
        s.notices.iter().any(|n| n == "The outside boundary offset has no effect here: only an open boundary has an outer side."),
        "{:?}",
        s.notices
    );
}
