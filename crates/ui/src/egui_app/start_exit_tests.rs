//! The contour's Start point / Exit point groups (#1448), driven through
//! the real page code: both groups sit on the Lead in / Lead out tab off
//! by default, the header switch seeds or clears the draft, a typed X
//! edits the draft only, and Apply commits through the command history -
//! undo and redo included. Other kinds say by name that they take no
//! pick (#3324).
//! Provenance: our own page law; no vendor material consulted.

use egui::accesskit::{Role, Toggled};
use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT, Queryable};

use camcraft_app::geo2d::Point2;
use camcraft_app::model;
use camcraft_app::operation::{KindInfo, OpParams};
use camcraft_app::path_picks::{PathPick, PickRole};

use super::super::dialog_tests::{face, opened, page};
use super::super::op_tabs::{SEC_GEO, SEC_LEADS};
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
    let picks = p.path_picks();
    (
        picks.first(PickRole::Start).map(|k| k.xy),
        picks.first(PickRole::Exit).map(|k| k.xy),
    )
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
    if let Some(picks) = params.path_picks_mut() {
        picks.exit = vec![PathPick::xy(Point2 { x: -7.0, y: 3.0 })];
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
fn the_slot_lead_tab_has_both_point_groups_and_takes_a_start() {
    // #3325: the Slot picks an end. Its Lead tab has the two groups, off,
    // with no "Not taken" line; the switch seeds the draft, Apply commits.
    let mut state = opened(crate::operations::retype(&face(), kind("Slot")));
    state.op_tab = SEC_LEADS;
    let mut harness = page(state);
    harness.run();
    for (g, rows) in [(START_GROUP, ["Start X", "Start Y"]), (EXIT_GROUP, ["Exit X", "Exit Y"])] {
        assert_eq!(harness.query_all_by_label(g).count(), 1, "the {g} group is on the tab one time");
        let sw = on_row(&harness, g, Role::CheckBox);
        assert_eq!(sw.accesskit_node().toggled(), Some(Toggled::False), "{g} is off on a new slot");
        for r in rows {
            assert!(harness.query_by_label(r).is_none(), "{r} hides while {g} is off");
        }
    }
    assert_eq!(harness.query_all_by_label_contains(super::NOT_TAKEN).count(), 0, "the Slot takes both picks");
    assert_eq!(draft(&harness), (None, None), "drawing the tab writes no point");

    on_row(&harness, START_GROUP, Role::CheckBox).click();
    harness.run();
    assert!(same(draft(&harness).0, 0.0, 0.0), "switching on seeds the origin: {:?}", draft(&harness));
    assert!(harness.query_by_label("Start X").is_some() && harness.query_by_label("Start Y").is_some());
    assert!(harness.query_by_label("Pick start").is_some(), "the viewport pick is offered");
    assert!(
        harness.query_by_label_contains("refused by name - the start never moves").is_some(),
        "the safety consequence is said under the rows"
    );
    harness.get_by_label("Apply").click();
    harness.run();
    harness.state_mut().drain_actions();
    assert!(same(live(harness.state()).0, 0.0, 0.0), "Apply committed it: {:?}", live(harness.state()));
    let st = harness.state_mut();
    assert!(st.history.undo(&st.world));
    assert_eq!(live(harness.state()), (None, None), "Ctrl+Z puts the automatic start back");
}

#[test]
fn kinds_without_a_picked_start_say_so_by_name() {
    // #3324: the maintainer's 2D Offset, a lead-tab kind (2D Pocket) and
    // a Geometry-only kind (Engrave; the Face takes its picks since
    // #3326): each draws both groups, no switch and no X / Y row, and a
    // line that says why the pick is not taken.
    for label in ["2D Offset", "2D Pocket", "Engrave"] {
        let mut state = opened(crate::operations::retype(&face(), kind(label)));
        state.op_tab = SEC_LEADS;
        let mut harness = page(state);
        harness.run();
        for g in [START_GROUP, EXIT_GROUP] {
            assert!(harness.query_by_label(g).is_some(), "{label} has the {g} group");
        }
        for r in ["Start X", "Start Y", "Exit X", "Exit Y", "Pick start", "Pick exit"] {
            assert!(harness.query_by_label(r).is_none(), "{label} has no {r} row");
        }
        let said = harness.query_all_by_label_contains(super::NOT_TAKEN).count();
        assert_eq!(said, 2, "{label}: the start and the exit say they are not taken");
        let why = harness.query_all_by_label_contains(camcraft_app::operation::PICK_OWED).count();
        assert_eq!(why, 2, "{label}: each says why, by name");
        assert_eq!(draft(&harness), (None, None), "drawing the tab writes no point");
    }
}

/// #3326: the Face states its start and exit on Geometry, typed in model
/// X and Y; the switch seeds a point, Clear puts the automatic raster back,
/// and Apply commits through the command history.
#[test]
fn the_face_geometry_tab_states_and_clears_its_start_and_exit() {
    let mut state = opened(face());
    state.op_tab = SEC_GEO;
    let mut harness = page(state);
    harness.run();
    for label in [START_GROUP, EXIT_GROUP, "Start point stated", "Exit point stated"] {
        assert!(harness.query_by_label(label).is_some(), "Face has {label} on Geometry");
    }
    assert_eq!(harness.query_all_by_label_contains(super::NOT_TAKEN).count(), 0, "the Face takes both picks");
    for r in ["Start X", "Start Y", "Exit X", "Exit Y", "Clear start", "Clear exit"] {
        assert!(harness.query_by_label(r).is_none(), "{r} waits for its switch");
    }
    assert_eq!(draft(&harness), (None, None), "drawing the tab writes no point");
    harness.get_by_label("Start point stated").click();
    harness.run();
    harness.get_by_label("Exit point stated").click();
    harness.run();
    assert!(same(draft(&harness).0, 0.0, 0.0) && same(draft(&harness).1, 0.0, 0.0), "each switch seeds its point");
    for r in ["Start X", "Start Y", "Exit X", "Exit Y", "Clear start", "Clear exit"] {
        assert!(harness.query_by_label(r).is_some(), "{r} is on the page");
    }
    harness.get_by_label("Apply").click();
    harness.run();
    harness.state_mut().drain_actions();
    assert!(same(live(harness.state()).0, 0.0, 0.0) && same(live(harness.state()).1, 0.0, 0.0), "Apply stores both picks");
    harness.get_by_label("Clear exit").click();
    harness.run();
    assert_eq!(draft(&harness).1, None, "Clear exit restores the automatic exit");
    assert!(same(draft(&harness).0, 0.0, 0.0), "the start stays");
    let st = harness.state_mut();
    assert!(st.history.undo(&st.world));
    assert_eq!(live(harness.state()), (None, None), "undo restores the automatic raster");
}

/// #3328: Drill, Bore and Thread take the first hole and the last hole.
/// Each page draws both groups off, with no "Not taken" line. Switching
/// a group on seeds a hole of the list in the draft; Apply commits the
/// draft as every parameter edit (pinned for the contour above).
#[test]
fn the_hole_kinds_type_their_first_and_last_hole() {
    use camcraft_app::drilling::Hole;
    let at = |x: f64| Hole { xy: Point2 { x, y: 5.0 }, radius: 2.0, entry_z: 0.0, floor_z: -5.0, through: false, stepped: false };
    for label in ["Drill", "Bore", "Thread"] {
        let mut params = crate::operations::retype(&face(), kind(label));
        if let OpParams::Drill { holes, .. } | OpParams::Bore { holes, .. } | OpParams::Thread { holes, .. } = &mut params {
            *holes = vec![at(-10.0), at(0.0), at(10.0)];
        }
        let ty = params.kind();
        // The fixture holds an end mill only, so the kind rides the draft.
        let mut state = opened(face());
        state.op_ui.params = params;
        state.op_tab = if super::super::op_tabs::has_section(ty, SEC_LEADS) { SEC_LEADS } else { super::super::op_tabs::SEC_GEO };
        let mut harness = page(state);
        harness.run();
        for g in [START_GROUP, EXIT_GROUP] {
            assert!(harness.query_by_label(g).is_some(), "{label} has the {g} group");
        }
        assert_eq!(harness.query_all_by_label_contains(super::NOT_TAKEN).count(), 0, "{label} takes both picks");
        for r in ["First hole X", "First hole Y", "Last hole X", "Last hole Y"] {
            assert!(harness.query_by_label(r).is_none(), "{label}: {r} hides while its group is off");
        }
        assert_eq!(draft(&harness), (None, None), "{label}: drawing the tab writes no point");

        on_row(&harness, "First hole stated", Role::CheckBox).click();
        harness.run();
        on_row(&harness, "Last hole stated", Role::CheckBox).click();
        harness.run();
        let (start, exit) = draft(&harness);
        assert!(same(start, -10.0, 5.0), "{label}: the start seeds the first hole of the list: {start:?}");
        assert!(same(exit, 10.0, 5.0), "{label}: the exit seeds the last hole of the list: {exit:?}");
        for r in ["First hole X", "First hole Y", "Last hole X", "Last hole Y"] {
            assert!(harness.query_by_label(r).is_some(), "{label}: {r} shows while its group is on");
        }
        assert!(harness.query_all_by_label_contains("is refused by name").count() >= 2, "{label}: each group says the law");
    }
}

#[test]
fn the_wire_geometry_tab_shows_the_start_hole_and_an_editable_exit() {
    let mut state = opened(face());
    let machine = camcraft_app::machine::parse_mch(include_str!("../../../app/tests/edm/wire-iso-fixture.mch"))
        .expect("wire machine fixture");
    state.pending.push_back(crate::state::CommandKind::EditMachineApply {
        path: "wire-test.mch".into(), machine: Box::new(machine),
    });
    state.pending.push_back(crate::state::CommandKind::AddTool {
        name: "EDM-W025".into(), tool: camcraft_app::wire_tools::wire_tool(0.25),
    });
    state.drain_actions();
    crate::actions::add_operation(&mut state, kind("Wire Contour"));
    state.drain_actions();
    state.sync_ops_ui();
    if let OpParams::WireContour(wire) = &mut state.op_ui.params {
        wire.picks.start = vec![PathPick::xy(Point2::new(2.0, 3.0))];
        wire.picks.exit = vec![PathPick::xy(Point2::new(7.0, 8.0))];
    }
    state.op_tab = super::super::op_tabs::SEC_GEO;
    let mut harness = page(state);
    harness.run();
    for label in [START_GROUP, EXIT_GROUP, "Start hole stated", "Start hole X", "Start hole Y", "Exit point stated", "Exit X", "Exit Y", "Clear exit"] {
        assert!(harness.query_by_label(label).is_some(), "Wire Contour has {label} on Geometry");
    }
    assert_eq!(harness.query_all_by_label_contains(super::NOT_TAKEN).count(), 0, "the wire takes both picks");
    assert!(same(draft(&harness).1, 7.0, 8.0), "the saved exit is preserved while drawn");
    harness.get_by_label("Apply").click();
    harness.run();
    harness.state_mut().drain_actions();
    let name = harness.state().selected_operation().expect("selected wire operation");
    let committed_exit = |state: &State| {
        let op = model::find_by_name(&state.world, &name)
            .and_then(|e| model::operation_of(&state.world, &e)).expect("wire operation");
        points(&op.params).1
    };
    assert!(same(committed_exit(harness.state()), 7.0, 8.0), "Apply stores the exit");
    harness.get_by_label(START_GROUP).click();
    harness.run();
    for label in ["Start hole stated", "Start hole X", "Start hole Y"] {
        assert!(harness.query_by_label(label).is_none(), "{label} folds with the Start point group");
    }
    assert!(harness.query_by_label(EXIT_GROUP).is_some(), "folding the start leaves the exit group visible");
    let OpParams::WireContour(wire) = &harness.state().op_ui.params else { panic!("wire draft") };
    assert_eq!(
        wire.picks.first(PickRole::Start).map(|p| [p.xy.x, p.xy.y]),
        Some([2.0, 3.0]),
        "folding the group preserves the threading point"
    );
    harness.get_by_label("Clear exit").click();
    harness.run();
    assert_eq!(draft(&harness).1, None, "Clear exit restores the automatic exit");
    assert!(harness.query_by_label("Exit X").is_none(), "the cleared exit rows leave the tree");
    assert!(same(committed_exit(harness.state()), 7.0, 8.0), "clearing waits for Apply");
    harness.get_by_label("Apply").click();
    harness.run();
    harness.state_mut().drain_actions();
    assert_eq!(committed_exit(harness.state()), None, "Apply clears the stored exit");
    let st = harness.state_mut();
    assert!(st.history.undo(&st.world));
    assert!(same(committed_exit(harness.state()), 7.0, 8.0), "undo restores the stated exit");
    let st = harness.state_mut();
    assert!(st.history.redo(&st.world));
    assert_eq!(committed_exit(harness.state()), None, "redo clears the stated exit again");
    harness.get_by_label("Exit point stated").click();
    harness.run();
    assert!(same(draft(&harness).1, 0.0, 0.0), "the switch seeds an exit for typing");
}

#[test]
fn the_contour_lead_tab_has_viewport_pick_buttons_for_start_and_exit() {
    let mut harness = contour_page();
    let pick_start = harness.get_by_label("Pick start");
    assert_eq!(pick_start.accesskit_node().toggled(), Some(Toggled::False));
    let pick_exit = harness.get_by_label("Pick exit");
    assert_eq!(pick_exit.accesskit_node().toggled(), Some(Toggled::False));

    harness.get_by_label("Pick start").click();
    harness.run();
    assert_eq!(draft(&harness), (None, None), "arming does not invent a coordinate");
    assert_eq!(harness.get_by_label("Stop picking").accesskit_node().toggled(), Some(Toggled::True));
    assert_eq!(harness.get_by_label("Pick exit").accesskit_node().toggled(), Some(Toggled::False));

    harness.get_by_label("Pick exit").click();
    harness.run();
    assert!(harness.query_by_label("Pick start").is_some(), "exit pick replaces start pick");
    assert_eq!(harness.get_by_label("Stop picking").accesskit_node().toggled(), Some(Toggled::True));

    harness.get_by_label("Stop picking").click();
    harness.run();
    assert!(harness.query_by_label("Stop picking").is_none());
    assert_eq!(harness.get_by_label("Pick start").accesskit_node().toggled(), Some(Toggled::False));
    assert_eq!(harness.get_by_label("Pick exit").accesskit_node().toggled(), Some(Toggled::False));
}

#[test]
fn clearing_a_picked_start_or_exit_resets_it_to_automatic() {
    let mut params = crate::operations::retype(&face(), kind("2D Contour"));
    if let Some(picks) = params.path_picks_mut() {
        picks.start = vec![PathPick::xy(Point2 { x: 12.5, y: 4.0 })];
        picks.exit = vec![PathPick::xy(Point2 { x: -7.0, y: 3.0 })];
    }
    let mut state = opened(params);
    state.op_tab = SEC_LEADS;
    let mut harness = page(state);
    harness.run();

    harness.get_by_label("Clear start").click();
    harness.run();
    assert_eq!(draft(&harness).0, None, "Clear start sets None");
    assert!(harness.query_by_label("Start X").is_none(), "Start X leaves when cleared");

    harness.get_by_label("Clear exit").click();
    harness.run();
    assert_eq!(draft(&harness).1, None, "Clear exit sets None");
    assert!(harness.query_by_label("Exit X").is_none(), "Exit X leaves when cleared");
}
