//! The contour's Start point / Exit point groups (#1448), driven through
//! the real page code: both groups sit on the Lead in / Lead out tab off
//! by default, the header switch seeds or clears the draft, a typed X
//! edits the draft only, and Apply commits through the command history -
//! undo and redo included. Other kinds draw neither group.
//! Provenance: our own page law; no vendor material consulted.

use egui::accesskit::{Role, Toggled};
use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT, Queryable};

use camcraft_app::geo2d::Point2;
use camcraft_app::model;
use camcraft_app::operation::{KindInfo, OpParams};

use super::super::dialog_tests::{face, opened, page};
use super::super::op_tabs::SEC_LEADS;
use super::{EXIT_GROUP, START_GROUP};
use crate::state::State;

fn kind(label: &str) -> usize {
    KindInfo::ALL.iter().position(|k| k.label == label).expect("the kind")
}

/// A committed 2D Contour named "Op", opened on its Lead tab.
fn contour_page() -> Harness<'static, State> {
    let mut state = opened(crate::operations::retype(&face(), kind("2D Contour")));
    state.op_tab = SEC_LEADS;
    let mut harness = page(state);
    harness.run();
    harness
}

fn points(p: &OpParams) -> (Option<Point2>, Option<Point2>) {
    match p {
        OpParams::Contour { links, .. } => (links.approach.start_point, links.approach.exit_point),
        _ => (None, None),
    }
}

fn live(state: &State) -> (Option<Point2>, Option<Point2>) {
    model::find_by_name(&state.world, "Op")
        .and_then(|e| model::operation_of(&state.world, &e))
        .map(|o| points(&o.params))
        .expect("the op")
}

fn draft(h: &Harness<'static, State>) -> (Option<Point2>, Option<Point2>) {
    points(&h.state().op_ui.params)
}

/// The node of `role` on the same row as the label `label`.
fn on_row<'a>(h: &'a Harness<'static, State>, label: &str, role: Role) -> egui_kittest::Node<'a> {
    let lab = h.get_by_label(label).rect();
    h.query_all_by(move |n| n.role() == role)
        .find(|n| {
            let c = n.rect().center().y;
            c >= lab.top() - 2.0 && c <= lab.bottom() + 2.0
        })
        .expect("a control on the label's row")
}

fn same(a: Option<Point2>, x: f64, y: f64) -> bool {
    a.is_some_and(|q| (q.x - x).abs() < 1e-9 && (q.y - y).abs() < 1e-9)
}

#[test]
fn the_contour_lead_tab_has_both_point_groups_off_by_default() {
    let harness = contour_page();
    for (g, rows) in [(START_GROUP, ["Start X", "Start Y"]), (EXIT_GROUP, ["Exit X", "Exit Y"])] {
        assert!(harness.query_by_label(g).is_some(), "the {g} group is on the tab");
        let sw = on_row(&harness, g, Role::CheckBox);
        assert_eq!(sw.accesskit_node().toggled(), Some(Toggled::False), "{g} is off on a new contour");
        assert!(!sw.accesskit_node().is_disabled(), "{g} can be switched on");
        for r in rows {
            assert!(harness.query_by_label(r).is_none(), "{r} hides while {g} is off");
        }
    }
    assert_eq!(draft(&harness), (None, None), "drawing the tab writes no point");
}

#[test]
fn a_typed_start_point_applies_through_the_history_and_undoes() {
    let mut harness = contour_page();
    on_row(&harness, START_GROUP, Role::CheckBox).click();
    harness.run();
    assert!(same(draft(&harness).0, 0.0, 0.0), "switching on seeds the origin: {:?}", draft(&harness));
    assert!(harness.query_by_label("Start X").is_some() && harness.query_by_label("Start Y").is_some());
    assert!(
        harness.query_by_label_contains("refused by name - the start never moves").is_some(),
        "the safety consequence is said under the rows"
    );

    let spin = on_row(&harness, "Start X", Role::SpinButton).rect();
    harness.query_all_by(|n| n.role() == Role::SpinButton).find(|n| n.rect() == spin).expect("spin").click();
    harness.run();
    harness
        .query_all_by(|n| n.role() == Role::SpinButton)
        .find(|n| (n.rect().top() - spin.top()).abs() < 1.0)
        .expect("editing")
        .type_text("12.5");
    harness.run();
    assert!(same(draft(&harness).0, 12.5, 0.0), "the draft took the typed X: {:?}", draft(&harness));
    assert_eq!(live(harness.state()), (None, None), "the world waits for Apply");

    harness.get_by_label("Apply").click();
    harness.run();
    harness.state_mut().drain_actions();
    assert!(same(live(harness.state()).0, 12.5, 0.0), "Apply committed it: {:?}", live(harness.state()));
    assert_eq!(live(harness.state()).1, None, "the exit point stays unset");

    let st = harness.state_mut();
    assert!(st.history.undo(&st.world));
    assert_eq!(live(harness.state()), (None, None), "Ctrl+Z puts the automatic start back");
    let st = harness.state_mut();
    assert!(st.history.redo(&st.world));
    assert!(same(live(harness.state()).0, 12.5, 0.0), "Ctrl+Y restores the pick");
}

#[test]
fn switching_the_exit_point_off_clears_it() {
    let mut params = crate::operations::retype(&face(), kind("2D Contour"));
    if let OpParams::Contour { links, .. } = &mut params {
        links.approach.exit_point = Some(Point2 { x: -7.0, y: 3.0 });
    }
    let mut state = opened(params);
    state.op_tab = SEC_LEADS;
    let mut harness = page(state);
    harness.run();
    let sw = on_row(&harness, EXIT_GROUP, Role::CheckBox);
    assert_eq!(sw.accesskit_node().toggled(), Some(Toggled::True), "a saved exit shows its group on");
    assert!(harness.query_by_label("Exit X").is_some() && harness.query_by_label("Exit Y").is_some());
    assert!(harness.query_by_label("Start X").is_none(), "the start stays off");
    sw.click();
    harness.run();
    assert_eq!(draft(&harness), (None, None), "off writes None - the automatic exit");
    assert!(harness.query_by_label("Exit X").is_none(), "and the rows leave the tree");
}

#[test]
fn kinds_without_a_picked_start_draw_neither_group() {
    for label in ["2D Pocket", "Face"] {
        let mut state = opened(crate::operations::retype(&face(), kind(label)));
        state.op_tab = SEC_LEADS;
        let mut harness = page(state);
        harness.run();
        for g in [START_GROUP, EXIT_GROUP] {
            assert!(harness.query_by_label(g).is_none(), "{label} has no {g} group");
        }
    }
}
