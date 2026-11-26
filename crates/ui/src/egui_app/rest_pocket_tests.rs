//! #1815: the "Rest machining" group on the 2D Pocket page (`Passes`
//! tab) - default off, header switch shows/hides the 4 parameters,
//! draft edits commit through `EditOperationParams` on Apply (undo and
//! redo included), and `#1489` refusals come back on Apply by name
//! (Adaptive, pre-drilled; Zigzag is legal since #2037).
//! Provenance: our own page law over `camcraft_app::machining2d::RestParams`.

use egui::Vec2;
use egui::accesskit::{Role, Toggled};
use egui_kittest::kittest::{NodeT, Queryable};
use egui_kittest::Harness;

use camcraft_app::commands::{AddTool, CreateOperation, ImportModel, SetUnits};
use camcraft_app::geometry::{Point3, cube};
use camcraft_app::machining2d::{PocketStrategy, RestParams};
use camcraft_app::machining3d::Entry3D;
use camcraft_app::model;
use camcraft_app::operation::{Height, KindInfo, OpParams, OpType, REST_PREDRILLED, SrcRef};
use camcraft_app::stock::StockShape;
use camcraft_app::tools::catalog;
use camcraft_app::units::Unit;

use super::op_tabs::SECTIONS;
use crate::state::{CommandKind, ErrorScope, State, UiTarget};

const REST_FIELDS: [&str; 4] = [
    "Prev. tool Ø",
    "Prev. corner rad.",
    "Prev. stock left",
    "Rest allowance",
];

fn kind_index(ty: OpType) -> usize {
    KindInfo::ALL.iter().position(|k| k.ty == ty).expect("registered op kind")
}

fn sec_index(name: &str) -> usize {
    SECTIONS.iter().position(|s| *s == name).expect("registered section")
}

fn pocket(strategy: PocketStrategy, rest: Option<RestParams>) -> OpParams {
    OpParams::Pocket { pass_start: Default::default(),
        links: camcraft_app::links::Links::PLANAR,
        stepover: 1.5,
        stock_to_leave: 0.0,
        clearance_z: Height::fixed(25.0),
        retract_z: None,
        top_z: Height::fixed(10.0),
        bottom_z: Height::fixed(5.0),
        stepdown: 2.0,
        feed: 800.0,
        plunge_feed: 200.0,
        climb: true,
        strategy,
        entry: Entry3D::Helix,
        helix_radius: 1.0,
        optimal_load: camcraft_app::load::DEFAULT_OPTIMAL_LOAD,
        blend_turns: false,
        blend_radius: None,
        rest,
        tilt: None,
        corner_relief: Default::default(),
        picks: Default::default(),
    }
}

/// Stock, a 40x40x10 part, an EM-0400-2FL (4 mm) tool and a 2D Pocket
/// named "P1" opened on the `Passes` tab.
fn opened_pocket(strategy: PocketStrategy, rest: Option<RestParams>) -> State {
    let mut state = State::new();
    state.pending.push_back(CommandKind::SetStock {
        shape: StockShape::Box { dx: 60.0, dy: 60.0, dz: 20.0 },
        pos: [0.0, 0.0, 0.0],
        yaw_deg: 0.0,
    });
    state.drain_actions();
    let em = catalog().into_iter().find(|(n, _)| *n == "EM-0400-2FL").map(|(_, t)| t).expect("catalog entry");
    state.history.execute(&state.world, AddTool::new(&state.world, "EM-0400-2FL", em).expect("tool"));
    state.history.execute(
        &state.world,
        ImportModel::from_mesh("part", cube(Point3::new(-20.0, -20.0, 0.0), Point3::new(20.0, 20.0, 10.0))),
    );
    state.history.execute(
        &state.world,
        CreateOperation::new(&state.world, "P1", "EM-0400-2FL", SrcRef::Stock, pocket(strategy, rest))
            .expect("operation"),
    );
    state.pending.push_back(CommandKind::Select(Some("P1".into())));
    state.drain_actions();
    state.ui_target = UiTarget::Operation("P1".into());
    state.op_tab = sec_index("Passes");
    state.sync_ops_ui();
    state
}

fn page(state: State) -> Harness<'static, State> {
    Harness::builder()
        .with_size(Vec2::new(620.0, 1400.0))
        .build_ui_state(|ui, st: &mut State| super::pages::properties(ui, st, &super::theme::current()), state)
}

fn live_rest(state: &State) -> Option<RestParams> {
    match model::find_by_name(&state.world, "P1").and_then(|e| model::operation_of(&state.world, &e))?.params {
        OpParams::Pocket { rest, .. } => rest,
        _ => panic!("P1 must stay a 2D Pocket"),
    }
}

fn draft_rest(state: &State) -> Option<RestParams> {
    match state.op_ui.params {
        OpParams::Pocket { rest, .. } => rest,
        _ => panic!("draft must stay a 2D Pocket"),
    }
}

fn on_row<'a>(h: &'a Harness<'static, State>, label: &str, role: Role) -> egui_kittest::Node<'a> {
    let lab = h.get_by_label(label).rect();
    h.query_all_by(move |n| n.role() == role)
        .find(|n| {
            let c = n.rect().center().y;
            c >= lab.top() - 2.0 && c <= lab.bottom() + 2.0
        })
        .unwrap_or_else(|| panic!("expected {role:?} on row {label:?}"))
}

fn set_row_number(h: &mut Harness<'static, State>, label: &str, text: &str) {
    let spin_rect = on_row(h, label, Role::SpinButton).rect();
    on_row(h, label, Role::SpinButton).click();
    h.run();
    h.query_all_by(|n| n.role() == Role::SpinButton)
        .find(|n| (n.rect().top() - spin_rect.top()).abs() < 1.0)
        .expect("editing spin button")
        .type_text(text);
    h.run();
}

fn click_apply(h: &mut Harness<'static, State>) {
    h.get_by_label("Apply").click();
    h.run();
    h.state_mut().drain_actions();
    h.run();
}

#[test]
fn rest_group_defaults_off_on_pocket_passes_only_and_hides_fields_when_off() {
    let mut h = page(opened_pocket(PocketStrategy::Auto, None));
    h.run();

    assert!(h.query_by_label("Rest machining").is_some(), "Pocket Passes shows Rest machining group");
    let switch = on_row(&h, "Rest machining", Role::CheckBox);
    assert_eq!(switch.accesskit_node().role(), Role::CheckBox);
    assert_eq!(switch.accesskit_node().toggled(), Some(Toggled::False), "off by default");

    for field in REST_FIELDS {
        assert!(h.query_by_label(field).is_none(), "{field} stays hidden while rest is off");
    }

    for tab in ["Geometry", "Heights", "Lead in / Lead out", "Feeds"] {
        h.state_mut().op_tab = sec_index(tab);
        h.run();
        assert!(h.query_by_label("Rest machining").is_none(), "Rest machining must not appear on {tab}");
    }

    h.state_mut().op_tab = sec_index("Passes");
    for ty in [OpType::Face, OpType::Adaptive2D, OpType::Offset2D] {
        let cur = h.state().op_ui.params.clone();
        h.state_mut().op_ui.params = crate::operations::retype(&cur, kind_index(ty));
        h.run();
        assert!(h.query_by_label("Rest machining").is_none(), "{ty} must not show 2D Pocket Rest machining");
    }
}

#[test]
fn rest_group_toggles_edits_draft_commits_via_apply_and_undoes() {
    let mut h = page(opened_pocket(PocketStrategy::Auto, None));
    h.run();

    on_row(&h, "Rest machining", Role::CheckBox).click();
    h.run();

    assert_eq!(
        draft_rest(h.state()).map(|r| r.previous_tool_diameter),
        Some(8.0),
        "switch arms rest machining in draft, seeded larger than mounted 4 mm tool"
    );
    assert_eq!(live_rest(h.state()), None, "switching in panel never writes the world");
    assert!(
        !h.state().pending.iter().any(|c| matches!(c, CommandKind::EditOperationParams { .. })),
        "toggling only edits the draft until Apply"
    );
    assert_eq!(
        on_row(&h, "Rest machining", Role::CheckBox).accesskit_node().toggled(),
        Some(Toggled::True),
        "header checkbox reflects active state"
    );

    assert_eq!(h.query_all_by_label("Rest machining").count(), 1, "Rest machining renders once on Passes");
    for field in REST_FIELDS {
        assert_eq!(h.query_all_by_label(field).count(), 1, "{field} is visible once while rest is on");
        let spin = on_row(&h, field, Role::SpinButton);
        assert!(spin.accesskit_node().labelled_by().next().is_some(), "{field} spin button carries labelled_by");
    }

    for tab in ["Geometry", "Heights", "Lead in / Lead out", "Feeds"] {
        h.state_mut().op_tab = sec_index(tab);
        h.run();
        assert!(h.query_by_label("Rest machining").is_none(), "Rest machining must not appear on {tab} even when active");
        for field in REST_FIELDS {
            assert!(h.query_by_label(field).is_none(), "{field} must not appear on {tab} when rest is active");
        }
    }
    h.state_mut().op_tab = sec_index("Passes");
    h.run();

    set_row_number(&mut h, "Prev. tool Ø", "10");
    set_row_number(&mut h, "Prev. corner rad.", "1.5");
    set_row_number(&mut h, "Prev. stock left", "0.25");
    set_row_number(&mut h, "Rest allowance", "0.1");

    let want = RestParams {
        source: Default::default(),
        previous_tool_diameter: 10.0,
        previous_corner_radius: 1.5,
        previous_stock_to_leave: 0.25,
        allowance: 0.1,
    };
    assert_eq!(draft_rest(h.state()), Some(want), "draft carries typed values");
    assert_eq!(live_rest(h.state()), None, "live op waits for Apply");

    click_apply(&mut h);
    assert_eq!(live_rest(h.state()), Some(want), "Apply commits via EditOperationParams");

    let st = h.state_mut();
    assert!(st.history.undo(&st.world));
    assert_eq!(live_rest(st), None, "undo restores off state");
    assert!(st.history.redo(&st.world));
    assert_eq!(live_rest(st), Some(want), "redo restores rest params");
    st.sync_ops_ui();
    h.run();

    on_row(&h, "Rest machining", Role::CheckBox).click();
    h.run();
    assert_eq!(draft_rest(h.state()), None, "turning switch off clears draft rest to None");
    for field in REST_FIELDS {
        assert!(h.query_by_label(field).is_none(), "{field} hides when switched back off");
    }
    assert_eq!(live_rest(h.state()), Some(want), "turning off still waits for Apply");

    click_apply(&mut h);
    assert_eq!(live_rest(h.state()), None, "Apply commits None");
    let st = h.state_mut();
    assert!(st.history.undo(&st.world));
    assert_eq!(live_rest(st), Some(want), "undo after disabling restores previous rest params");
}

#[test]
fn rest_group_refuses_non_larger_previous_tool_by_name_on_apply() {
    let mut h = page(opened_pocket(PocketStrategy::Auto, None));
    h.run();

    on_row(&h, "Rest machining", Role::CheckBox).click();
    h.run();
    set_row_number(&mut h, "Prev. tool Ø", "4");

    click_apply(&mut h);

    assert_eq!(live_rest(h.state()), None, "refused Apply writes nothing");
    let err = h.state().error_for(ErrorScope::Operation).expect("refusal message on Operation scope");
    assert!(err.contains("larger") && err.contains("4 mm"), "refusal names tool comparison: {err}");
    assert!(
        h.query_by_label_contains("parameters not updated").is_some()
            && h.query_by_label_contains("larger").is_some(),
        "refusal line renders in the page action area: {err}"
    );
}

fn live_strategy(state: &State) -> Option<PocketStrategy> {
    match model::find_by_name(&state.world, "P1").and_then(|e| model::operation_of(&state.world, &e))?.params {
        OpParams::Pocket { strategy, .. } => Some(strategy),
        _ => None,
    }
}

#[test]
fn rest_group_warns_and_refuses_incompatible_strategy_and_predrilled_on_apply() {
    // #1490 / #2037: Zigzag rest passes close each rest boundary, so Zigzag + rest is legal.
    let mut h = page(opened_pocket(PocketStrategy::Zigzag, None));
    h.run();
    on_row(&h, "Rest machining", Role::CheckBox).click();
    h.run();
    assert_eq!(h.state().op_ui.params.named_refusal(), None, "Zigzag + rest is not refused");
    assert!(h.query_by_label_contains("Apply will be refused").is_none(), "no Apply hint for Zigzag + rest");
    click_apply(&mut h);
    let committed = live_rest(h.state()).expect("Zigzag + rest commits on Apply");
    assert_eq!(live_strategy(h.state()), Some(PocketStrategy::Zigzag));
    assert!(h.state().error_for(ErrorScope::Operation).is_none(), "no refusal for Zigzag + rest");

    // Retired Adaptive and its rest-specific incompatibility both stay visible.
    if let OpParams::Pocket { strategy, .. } = &mut h.state_mut().op_ui.params {
        *strategy = PocketStrategy::Adaptive;
    }
    h.run();
    let adaptive_refusal = h.state().op_ui.params.named_refusal().expect("Adaptive draft refusal");
    assert!(adaptive_refusal.contains("Adaptive"), "{adaptive_refusal}");
    assert!(h.query_by_label_contains("not available with the Adaptive strategy").is_some(), "main's rest-specific refusal stays visible");
    assert!(
        h.query_by_label(&adaptive_refusal).is_some(),
        "consequence warns when Adaptive is active with rest machining"
    );
    assert!(
        h.query_by_label(&format!("Apply will be refused: {adaptive_refusal}.")).is_some(),
        "Apply hint also names the Adaptive refusal"
    );
    click_apply(&mut h);
    assert_eq!(live_strategy(h.state()), Some(PocketStrategy::Zigzag), "Adaptive + rest refuses on Apply");
    assert_eq!(live_rest(h.state()), Some(committed), "refused Apply keeps the committed rest");
    let msg = h.state().error_for(ErrorScope::Operation).expect("Adaptive refusal");
    assert!(msg.contains("Adaptive"), "{msg}");

    if let OpParams::Pocket { strategy, links, .. } = &mut h.state_mut().op_ui.params {
        *strategy = PocketStrategy::Auto;
        links.approach.predrilled = true;
    }
    h.run();
    assert!(
        h.query_by_label(REST_PREDRILLED).is_some(),
        "consequence warns when pre-drilled entry is on with rest machining"
    );
    assert!(
        h.query_by_label(&format!("Apply will be refused: {REST_PREDRILLED}.")).is_some(),
        "Apply hint also names the pre-drilled refusal"
    );
    click_apply(&mut h);
    assert_eq!(live_strategy(h.state()), Some(PocketStrategy::Zigzag), "predrilled + rest refuses on Apply");
    let msg = h.state().error_for(ErrorScope::Operation).expect("predrilled refusal");
    assert_eq!(msg, format!("parameters not updated: invalid parameters: {REST_PREDRILLED}"));
}

#[test]
fn rest_group_respects_document_unit_lens() {
    let mut state = opened_pocket(PocketStrategy::Auto, None);
    state.history.execute(&state.world, SetUnits::new(&state.world, Unit::Inches).expect("units"));
    let mut h = page(state);
    h.run();

    on_row(&h, "Rest machining", Role::CheckBox).click();
    h.run();
    set_row_number(&mut h, "Prev. tool Ø", "0.5");
    set_row_number(&mut h, "Rest allowance", "0.1");
    click_apply(&mut h);

    let committed = live_rest(h.state()).expect("committed inch rest");
    assert!((committed.previous_tool_diameter - 12.7).abs() < 1e-6, "0.5 in -> 12.7 mm, got {}", committed.previous_tool_diameter);
    assert!((committed.allowance - 2.54).abs() < 1e-6, "0.1 in -> 2.54 mm, got {}", committed.allowance);
}

#[test]
fn rest_group_auto_note_shows_waterline_when_rest_is_on() {
    let mut h = page(opened_pocket(PocketStrategy::Auto, None));
    h.run();
    assert!(
        h.query_by_label_contains("Auto -> Zigzag").is_some(),
        "rectangular pocket with rest off shows Auto -> Zigzag"
    );
    assert!(
        h.query_by_label_contains("Auto -> Waterline").is_none(),
        "rest off on rectangular pocket must not show Waterline"
    );

    on_row(&h, "Rest machining", Role::CheckBox).click();
    h.run();
    assert!(
        h.query_by_label_contains("Auto -> Waterline").is_some(),
        "Auto note uses rest_strategy(Auto) -> Waterline when rest is on in draft"
    );
    assert!(
        h.query_by_label_contains("Auto -> Zigzag").is_none(),
        "Auto note must not advertise Zigzag when rest machining is on"
    );

    on_row(&h, "Rest machining", Role::CheckBox).click();
    h.run();
    assert!(
        h.query_by_label_contains("Auto -> Zigzag").is_some(),
        "switching rest off restores outline-based Auto -> Zigzag note"
    );
    assert!(h.query_by_label_contains("Auto -> Waterline").is_none());

    let mut h_saved = page(opened_pocket(
        PocketStrategy::Auto,
        Some(RestParams {
            source: Default::default(),
            previous_tool_diameter: 8.0,
            previous_corner_radius: 0.0,
            previous_stock_to_leave: 0.0,
            allowance: 0.0,
        }),
    ));
    h_saved.run();
    assert!(
        h_saved.query_by_label_contains("Auto -> Waterline").is_some(),
        "persisted rest machining on Auto pocket shows Auto -> Waterline"
    );
    assert!(h_saved.query_by_label_contains("Auto -> Zigzag").is_none());
}

#[path = "rest_source_tests.rs"]
mod source;
