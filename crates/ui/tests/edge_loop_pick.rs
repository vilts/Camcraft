//! Edge multi-select outside the boundary picker (user hand test
//! 2026-11-06): Shift+click on one rim edge takes the whole face loop it
//! lies on - one click closes a pocket rim and the 2D Adaptive
//! generates; Cmd/Ctrl (the platform command modifier, #1238) toggles one
//! edge in and back out.
//!
//! Gate map: covers ui::edge_pick, ui::interact, app::operation
//! (boundary_picks, selection), app::commands.
//! Provenance: our own pick law test; no vendor material consulted.

use camcraft_app::commands::ImportModel;
use camcraft_app::geometry::{Point3, cube, feature};
use camcraft_app::machining2d::PocketStrategy;
use camcraft_app::model;
use camcraft_app::operation::{self, FeatureSel, GeoSel, Height, OpParams, OpStatus, OpType, SrcRef};
use camcraft_app::stock::StockShape;
use camcraft_app::tools::catalog;
use camcraft_ui::edge_pick::queue_edge_pick;
use camcraft_ui::interact::Mods;
use camcraft_ui::state::{CommandKind, State};

const TOOL: &str = "EM-0400-2FL";

/// 60 x 60 x 20 stock, a 20 x 20 x 5 block part on its floor, and a 2D
/// Adaptive "A1" that will clear the stock above the block's top rim.
fn setup() -> State {
    let mut state = State::new();
    state.pending.push_back(CommandKind::SetStock {
        shape: StockShape::Box { dx: 60.0, dy: 60.0, dz: 20.0 },
        pos: [0.0, 0.0, 0.0],
        yaw_deg: 0.0,
    });
    let tool = catalog().into_iter().find(|(n, _)| *n == TOOL).map(|(_, t)| t).expect("catalog tool");
    state.pending.push_back(CommandKind::AddTool { name: TOOL.into(), tool });
    state.drain_actions();
    state.history.execute(
        &state.world,
        ImportModel::from_mesh("part", cube(Point3::new(-10.0, -10.0, 0.0), Point3::new(10.0, 10.0, 5.0))),
    );
    let pocket = OpParams::Pocket { rest: None,
        blend_radius: None,
        links: camcraft_app::links::Links::PLANAR,
        tilt: None,
        stepover: 1.5,
        stock_to_leave: 0.0,
        clearance_z: Height::fixed(30.0),
        retract_z: None,
        top_z: Height::fixed(20.0),
        bottom_z: Height::fixed(5.0),
        stepdown: 5.0,
        feed: 800.0,
        plunge_feed: 200.0,
        climb: true,
        strategy: PocketStrategy::Zigzag,
        entry: camcraft_app::machining3d::Entry3D::Plunge,
        helix_radius: 1.0,
        optimal_load: camcraft_app::load::DEFAULT_OPTIMAL_LOAD,
        blend_turns: false,
    };
    let params = operation::retype_params(&pocket, OpType::Adaptive2D.ordinal());
    state.pending.push_back(CommandKind::CreateOperation {
        name: "A1".into(),
        tool: TOOL.into(),
        src: SrcRef::Stock,
        params,
    });
    state.pending.push_back(CommandKind::Select(Some("A1".into())));
    state.drain_actions();
    state.sync_ops_ui();
    assert_eq!(state.op_ui.name, "A1");
    state
}

/// The open chain along the block's top rim at y = -10.
fn rim_edge(state: &State) -> GeoSel {
    let e = model::find_by_name(&state.world, "part").expect("part");
    let f = model::features_of(&state.world, &e).expect("features");
    let c = feature::chains(&f)
        .into_iter()
        .find(|c| {
            let p = feature::chain_points(&f, c);
            !c.closed && p.len() >= 2 && p.iter().all(|q| (q[2] - 5.0).abs() < 1e-9 && (q[1] + 10.0).abs() < 1e-9)
        })
        .expect("the rim edge is its own open chain");
    GeoSel { entity: "part".into(), feature: FeatureSel::Chain(c.id) }
}

fn picks(state: &State) -> Vec<GeoSel> {
    let e = model::find_by_name(&state.world, "A1").expect("op");
    match model::operation_of(&state.world, &e).expect("op").src {
        SrcRef::Selection(items) => items,
        other => panic!("a selection expected, got {other:?}"),
    }
}

#[test]
fn shift_click_on_one_rim_edge_takes_the_whole_loop_and_the_adaptive_generates() {
    let mut state = setup();
    let edge = rim_edge(&state);
    queue_edge_pick(&mut state, edge.clone(), Mods { shift: true, ..Default::default() });
    state.drain_actions();
    let items = picks(&state);
    assert_eq!(items.len(), 4, "the top rim's four runs: {items:?} / {}", state.status);
    assert!(state.status.contains("closed around its loop") || state.status.contains("updated"), "{}", state.status);
    state.pending.push_back(CommandKind::GenerateOperation("A1".into()));
    state.drain_actions();
    assert!(state.status.starts_with("generated A1:"), "{}", state.status);
    let e = model::find_by_name(&state.world, "A1").expect("op");
    assert_eq!(operation::status_of(&state.world, &e), OpStatus::Ok);
}

#[test]
fn command_click_toggles_one_edge_in_and_out() {
    let mut state = setup();
    let edge = rim_edge(&state);
    queue_edge_pick(&mut state, edge.clone(), Mods::default());
    state.drain_actions();
    assert_eq!(picks(&state), vec![edge.clone()]);
    // A second rim edge joins under the command modifier...
    let e = model::find_by_name(&state.world, "part").expect("part");
    let f = model::features_of(&state.world, &e).expect("features");
    let other = feature::chains(&f)
        .into_iter()
        .find(|c| {
            let p = feature::chain_points(&f, c);
            !c.closed && p.len() >= 2 && p.iter().all(|q| (q[2] - 5.0).abs() < 1e-9 && (q[0] - 10.0).abs() < 1e-9)
        })
        .map(|c| GeoSel { entity: "part".into(), feature: FeatureSel::Chain(c.id) })
        .expect("a second rim edge");
    queue_edge_pick(&mut state, other.clone(), Mods { command: true, ..Default::default() });
    state.drain_actions();
    assert_eq!(picks(&state), vec![edge.clone(), other.clone()]);
    // ...and the command modifier again takes it back out.
    queue_edge_pick(&mut state, other, Mods { command: true, ..Default::default() });
    state.drain_actions();
    assert_eq!(picks(&state), vec![edge]);
}

/// #1238: the add key is the platform command modifier, read from egui's
/// `Modifiers`. On macOS Control is the secondary click, so a Control press
/// (`ctrl` without `command`) must not toggle; Shift keeps its own meaning.
#[test]
fn the_command_modifier_from_egui_toggles_and_control_alone_does_not() {
    let cmd = Mods::from(egui::Modifiers::COMMAND);
    assert!(cmd.command && !cmd.shift && !cmd.alt, "{cmd:?}");
    let shift = Mods::from(egui::Modifiers { shift: true, ..Default::default() });
    assert!(shift.shift && !shift.command, "{shift:?}");
    let ctrl_only = Mods::from(egui::Modifiers { ctrl: true, ..Default::default() });
    assert!(!ctrl_only.command, "Control alone is not the add key: {ctrl_only:?}");

    let mut state = setup();
    let edge = rim_edge(&state);
    queue_edge_pick(&mut state, edge.clone(), Mods::default());
    state.drain_actions();
    let other = {
        let e = model::find_by_name(&state.world, "part").expect("part");
        let f = model::features_of(&state.world, &e).expect("features");
        feature::chains(&f)
            .into_iter()
            .find(|c| {
                let p = feature::chain_points(&f, c);
                !c.closed && p.len() >= 2 && p.iter().all(|q| (q[2] - 5.0).abs() < 1e-9 && (q[0] - 10.0).abs() < 1e-9)
            })
            .map(|c| GeoSel { entity: "part".into(), feature: FeatureSel::Chain(c.id) })
            .expect("a second rim edge")
    };
    // Control alone on macOS: a plain replace, not an add.
    queue_edge_pick(&mut state, other.clone(), ctrl_only);
    state.drain_actions();
    assert_eq!(picks(&state), vec![other.clone()], "Control alone replaces the selection");
    // The command modifier adds the first edge back.
    queue_edge_pick(&mut state, edge.clone(), cmd);
    state.drain_actions();
    assert_eq!(picks(&state), vec![other, edge]);
}

#[test]
fn shift_click_on_an_aligned_setup_closes_around_the_setup_horizontal_face() {
    use camcraft_app::commands::{ActivateSetup, AlignPartToSetup, CreateOperation, CreateSetup};

    let mut state = State::new();
    state.pending.push_back(CommandKind::SetStock {
        shape: StockShape::Box { dx: 60.0, dy: 60.0, dz: 60.0 },
        pos: [0.0, 0.0, -25.0],
        yaw_deg: 0.0,
    });
    let tool = catalog().into_iter().find(|(n, _)| *n == TOOL).map(|(_, t)| t).expect("catalog tool");
    state.pending.push_back(CommandKind::AddTool { name: TOOL.into(), tool });
    state.drain_actions();
    state.history.execute(
        &state.world,
        ImportModel::from_mesh("part", cube(Point3::new(-20.0, -10.0, 0.0), Point3::new(20.0, 10.0, 10.0))),
    );
    state.history.execute(&state.world, CreateSetup::new(&state.world, None, None).expect("Setup1"));
    state.history.execute(&state.world, CreateSetup::new(&state.world, None, None).expect("Setup2"));
    state.history.execute(&state.world, ActivateSetup::new(&state.world, Some("Setup2")).expect("Setup2 active"));
    state.pending.push_back(CommandKind::SetStock {
        shape: StockShape::Box { dx: 60.0, dy: 60.0, dz: 60.0 },
        pos: [0.0, 0.0, -25.0],
        yaw_deg: 0.0,
    });
    state.drain_actions();
    state.history.execute(
        &state.world,
        AlignPartToSetup::angles(&state.world, "Setup2", [90.0, 0.0, 0.0]).expect("align Setup2"),
    );
    let pocket = OpParams::Pocket {
        rest: None,
        blend_radius: None,
        links: camcraft_app::links::Links::PLANAR,
        tilt: None,
        stepover: 1.5,
        stock_to_leave: 0.0,
        clearance_z: Height::fixed(30.0),
        retract_z: None,
        top_z: Height::fixed(20.0),
        bottom_z: Height::fixed(5.0),
        stepdown: 5.0,
        feed: 800.0,
        plunge_feed: 200.0,
        climb: true,
        strategy: PocketStrategy::Zigzag,
        entry: camcraft_app::machining3d::Entry3D::Plunge,
        helix_radius: 1.0,
        optimal_load: camcraft_app::load::DEFAULT_OPTIMAL_LOAD,
        blend_turns: false,
    };
    state.history.execute(
        &state.world,
        CreateOperation::new(&state.world, "A2", TOOL, SrcRef::Stock, pocket).expect("A2"),
    );
    // Activate Setup1 (unaligned) while A2 (owned by Setup2) is open.
    state.history.execute(&state.world, ActivateSetup::new(&state.world, Some("Setup1")).expect("Setup1 active"));
    state.pending.push_back(CommandKind::Select(Some("A2".into())));
    state.drain_actions();
    state.sync_ops_ui();

    let part = model::find_by_name(&state.world, "part").expect("part");
    let f = model::features_of(&state.world, &part).expect("features");
    let edge = feature::chains(&f)
        .into_iter()
        .find(|c| {
            let p = feature::chain_points(&f, c);
            !c.closed && p.len() >= 2 && p.iter().all(|q| (q[1] - 10.0).abs() < 1e-9 && (q[2] - 10.0).abs() < 1e-9)
        })
        .map(|c| GeoSel { entity: "part".into(), feature: FeatureSel::Chain(c.id) })
        .expect("shared edge");
    queue_edge_pick(&mut state, edge, Mods { shift: true, ..Default::default() });
    state.drain_actions();

    let op_e = model::find_by_name(&state.world, "A2").expect("A2");
    let SrcRef::Selection(items) = model::operation_of(&state.world, &op_e).expect("op").src else {
        panic!("selection expected");
    };
    assert_eq!(items.len(), 4);
    for g in items {
        let FeatureSel::Chain(id) = g.feature else { panic!("chain expected") };
        let c = feature::resolve_chain(&f, id).expect("chain");
        let pts = feature::chain_points(&f, &c);
        assert!(pts.iter().all(|q| (q[1] - 10.0).abs() < 1e-6), "selected y=10 loop, got {pts:?}");
    }
}
