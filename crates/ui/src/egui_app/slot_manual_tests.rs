//! #1732: the Slot page's "Manual" section and the A/B marks in the
//! viewport. Structural (AccessKit labels and placement) plus the mark
//! geometry read straight off the page draft; never pixels.
//! Provenance: our own page law; no vendor material consulted.

use camcraft_app::geo2d::Point2;
use camcraft_app::model;
use camcraft_app::operation::OpParams;
use egui_kittest::kittest::Queryable;

use super::dialog_tests::page;
use super::slot_pick_page_tests::opened;
use crate::slot_manual_viz::{SlotMarkKind, draft_points, slot_marks};
use crate::state::State;

fn draft_a(state: &mut State, x: f64, y: f64) {
    if let OpParams::Slot { a, .. } = &mut state.op_ui.params {
        *a = Point2::new(x, y);
    }
}

fn committed_a(state: &State) -> Point2 {
    match model::find_by_name(&state.world, "Op")
        .and_then(|e| model::operation_of(&state.world, &e))
        .map(|o| o.params)
    {
        Some(OpParams::Slot { a, .. }) => a,
        other => panic!("a slot op expected, got {other:?}"),
    }
}

#[test]
fn the_hand_typed_ends_sit_in_a_manual_section() {
    let mut harness = page(opened());
    harness.run();
    let top = |l: &str| harness.get_by_label(l).rect().min.y;
    let manual = top("Manual");
    for f in ["A X", "A Y", "B X", "B Y"] {
        assert!(top(f) > manual, "{f} stands under the Manual header");
    }
    assert!(top("Selection") < manual, "Manual is its own section after Selection");
    assert!(top("Slot width") > top("B Y"), "the width stays outside the point rows");
}

#[test]
fn the_marks_follow_a_draft_edit_before_it_is_committed() {
    let mut state = opened();
    let before = slot_marks(&state, 1.5);
    assert!(
        before.iter().any(|m| m.kind == SlotMarkKind::Line),
        "the A-B line draws"
    );
    assert!(before.iter().any(|m| m.kind == SlotMarkKind::PointA));
    assert!(before.iter().any(|m| m.kind == SlotMarkKind::PointB));
    let (pa, _) = draft_points(&state).expect("points");
    assert_eq!((pa[0], pa[1]), (-20.0, 0.0));
    draft_a(&mut state, -10.0, 5.0);
    let (pa, _) = draft_points(&state).expect("points");
    assert_eq!((pa[0], pa[1]), (-10.0, 5.0), "the preview reads the draft");
    assert_ne!(slot_marks(&state, 1.5), before, "the screen marks moved");
    assert_eq!(
        (committed_a(&state).x, committed_a(&state).y),
        (-20.0, 0.0),
        "nothing committed yet"
    );
}

fn assert_tilted_draft_marks(origin: [f64; 3], expected: ([f64; 3], [f64; 3])) {
    let mut state = opened();
    if let OpParams::Slot { tilt, b, .. } = &mut state.op_ui.params {
        *tilt = Some(camcraft_app::operation::Tilt {
            basis_version: 1,
            a_deg: 90.0,
            b_deg: 0.0,
            origin,
        });
        b.y = 6.0;
    }
    let actual = draft_points(&state).expect("the tilted draft resolves");
    for (actual, expected) in [(actual.0, expected.0), (actual.1, expected.1)] {
        for axis in 0..3 {
            assert!(
                (actual[axis] - expected[axis]).abs() < 1e-9,
                "{actual:?} != {expected:?}"
            );
        }
    }
    let line = slot_marks(&state, 1.5)
        .into_iter()
        .find(|m| m.kind == SlotMarkKind::Line)
        .expect("the tilted draft has a projected A-B line");
    let mvp = state.cam.mvp(1.5);
    assert_eq!(
        line.a,
        crate::interact::project_uv(&mvp, expected.0).expect("A projects")
    );
    assert_eq!(
        line.b,
        crate::interact::project_uv(&mvp, expected.1).expect("B projects")
    );
    let committed = model::find_by_name(&state.world, "Op")
        .and_then(|e| model::operation_of(&state.world, &e))
        .expect("the committed operation");
    assert!(
        committed.params.tilt().is_none(),
        "the tilt is still an unapplied draft"
    );
}

#[test]
fn tilted_draft_marks_use_the_cutting_plane() {
    // At A=90, local X stays X, local Y becomes world Z, and Top Z=10
    // becomes world Y=-10. These values are independent of Tilt::frame.
    assert_tilted_draft_marks([0.0; 3], ([-20.0, -10.0, 0.0], [20.0, -10.0, 6.0]));
}

#[test]
fn tilted_draft_marks_shift_resolved_height_before_the_world_transform() {
    // The fixed Top Z=10 shifts to local Z=5, then rotates and translates.
    assert_tilted_draft_marks([3.0, 4.0, 5.0], ([-17.0, -1.0, 5.0], [23.0, -1.0, 11.0]));
}

#[test]
fn no_marks_with_a_selection_or_off_the_geometry_tab() {
    let mut state = opened();
    state.op_tab = super::op_tabs::SEC_HEIGHTS;
    assert!(slot_marks(&state, 1.5).is_empty(), "only the Geometry tab shows them");
    state.op_tab = super::op_tabs::SEC_GEO;
    assert!(!slot_marks(&state, 1.5).is_empty());
    state.op_ui.geo.push(camcraft_app::operation::GeoSel {
        entity: "part".into(),
        feature: camcraft_app::operation::FeatureSel::Chain(0),
    });
    assert!(slot_marks(&state, 1.5).is_empty(), "a picked selection owns A and B");
}

#[test]
fn no_marks_after_switching_to_a_tool_or_setup_page() {
    use crate::state::{SetupPage, UiTarget};

    let mut state = opened();
    for target in [
        UiTarget::Tool(String::new()),
        UiTarget::Setups,
        UiTarget::SetupNode(String::new(), SetupPage::Settings),
    ] {
        state.ui_target = UiTarget::Operation("Op".into());
        state.sync_ops_ui();
        assert!(!slot_marks(&state, 1.5).is_empty());
        state.ui_target = target;
        state.sync_ops_ui();
        assert_eq!(state.op_ui.name, "Op", "the old operation draft remains cached");
        assert!(
            draft_points(&state).is_none(),
            "another Properties page cannot show the cached Slot draft"
        );
        assert!(slot_marks(&state, 1.5).is_empty());
    }
    state.ui_target = UiTarget::Operation("Op".into());
    state.sync_ops_ui();
    assert!(
        !slot_marks(&state, 1.5).is_empty(),
        "returning to the Slot restores its marks"
    );
}

#[test]
fn no_marks_when_properties_is_closed_or_hidden() {
    let mut state = opened();
    assert!(!slot_marks(&state, 1.5).is_empty());
    state.panels_open.properties = false;
    assert!(slot_marks(&state, 1.5).is_empty(), "closing Properties hides the marks");
    state.panels_open.properties = true;
    assert!(
        !slot_marks(&state, 1.5).is_empty(),
        "reopening Properties restores the marks"
    );
    state.properties_shown = false;
    assert!(
        slot_marks(&state, 1.5).is_empty(),
        "a Properties tab hidden behind another tab has no marks"
    );
    state.properties_shown = true;
    assert!(
        !slot_marks(&state, 1.5).is_empty(),
        "showing Properties restores the marks"
    );
}

#[test]
fn shell_updates_slot_marks_when_properties_is_hidden_and_closed() {
    use super::shell::Tab;
    use crate::state::{LayoutRequest, Panel};

    let mut harness = egui_kittest::Harness::builder()
        .with_size(egui::vec2(1280.0, 800.0))
        .build_ui_state(
            |ui, (state, dock): &mut (State, egui_dock::DockState<Tab>)| {
                super::shell::shell_layout(
                    ui,
                    state,
                    dock,
                    egui::TextureId::default(),
                    None,
                    &mut [1280.0, 649.0],
                    &mut None,
                );
            },
            (opened(), super::layout::build_dock()),
        );
    harness.run();
    assert!(!slot_marks(&harness.state().0, 1.5).is_empty());
    {
        let (state, dock) = harness.state_mut();
        let props = dock.find_tab(&Tab::Properties).expect("Properties tab");
        dock.leaf_mut(props.node_path())
            .expect("Properties leaf")
            .append_tab(Tab::Machines);
        state.properties_shown = true;
    }
    harness.run();
    assert!(
        !harness.state().0.properties_shown,
        "the shell sees Properties behind Machines"
    );
    assert!(slot_marks(&harness.state().0, 1.5).is_empty());
    {
        let (_, dock) = harness.state_mut();
        let props = dock.find_tab(&Tab::Properties).expect("Properties tab");
        dock.leaf_mut(props.node_path()).expect("Properties leaf").active = props.tab;
    }
    harness.run();
    assert!(!slot_marks(&harness.state().0, 1.5).is_empty());
    harness.state_mut().0.layout_request = Some(LayoutRequest::Toggle(Panel::Properties));
    harness.run();
    assert!(!harness.state().0.panels_open.properties);
    assert!(!harness.state().0.properties_shown);
    assert!(slot_marks(&harness.state().0, 1.5).is_empty());
}
