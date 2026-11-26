//! #1449: the "Entry positions (pre-drilled)" group on the 2D Pocket
//! page (`Lead in / Lead out` tab) - off by default, the header switch
//! writes `links.approach.predrilled` into the draft, Apply commits it
//! through `EditOperationParams` (undo and redo included), and the
//! refusal beside rest machining is named in the group.
//! Provenance: our own page law over `camcraft_app::links::Approach`.

use egui::Vec2;
use egui::accesskit::{Role, Toggled};
use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT, Queryable};

use camcraft_app::commands::{AddTool, CreateOperation, ImportModel};
use camcraft_app::geometry::{Point3, cube};
use camcraft_app::machining2d::{PocketStrategy, RestParams, RestSource};
use camcraft_app::machining3d::Entry3D;
use camcraft_app::model;
use camcraft_app::operation::{Height, KindInfo, OpParams, OpType, REST_PREDRILLED, SrcRef};
use camcraft_app::stock::StockShape;
use camcraft_app::tools::catalog;

use super::op_tabs::SECTIONS;
use crate::state::{CommandKind, State, UiTarget};

const GROUP: &str = super::predrilled_page::PREDRILLED_GROUP;
const SOURCE: &str = "a drilling operation ahead of this pocket";
const LEADS: &str = "Lead in / Lead out";

fn sec_index(name: &str) -> usize {
    SECTIONS.iter().position(|s| *s == name).expect("registered section")
}

fn pocket(rest: Option<RestParams>) -> OpParams {
    OpParams::Pocket {
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
        strategy: PocketStrategy::Auto,
        entry: Entry3D::Helix,
        helix_radius: 1.0,
        optimal_load: camcraft_app::load::DEFAULT_OPTIMAL_LOAD,
        blend_turns: false,
        blend_radius: None,
        corner_relief: Default::default(),
        pass_start: Default::default(),
        rest,
        tilt: None,
        picks: Default::default(),
    }
}

/// Stock, a 40x40x10 part, a 4 mm end mill and a 2D Pocket "P1" opened
/// on the `Lead in / Lead out` tab.
fn opened_pocket(rest: Option<RestParams>) -> Harness<'static, State> {
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
        CreateOperation::new(&state.world, "P1", "EM-0400-2FL", SrcRef::Stock, pocket(rest)).expect("operation"),
    );
    state.pending.push_back(CommandKind::Select(Some("P1".into())));
    state.drain_actions();
    state.ui_target = UiTarget::Operation("P1".into());
    state.op_tab = sec_index(LEADS);
    state.sync_ops_ui();
    let mut h = Harness::builder()
        .with_size(Vec2::new(620.0, 2600.0))
        .build_ui_state(|ui, st: &mut State| super::pages::properties(ui, st, &super::theme::current()), state);
    h.run();
    h
}

fn live(state: &State) -> bool {
    let op = model::find_by_name(&state.world, "P1").and_then(|e| model::operation_of(&state.world, &e));
    op.expect("P1 stays an operation").params.links().expect("a pocket has links").approach.predrilled
}

fn draft(state: &State) -> bool {
    state.op_ui.params.links().expect("a pocket has links").approach.predrilled
}

fn switch<'a>(h: &'a Harness<'static, State>) -> egui_kittest::Node<'a> {
    let lab = h.get_by_label(GROUP).rect();
    h.query_all_by(|n| n.role() == Role::CheckBox)
        .find(|n| {
            let c = n.rect().center().y;
            c >= lab.top() - 2.0 && c <= lab.bottom() + 2.0
        })
        .expect("the group header carries its switch")
}

#[test]
fn the_pre_drilled_group_is_on_the_pocket_lead_tab_only_and_off_by_default() {
    let mut h = opened_pocket(None);

    assert_eq!(h.query_all_by_label(GROUP).count(), 1, "the group renders once on the pocket's lead tab");
    assert_eq!(switch(&h).accesskit_node().toggled(), Some(Toggled::False), "off by default");
    assert!(h.query_by_label_contains(SOURCE).is_none(), "the body stays hidden while the group is off");

    for tab in ["Geometry", "Heights", "Passes", "Feeds"] {
        h.state_mut().op_tab = sec_index(tab);
        h.run();
        assert!(h.query_by_label(GROUP).is_none(), "the group must not appear on {tab}");
    }

    // The other kinds refuse the flag at the door (`PREDRILLED_POCKET_ONLY`):
    // their pages must not offer it.
    for ty in [OpType::Contour, OpType::Adaptive2D, OpType::Slot] {
        let kind = KindInfo::ALL.iter().position(|k| k.ty == ty).expect("registered op kind");
        let cur = h.state().op_ui.params.clone();
        h.state_mut().op_ui.params = crate::operations::retype(&cur, kind);
        h.state_mut().op_tab = sec_index(LEADS);
        h.run();
        assert!(h.query_by_label(GROUP).is_none(), "{ty} must not show the pre-drilled group");
    }
}

#[test]
fn the_pre_drilled_group_switch_edits_the_draft_and_apply_commits_an_undoable_edit() {
    let mut h = opened_pocket(None);

    switch(&h).click();
    h.run();
    assert!(draft(h.state()), "the switch turns pre-drilled entry on in the draft");
    assert!(!live(h.state()), "the switch never writes the world");
    assert!(
        !h.state().pending.iter().any(|c| matches!(c, CommandKind::EditOperationParams { .. })),
        "the switch only edits the draft until Apply"
    );
    assert_eq!(switch(&h).accesskit_node().toggled(), Some(Toggled::True), "the header shows the on state");
    assert!(h.query_by_label_contains(SOURCE).is_some(), "the body says where the holes come from");

    h.get_by_label("Apply").click();
    h.run();
    h.state_mut().drain_actions();
    h.run();
    assert!(live(h.state()), "Apply commits the flag");

    let st = h.state_mut();
    assert!(st.history.undo(&st.world));
    assert!(!live(st), "undo turns pre-drilled entry off again");
    assert!(st.history.redo(&st.world));
    assert!(live(st), "redo turns it on again");
}

#[test]
fn the_pre_drilled_group_names_the_refusal_beside_rest_machining() {
    let rest = RestParams {
        source: RestSource::EnteredTool,
        previous_tool_diameter: 8.0,
        previous_corner_radius: 0.0,
        previous_stock_to_leave: 0.0,
        allowance: 0.0,
    };
    let mut h = opened_pocket(Some(rest));
    assert!(h.query_by_label(REST_PREDRILLED).is_none(), "no refusal while pre-drilled entry is off");

    switch(&h).click();
    h.run();
    assert!(h.query_by_label(REST_PREDRILLED).is_some(), "the group names the refusal that Apply will give");
}
