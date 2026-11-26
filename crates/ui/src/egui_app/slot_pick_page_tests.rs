//! #1504: the Slot page with picks. Two opposite top edges of a block,
//! picked through the queue's `SetOperationGeometry`, seed the slot that
//! fills them; the Geometry tab echoes the room the slot stays inside,
//! a draft too wide for the picks reads the refusal generation would
//! make, and Ctrl+Z takes the pick and the seed back together - the
//! echo with them. Structural (AccessKit labels), never pixels.
//! Provenance: our own page law; no vendor material consulted.

use egui_kittest::kittest::Queryable;

use camcraft_app::commands::{AddTool, CreateOperation, ImportModel};
use camcraft_app::geo2d::Point2;
use camcraft_app::geometry::{Point3, cube, feature};
use camcraft_app::model;
use camcraft_app::operation::{FeatureSel, GeoSel, Height, OpParams, SrcRef};
use camcraft_app::stock::StockShape;
use camcraft_app::tools::catalog;

use super::dialog_tests::page;
use crate::state::{CommandKind, State, UiTarget};

const MILL: &str = "EM-1000-4FL";
const ECHO: &str = "From selection: room 40.000 x 20.000 mm - the slot stays inside it";

/// A 40 x 20 x 10 block, a 10 mm end mill and a Slot "Op" typed across
/// the block, opened on its Geometry tab.
pub(super) fn opened() -> State {
    let mut state = State::new();
    state.history.execute(
        &state.world,
        ImportModel::from_mesh("part", cube(Point3::new(0.0, 0.0, 0.0), Point3::new(40.0, 20.0, 10.0))),
    );
    state.pending.push_back(CommandKind::SetStock {
        shape: StockShape::Box { dx: 60.0, dy: 40.0, dz: 10.0 },
        pos: [-10.0, -10.0, 0.0],
        yaw_deg: 0.0,
    });
    state.drain_actions();
    let mill = catalog().into_iter().find(|(n, _)| *n == MILL).map(|(_, t)| t).expect("catalog end mill");
    state.history.execute(&state.world, AddTool::new(&state.world, MILL, mill).expect("tool"));
    let params = OpParams::Slot {
        links: camcraft_app::links::Links::PLANAR,
        a: Point2::new(-20.0, 0.0),
        b: Point2::new(20.0, 0.0),
        width: 12.0,
        stock_to_leave: 0.0,
        clearance_z: Height::fixed(30.0),
        retract_z: None,
        top_z: Height::fixed(10.0),
        bottom_z: Height::fixed(5.0),
        stepdown: 5.0,
        stepover: 3.0,
        feed: 1200.0,
        plunge_feed: 300.0,
        climb: true,
        entry: camcraft_app::machining3d::Entry3D::Plunge,
        helix_radius: 1.0, blend_turns: false, blend_radius: None,
        tilt: None,
        extend_start: 0.0,
        extend_end: 0.0,
        picks: Default::default(),
    };
    state.history.execute(
        &state.world,
        CreateOperation::new(&state.world, "Op", MILL, SrcRef::Entity("part".into()), params).expect("operation"),
    );
    state.pending.push_back(CommandKind::Select(Some("Op".into())));
    state.drain_actions();
    state.ui_target = UiTarget::Operation("Op".into());
    state.sync_ops_ui();
    state.op_tab = super::op_tabs::SEC_GEO;
    state
}

/// The block's two long top edges (y 0 and y 20, z 10).
fn long_top_edges(state: &State) -> Vec<GeoSel> {
    let e = model::find_by_name(&state.world, "part").expect("model");
    let f = model::features_of(&state.world, &e).expect("features");
    [0.0, 20.0]
        .into_iter()
        .map(|y| {
            let c = feature::chains(&f)
                .into_iter()
                .find(|c| {
                    let p = feature::chain_points(&f, c);
                    !c.closed && p.len() >= 2 && p.iter().all(|q| (q[2] - 10.0).abs() < 1e-9 && (q[1] - y).abs() < 1e-9)
                })
                .unwrap_or_else(|| panic!("the top edge at y {y} is its own open chain"));
            GeoSel { entity: "part".into(), feature: FeatureSel::Chain(c.id) }
        })
        .collect()
}

fn width(state: &State) -> f64 {
    match model::find_by_name(&state.world, "Op").and_then(|e| model::operation_of(&state.world, &e)).map(|o| o.params) {
        Some(OpParams::Slot { width, .. }) => width,
        other => panic!("a slot op expected, got {other:?}"),
    }
}

#[test]
fn picked_edges_seed_the_slot_echo_its_room_and_undo_together() {
    let mut state = opened();
    let picks = long_top_edges(&state);
    state.pending.push_back(CommandKind::SetOperationGeometry { name: "Op".into(), items: picks });
    state.drain_actions();
    assert!((width(&state) - 20.0).abs() < 1e-9, "the pick seeded the edges' width");
    let mut harness = page(state);
    harness.run();
    assert!(harness.query_by_label(ECHO).is_some(), "the Geometry tab echoes the picks' room");
    // A draft wider than the picks reads the refusal, by name.
    if let OpParams::Slot { width, .. } = &mut harness.state_mut().op_ui.params {
        *width = 30.0;
    }
    harness.run();
    harness.run();
    assert!(harness.query_by_label(ECHO).is_none(), "a slot past its picks is not echoed as fine");
    let refusal = "invalid machining parameters: the slot does not fit the selection: it is 30.000 mm wide across the slot, but the picked edges stand only 20.000 mm apart - narrow it or re-pick";
    assert!(harness.query_by_label(refusal).is_some(), "the refusal generation would make");
    // Ctrl+Z takes the pick and the seed back together.
    let st = harness.state_mut();
    st.pending.push_back(CommandKind::Undo);
    st.drain_actions();
    st.sync_ops_ui();
    assert!((width(st) - 12.0).abs() < 1e-9, "the typed width is back");
    harness.run();
    harness.run();
    assert!(harness.query_by_label(ECHO).is_none() && harness.query_by_label(refusal).is_none(), "no picks, no echo");
    assert!(harness.query_by_label("Slot width").is_some(), "the typed fields stay");
}
