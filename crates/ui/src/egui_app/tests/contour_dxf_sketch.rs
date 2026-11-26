//! #1864: `egui_kittest::Harness` shell test for DXF sketch layer picking
//! and `mill2d_page` per-chain overrides driving `GenerateOperation` to `OpStatus::Ok`.
//!
//! Provenance: our own page harness test; no vendor material consulted.

use camcraft_app::commands::{DxfOptions, ImportDxf};
use camcraft_app::operation::{self, GeoSel, HeightReference, OpStatus};
use camcraft_app::stock::StockShape;
use camcraft_app::tools::catalog;

use super::*;

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

fn sketch_shell() -> ShellUi {
    let mut sh = ShellUi::blank();
    let state = &mut sh.state;
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
    sh
}

#[test]
fn geo_page_layer_picker_and_mill2d_per_chain_overrides_work_and_generate() {
    let mut harness = Harness::builder()
        .with_size(egui::vec2(2400.0, 1200.0))
        .build_ui_state(frame, sketch_shell());
    harness.run_steps(2);

    // Click layer button "holes (1)" in the Geometry tab.
    harness.get_by_label("holes (1)").click();
    harness.run_steps(3);
    assert_eq!(
        harness.state().state.op_ui.geo,
        vec![GeoSel { entity: "plate".into(), feature: FeatureSel::SketchLayer("holes".into()) }],
        "clicking layer button should select SketchLayer"
    );
    let chips = harness.state().state.geo_chips();
    assert_eq!(chips.len(), 1);
    assert!(chips[0].label.contains("1 closed, 0 open"), "layer chip label: {}", chips[0].label);

    // Toggle "Reverse" under Per-chain overrides on the active Geometry tab.
    harness.get_by_label("Reverse").click();
    harness.run_steps(2);
    let has_reverse = match &harness.state().state.op_ui.params {
        OpParams::Contour { chain_overrides, .. } => chain_overrides.len() == 1 && chain_overrides[0].reverse,
        _ => false,
    };
    assert!(has_reverse, "clicking Reverse checkbox should set per-chain reverse override");

    // Apply draft and generate: operation should become OpStatus::Ok.
    let target = harness.state().state.op_ui.name.clone();
    let edited = harness.state().state.op_ui.params.clone();
    let st = &mut harness.state_mut().state;
    st.pending.push_back(CommandKind::EditOperationParams { name: target.clone(), params: edited });
    st.pending.push_back(CommandKind::GenerateOperation(target.clone()));
    st.drain_actions();

    let op_e = model::find_by_name(&harness.state().state.world, &target).expect("op entity");
    let op_status = operation::status_of(&harness.state().state.world, &op_e);
    assert_eq!(
        op_status,
        OpStatus::Ok,
        "expected OpStatus::Ok, got {:?}, status={}",
        op_status,
        harness.state().state.status
    );
    let tp = model::toolpath_of(&harness.state().state.world, &op_e).expect("generated toolpath");
    assert!(!tp.path.moves.is_empty());
}

#[test]
fn whole_sketch_entity_source_stays_selected_and_shows_per_chain_overrides_before_layer_click() {
    let mut harness = Harness::builder()
        .with_size(egui::vec2(2400.0, 1200.0))
        .build_ui_state(frame, sketch_shell());
    harness.run_steps(2);

    // Before any layer click, SrcRef::Entity("plate") must stay in Source and draft.
    assert_eq!(
        harness.state().state.op_ui.src(),
        SrcRef::Entity("plate".into()),
        "whole-sketch SrcRef::Entity must not reset to Stock"
    );
    assert_eq!(
        harness
            .query_all_by(|n| {
                n.role() == egui::accesskit::Role::ComboBox
                    && n.value().as_deref() == Some("plate")
            })
            .count(),
        1,
        "Source combo should display the placed DXF sketch name 'plate'"
    );
    assert!(
        harness
            .query_by_label("Whole of 'plate' - the first pick replaces this.")
            .is_some(),
        "Geometry tab should show whole-sketch label before any layer click"
    );
    assert!(
        harness.query_by_label("Per-chain overrides").is_some(),
        "Per-chain overrides should render for whole-sketch SrcRef::Entity"
    );
    assert_eq!(
        harness.get_all_by_label("Reverse").count(),
        3,
        "plate.dxf has 3 chains (cut, holes, open)"
    );

    // Toggle the first chain's Reverse override directly under whole-sketch mode and generate.
    harness
        .get_all_by_label("Reverse")
        .next()
        .expect("first chain reverse checkbox")
        .click();
    harness.run_steps(2);
    let has_reverse = match &harness.state().state.op_ui.params {
        OpParams::Contour { chain_overrides, .. } => {
            chain_overrides.len() == 1 && chain_overrides[0].reverse
        }
        _ => false,
    };
    assert!(has_reverse, "clicking Reverse on whole sketch sets override");

    let target = harness.state().state.op_ui.name.clone();
    let edited = harness.state().state.op_ui.params.clone();
    let st = &mut harness.state_mut().state;
    st.pending.push_back(CommandKind::EditOperationParams {
        name: target.clone(),
        params: edited,
    });
    st.pending.push_back(CommandKind::GenerateOperation(target.clone()));
    st.drain_actions();

    let op_e = model::find_by_name(&harness.state().state.world, &target).expect("op entity");
    assert_eq!(operation::status_of(&harness.state().state.world, &op_e), OpStatus::Ok);
}

#[test]
fn sketch_frame_caches_skip_still_frames_and_track_changed_inputs() {
    let mut sh = sketch_shell();
    {
        let st = &mut sh.state;
        let mut p = contour_params();
        if let OpParams::Contour { top_z, bottom_z, .. } = &mut p {
            *top_z = Height::of(HeightReference::TopOfSelection, 0.0);
            *bottom_z = Height::of(HeightReference::BottomOfSelection, -2.0);
        }
        st.pending.push_back(CommandKind::EditOperationParams {
            name: "Contour1".into(),
            params: p,
        });
        st.drain_actions();
        st.sync_ops_ui();
    }
    let mut harness = Harness::builder()
        .with_size(egui::vec2(2400.0, 1200.0))
        .build_ui_state(frame, sh);
    harness.run_steps(2);

    // Still frames perform zero sketch recomputes and emit the frame-cost spans.
    let w0 = operation::selection_walks();
    let i0 = operation::sketch_chain_info_builds();
    let l0 = crate::sketch_memo::sketch_layer_builds();
    let z0 = operation::sketch_z_builds();
    let u0 = crate::height_memo::unresolved_builds();
    let f0 = operation::sketch_z_fresh_reads();
    let ((), costs) = crate::frame_cost::record(|| {
        for _ in 0..4 {
            harness.step();
        }
    });
    assert_eq!(operation::selection_walks() - w0, 0, "still frame walks no sketch contour_auto_side");
    assert_eq!(operation::sketch_chain_info_builds() - i0, 0, "still frame scans no sketch_chain_infos");
    assert_eq!(crate::sketch_memo::sketch_layer_builds() - l0, 0, "still frame scans no sketch layers");
    assert_eq!(operation::sketch_z_builds() - z0, 0, "still frame samples no sketch Z range");
    assert_eq!(operation::sketch_z_fresh_reads() - f0, 0, "still frame takes no authoritative sketch Z read");
    assert_eq!(crate::height_memo::unresolved_builds() - u0, 0, "still frame resolves no unresolved_note");
    for span in [
        "ui/properties.op.fields.contour_auto_side",
        "ui/properties.op.fields.sketch_chain_overrides",
        "ui/properties.op.picks.sketch_layers",
        "ui/properties.op.unresolved",
    ] {
        assert!(
            costs.iter().any(|c| c.name == span && c.count == 4),
            "expected span {span} x4 in {costs:?}"
        );
    }

    // Fresh vs cached equality on whole sketch, after layer click, and after stock/sketch changes.
    let assert_fresh = |h: &Harness<'static, ShellUi>| {
        let st = &h.state().state;
        let op = model::find_by_name(&st.world, "Contour1")
            .and_then(|e| model::operation_of(&st.world, &e))
            .expect("op");
        let setup = op.setup.as_deref();
        assert_eq!(
            operation::contour_auto_side(&st.world, setup, &op.src, &st.op_ui.params),
            operation::contour_auto_side_fresh(&st.world, setup, &op.src, &st.op_ui.params),
        );
        assert_eq!(
            operation::sketch_chain_infos(&st.world, &st.op_ui.src()),
            operation::sketch_chain_infos_fresh(&st.world, &st.op_ui.src()),
        );
        let placed = camcraft_app::sketch::placed_in(&st.world, setup);
        assert_eq!(
            crate::sketch_memo::sketch_layers(&st.world, &placed).to_vec(),
            crate::sketch_memo::sketch_layers_fresh(&st.world, &placed),
        );
        assert_eq!(
            crate::operations::unresolved_note(&st.world, setup, Some(&st.op_ui.src()), &st.op_ui.params),
            operation::resolve_params(&st.world, setup, Some(&st.op_ui.src()), &st.op_ui.params)
                .err()
                .map(|why| format!("{why}; set the source up or switch the reference")),
        );
    };
    assert_fresh(&harness);

    // Clicking a layer changes src -> recomputes chain infos and contour_auto_side once.
    let i1 = operation::sketch_chain_info_builds();
    let w1 = operation::selection_walks();
    let cb = crate::sketch_chip_memo::sketch_chip_builds();
    harness.get_by_label("holes (1)").click();
    harness.run_steps(2);
    assert!(crate::sketch_chip_memo::sketch_chip_builds() > cb, "the new layer chip is built through its cache");
    assert!(operation::sketch_chain_info_builds() > i1);
    assert!(operation::selection_walks() > w1);
    assert_fresh(&harness);

    // The layer pick shows a Geometry chip; still frames read it from the cache.
    let chips = harness.state().state.geo_chips();
    assert!(chips.iter().any(|c| c.label.contains("1 closed, 0 open") && !c.missing), "{:?}", chips.iter().map(|c| &c.label).collect::<Vec<_>>());
    let c0 = crate::sketch_chip_memo::sketch_chip_builds();
    for _ in 0..4 {
        harness.step();
    }
    assert_eq!(crate::sketch_chip_memo::sketch_chip_builds() - c0, 0, "still frame reads no sketch chip");

    // Changing stock Z height recomputes sketch_z_range and unresolved_note once, and undo restores.
    let z1 = operation::sketch_z_builds();
    let u1 = crate::height_memo::unresolved_builds();
    {
        let st = &mut harness.state_mut().state;
        st.pending.push_back(CommandKind::SetStock {
            shape: StockShape::Box { dx: 300.0, dy: 240.0, dz: 25.0 },
            pos: [0.0, 0.0, 0.0],
            yaw_deg: 0.0,
        });
        st.drain_actions();
    }
    harness.run_steps(2);
    assert!(operation::sketch_z_builds() > z1);
    assert!(crate::height_memo::unresolved_builds() > u1);
    assert_fresh(&harness);

    // Importing a second DXF sketch recomputes sketch_layer_picks once.
    let l1 = crate::sketch_memo::sketch_layer_builds();
    {
        let st = &mut harness.state_mut().state;
        let cmd = ImportDxf::from_bytes(&st.world, None, "second.dxf", plate_dxf().as_bytes(), &DxfOptions::default())
            .expect("second dxf");
        st.history.execute(&st.world, cmd);
    }
    harness.run_steps(2);
    assert!(crate::sketch_memo::sketch_layer_builds() > l1);
    assert_fresh(&harness);

    {
        let st = &mut harness.state_mut().state;
        st.pending.push_back(CommandKind::Undo);
        st.drain_actions();
    }
    harness.run_steps(2);
    assert_fresh(&harness);
}

#[test]
fn start_exit_marks_read_the_sketch_height_through_the_display_memo() {
    // #3161: the Lead tab's start/exit marks are a display read. A still
    // frame must not resample a DXF sketch's Z span.
    let mut st = sketch_shell().state;
    if let OpParams::Contour { top_z, .. } = &mut st.op_ui.params {
        *top_z = Height::of(HeightReference::TopOfSelection, 0.0);
    }
    st.ui_target = UiTarget::Operation("Contour1".into());
    st.sync_ops_ui();
    st.op_tab = crate::egui_app::op_tabs::SEC_LEADS;
    st.properties_shown = true;

    let z_prime = operation::sketch_z_builds();
    let _ = crate::start_exit_pick::marks(&st, 1.5);
    assert!(
        operation::sketch_z_builds() > z_prime,
        "the marks resolve the sketch height through the memo"
    );

    let z0 = operation::sketch_z_builds();
    let f0 = operation::sketch_z_fresh_reads();
    for _ in 0..3 {
        let _ = crate::start_exit_pick::marks(&st, 1.5);
    }
    assert_eq!(
        operation::sketch_z_builds() - z0,
        0,
        "a still frame resamples no sketch Z span"
    );
    assert_eq!(
        operation::sketch_z_fresh_reads() - f0,
        0,
        "a still frame takes no authoritative sketch Z read"
    );
}
