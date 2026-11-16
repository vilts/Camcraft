//! The operation page's footer and Geometry pick line, driven through the
//! real page code: Clear toolpath beside Generate (keeps the operation),
//! the renamed Delete operation, the Apply hint, and a refused wall pick
//! shown under the picks it is about.
//! Provenance: our own page law, specced from the hand-test report.

use egui::Vec2;
use egui_kittest::kittest::Queryable;
use egui_kittest::Harness;

use camcraft_app::commands::{AddTool, CreateOperation, GenerateOperation, ImportModel};
use camcraft_app::geometry::feature::FaceKind;
use camcraft_app::geometry::{cube, Point3};
use camcraft_app::machining2d::PocketStrategy;
use camcraft_app::model;
use camcraft_app::operation::{FeatureSel, GeoSel, Height, OpParams, SrcRef};
use camcraft_app::stock::StockShape;
use camcraft_app::tools::catalog;

use crate::state::{CommandKind, ErrorScope, State, UiTarget};

fn pocket() -> OpParams {
    OpParams::Pocket {
        blend_radius: None,
        links: camcraft_app::links::Links::PLANAR,
        tilt: None,
        stepover: 1.0,
        stock_to_leave: 0.0,
        clearance_z: Height::fixed(25.0),
        retract_z: None,
        top_z: Height::fixed(12.0),
        bottom_z: Height::fixed(9.0),
        stepdown: 1.0,
        feed: 800.0,
        plunge_feed: 200.0,
        climb: true,
        strategy: PocketStrategy::Zigzag,
        entry: camcraft_app::machining3d::Entry3D::Plunge,
        helix_radius: 1.0,
        optimal_load: camcraft_app::load::DEFAULT_OPTIMAL_LOAD,
        blend_turns: false,
    }
}

fn face(state: &State, vertical: bool) -> GeoSel {
    let e = model::find_by_name(&state.world, "block").expect("block");
    let f = model::features_of(&state.world, &e).expect("features");
    let id = f
        .faces
        .iter()
        .filter(|fa| matches!(fa.kind, FaceKind::Planar))
        .filter(|fa| (fa.normal[2].abs() < 0.5) == vertical)
        .max_by(|a, b| a.offset.total_cmp(&b.offset))
        .map(|fa| fa.id)
        .expect("a planar face");
    GeoSel { entity: "block".into(), feature: FeatureSel::Face(id) }
}

/// Stock, a 10x10x9 block, a tool, a generated pocket on the block's top
/// face, opened on the page.
fn opened() -> State {
    let mut state = State::new();
    state.pending.push_back(CommandKind::SetStock {
        shape: StockShape::Box { dx: 40.0, dy: 40.0, dz: 20.0 },
        pos: [0.0, 0.0, 0.0],
        yaw_deg: 0.0,
    });
    state.drain_actions();
    let em = catalog()
        .into_iter()
        .find(|(n, _)| *n == "EM-0400-2FL")
        .map(|(_, t)| t)
        .expect("catalog entry");
    state.history.execute(&state.world, AddTool::new(&state.world, "EM-0400-2FL", em).expect("tool"));
    state.history.execute(
        &state.world,
        ImportModel::from_mesh("block", cube(Point3::new(0.0, 0.0, 0.0), Point3::new(10.0, 10.0, 9.0))),
    );
    let src = SrcRef::Selection(vec![face(&state, false)]);
    state.history.execute(
        &state.world,
        CreateOperation::new(&state.world, "P1", "EM-0400-2FL", src, pocket()).expect("operation"),
    );
    state.history.execute(&state.world, GenerateOperation::new(&state.world, "P1").expect("generate"));
    state.doc_version += 1;
    state.pending.push_back(CommandKind::Select(Some("P1".into())));
    state.drain_actions();
    state.ui_target = UiTarget::Operation("P1".into());
    state.sync_ops_ui();
    state
}

fn page(state: State) -> Harness<'static, State> {
    Harness::builder()
        .with_size(Vec2::new(700.0, 1400.0))
        .build_ui_state(
            |ui, st: &mut State| super::pages::properties(ui, st, &super::theme::current()),
            state,
        )
}

fn has_path(state: &State) -> bool {
    model::find_by_name(&state.world, "P1").and_then(|e| model::toolpath_of(&state.world, &e)).is_some()
}

fn labels_containing(h: &Harness<'static, State>, text: &str) -> usize {
    // A Label carries its text as the node's value (its TextRun child
    // repeats it); count the Label nodes only.
    h.query_all_by(|n| {
        n.role() == egui::accesskit::Role::Label && n.value().is_some_and(|v| v.contains(text))
    })
    .count()
}

#[test]
fn clear_toolpath_sits_beside_generate_and_keeps_the_operation() {
    let mut harness = page(opened());
    harness.run();
    assert!(harness.query_by_label("Delete operation").is_some(), "the destructive verb says what it deletes");
    assert!(harness.query_by_label("Delete").is_none(), "no bare Delete on the page");
    harness.get_by_label("Clear toolpath").click();
    harness.run();
    harness.state_mut().drain_actions();
    harness.run();
    let state = harness.state();
    assert!(!has_path(state), "the generated path is gone");
    assert!(model::find_by_name(&state.world, "P1").is_some(), "the operation is kept");
    assert!(labels_containing(&harness, "never generated") > 0, "the page says so");
}

#[test]
fn apply_says_why_it_is_dark() {
    let mut harness = page(opened());
    harness.run();
    // The reason is the disabled button's hover, not an extra sentence.
    assert_eq!(labels_containing(&harness, "no parameter differs from the operation"), 0, "no sentence under the bar");
    harness.get_by_label("Apply").hover();
    harness.run_steps(8);
    assert!(
        labels_containing(&harness, "no parameter differs from the operation") > 0,
        "a dark Apply names its reason on hover"
    );
}

#[test]
fn a_refused_wall_pick_is_shown_under_the_picks() {
    let mut state = opened();
    let wall = face(&state, true);
    state.pending.push_back(CommandKind::SetOperationGeometry { name: "P1".into(), items: vec![wall] });
    state.drain_actions();
    let msg = state.error_for(ErrorScope::Operation).map(str::to_owned).expect("refused at the pick");
    assert!(msg.contains("projects to a line from above"), "{msg}");
    assert!(has_path(&state), "the refusal wrote nothing");
    let mut harness = page(state);
    harness.run();
    // Under the picks AND in the footer: the Geometry tab is the open one.
    assert!(
        labels_containing(&harness, "select a horizontal face or a closed edge chain") >= 2,
        "the pick line carries the refusal and its fix"
    );
}
