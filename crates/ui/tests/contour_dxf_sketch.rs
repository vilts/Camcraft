//! #1864 UI integration tests: DXF sketch chain clicking (`edge_pick` replace,
//! Shift+append, Ctrl toggle), `geo_chips`, and `State` command generation with
//! `SketchLayer` + `ChainOverride`.
//!
//! Gate map (M4.4): covers ui::edge_pick, ui::state, ui::pick_highlight,
//! ui::interact, ui::egui_app (geo_page, mill2d_page, fields).
//! Provenance: our own UI interaction & state tests; no vendor material consulted.

use camcraft_app::commands::{DxfOptions, ImportDxf};
use camcraft_app::model;
use camcraft_app::operation::{
    self, ChainOverride, ChainSide, FeatureSel, GeoSel, Height, HeightReference, OpParams, OpStatus, SrcRef,
};
use camcraft_app::sketch;
use camcraft_app::stock::StockShape;
use camcraft_app::tools::catalog;
use camcraft_ui::edge_pick::queue_edge_pick;
use camcraft_ui::interact::Mods;
use camcraft_ui::state::{CommandKind, State};

const TOOL: &str = "EM-0400-2FL";

fn plate_dxf() -> &'static str {
    "0\nSECTION\n2\nHEADER\n9\n$INSUNITS\n70\n4\n0\nENDSEC\n0\nSECTION\n2\nENTITIES\n\
     0\nLWPOLYLINE\n8\ncut\n90\n4\n70\n1\n10\n0\n20\n0\n10\n120\n20\n0\n10\n120\n20\n80\n10\n0\n20\n80\n\
     0\nCIRCLE\n8\nholes\n10\n25\n20\n55\n40\n8\n\
     0\nLINE\n8\nopen\n10\n20\n20\n10\n11\n100\n21\n10\n\
     0\nENDSEC\n0\nEOF\n"
}

fn contour_params() -> OpParams {
    let mut p = operation::seed_params(camcraft_app::operation::OpType::Contour.ordinal());
    if let OpParams::Contour { clearance_z, retract_z, top_z, bottom_z, .. } = &mut p {
        *clearance_z = Height::of(HeightReference::TopOfStock, 10.0);
        *retract_z = Height::of(HeightReference::TopOfStock, 5.0);
        *top_z = Height::of(HeightReference::TopOfStock, 0.0);
        *bottom_z = Height::of(HeightReference::TopOfStock, -2.0);
    }
    p
}

fn sketch_ui_state() -> State {
    let mut state = State::new();
    state.pending.push_back(CommandKind::SetStock {
        shape: StockShape::Box { dx: 300.0, dy: 240.0, dz: 10.0 },
        pos: [0.0, 0.0, 0.0],
        yaw_deg: 0.0,
    });
    let tool = catalog().into_iter().find(|(n, _)| *n == TOOL).map(|(_, t)| t).expect("catalog");
    state.pending.push_back(CommandKind::AddTool { name: TOOL.into(), tool });
    state.drain_actions();

    let cmd = ImportDxf::from_bytes(&state.world, None, "plate.dxf", plate_dxf().as_bytes(), &DxfOptions::default())
        .expect("import dxf");
    state.history.execute(&state.world, cmd);

    state.pending.push_back(CommandKind::CreateOperation {
        name: "Contour1".into(),
        tool: TOOL.into(),
        src: SrcRef::Entity("plate".into()),
        params: contour_params(),
    });
    state.pending.push_back(CommandKind::Select(Some("Contour1".into())));
    state.drain_actions();
    state.sync_ops_ui();
    state
}

#[test]
fn edge_pick_handles_sketch_chain_replace_shift_append_and_ctrl_toggle() {
    let mut state = sketch_ui_state();
    let (_, sk) = sketch::find(&state.world, "plate").expect("plate");
    let ids: Vec<_> = sk.data.chains().all().map(|c| c.id()).collect();
    assert_eq!(ids.len(), 3);
    let c0 = GeoSel { entity: "plate".into(), feature: FeatureSel::SketchChain(ids[0]) };
    let c1 = GeoSel { entity: "plate".into(), feature: FeatureSel::SketchChain(ids[1]) };

    // Plain click replaces entity source with [c0].
    queue_edge_pick(&mut state, c0.clone(), Mods::default());
    state.drain_actions();
    state.sync_ops_ui();
    assert_eq!(state.op_ui.geo, vec![c0.clone()]);

    // Shift+click appends c1 without attempting 3D face-loop extension!
    queue_edge_pick(&mut state, c1.clone(), Mods { shift: true, ..Mods::default() });
    state.drain_actions();
    state.sync_ops_ui();
    assert_eq!(state.op_ui.geo, vec![c0.clone(), c1.clone()]);

    // Ctrl+click toggles c0 back out, leaving [c1].
    queue_edge_pick(&mut state, c0.clone(), Mods { command: true, ..Mods::default() });
    state.drain_actions();
    state.sync_ops_ui();
    assert_eq!(state.op_ui.geo, vec![c1.clone()]);

    // geo_chips resolves SketchChain cleanly with the entity name.
    let chips = state.geo_chips();
    assert_eq!(chips.len(), 1);
    assert!(!chips[0].missing);
    assert!(chips[0].label.contains("plate"), "chip label: {}", chips[0].label);
}

#[test]
fn state_sketch_layer_pick_per_chain_override_and_generate_produces_ok_status() {
    let mut state = sketch_ui_state();
    let (_, sk) = sketch::find(&state.world, "plate").expect("plate");
    let hole_id = sk.data.chains().by_layer("holes")[0].id();

    let items = vec![GeoSel { entity: "plate".into(), feature: FeatureSel::SketchLayer("holes".into()) }];
    state.pending.push_back(CommandKind::SetOperationGeometry {
        name: "Contour1".into(),
        items: items.clone(),
    });
    state.drain_actions();
    state.sync_ops_ui();
    assert_eq!(state.op_ui.geo, items);

    let chips = state.geo_chips();
    assert_eq!(chips.len(), 1);
    assert!(!chips[0].missing);
    assert!(chips[0].label.contains("1 closed, 0 open"), "chip label: {}", chips[0].label);

    let mut params = state.op_ui.params.clone();
    if let OpParams::Contour { chain_overrides, .. } = &mut params {
        chain_overrides.push(ChainOverride {
            entity: "plate".into(),
            chain: hole_id,
            side: Some(ChainSide::Inside),
            reverse: true,
        });
    }
    state.pending.push_back(CommandKind::EditOperationParams { name: "Contour1".into(), params });
    state.pending.push_back(CommandKind::GenerateOperation("Contour1".into()));
    state.drain_actions();

    let op_e = model::find_by_name(&state.world, "Contour1").expect("Contour1");
    let op_status = operation::status_of(&state.world, &op_e);
    assert_eq!(op_status, OpStatus::Ok, "expected OpStatus::Ok, status={:?}, last_status={}", op_status, state.status);
    let tp = model::toolpath_of(&state.world, &op_e).expect("generated toolpath");
    assert!(!tp.path.moves.is_empty());
}
