//! Gate map: covers ui::egui_app::heights_page,
//! ui::egui_app::mill2d_more_page, ui::operations.
//! Regression pins for height relationship notes in the production Heights page.
//! Provenance: the asymmetric fixture and local tool-axis display law from #2416.
use super::*;

fn retract_page(retract: f64) -> Harness<'static, State> {
    let mut state = asymmetric_tilted_pocket();
    state.op_ui.params = with_retract(state.op_ui.params, Height::fixed(retract));
    heights_tab(state)
}

#[test]
fn tilted_retract_below_clearance_does_not_report_setup_z_order() {
    let mut harness = retract_page(23.5);
    harness.run();
    assert!(harness.query_by_label("(= 21.000 along tool axis)").is_some());
    assert!(harness.query_by_label("(= 20.500 along tool axis)").is_some());
    assert!(harness.query_by_label_contains("above clearance -").is_none(),
        "local retract 20.5 is below local clearance 21");
}

#[test]
fn tilted_retract_above_clearance_names_the_local_hop_height() {
    let mut harness = retract_page(25.5);
    harness.run();
    assert!(harness.query_by_label("(= 21.000 along tool axis)").is_some());
    assert!(harness.query_by_label("(= 22.500 along tool axis)").is_some());
    assert!(harness.query_by_label("above clearance - the hop travels at 21.000 (min of the two)").is_some(),
        "the note must use the same folded local clearance as its row");
}

#[test]
fn retract_note_follows_an_unapplied_tilt_edit() {
    let mut state = asymmetric_tilted_pocket();
    state.op_ui.params = with_retract(state.op_ui.params, Height::fixed(37.0));
    if let OpParams::Pocket { tilt: Some(tilt), .. } = &mut state.op_ui.params {
        tilt.a_deg = 0.0;
        tilt.b_deg = 180.0;
    }
    let mut harness = heights_tab(state);
    harness.run();
    assert!(harness.query_by_label("(= 33.000 along tool axis)").is_some());
    assert!(harness.query_by_label("(= 34.000 along tool axis)").is_some());
    assert!(harness.query_by_label("above clearance - the hop travels at 33.000 (min of the two)").is_some(),
        "the note must read the draft frame rather than the saved A=-90 frame");
}

#[test]
fn tilted_corner_note_uses_the_same_local_frame_as_its_top_row() {
    let mut state = asymmetric_tilted_pocket();
    state.op_ui.params = OpParams::Chamfer2D {
        width: 1.0, angle_deg: 45.0,
        minimum_cone_radius: 2.0,
        clearance_z: Height::of(R::TopOfStock, 10.0), retract_z: None,
        top_z: Height::of(R::TopOfStock, 0.0),
        feed: 800.0, plunge_feed: 200.0, climb: true,
        side: camcraft_app::machining2d::Side::Outside,
        tolerance: 0.01, tilt: state.op_ui.params.tilt(),
        picks: Default::default(),
    };
    let mut harness = heights_tab(state);
    harness.run();
    assert!(harness.query_by_label("(= 11.000 along tool axis)").is_some());
    assert!(harness.query_by_label("the virtual cone tip stands at 8.000 - derived: (tip offset + width) x tan(angle) below the top plane").is_some(),
        "the tip is radius plus width below the local top, not below setup Z");
}

#[test]
fn chamfer_geometry_exposes_tip_offset() {
    let mut state = asymmetric_tilted_pocket();
    state.op_ui.params = OpParams::Chamfer2D {
        width: 1.0,
        angle_deg: 45.0,
        minimum_cone_radius: 2.0,
        clearance_z: Height::of(R::TopOfStock, 10.0),
        retract_z: None,
        top_z: Height::of(R::TopOfStock, 0.0),
        feed: 800.0,
        plunge_feed: 200.0,
        climb: true,
        side: camcraft_app::machining2d::Side::Outside,
        tolerance: 0.01,
        tilt: state.op_ui.params.tilt(),
        picks: Default::default(),
    };
    let mut harness = Harness::builder()
        .with_size(Vec2::new(600.0, 900.0))
        .build_ui_state(
            |ui, state: &mut State| {
                super::super::params_page::params_fields(
                    ui,
                    state,
                    super::super::op_tabs::SEC_GEO,
                )
            },
            state,
        );
    harness.run();
    assert!(
        harness.query_by_label("Tip offset").is_some(),
        "the Geometry page exposes the cone-band offset"
    );
    harness.get_by_label("Tip offset").hover();
    harness.run_steps(8);
    assert!(
        harness.query_by_label_contains("moves the tool away from the selected edge and lowers it").is_some(),
        "the field explains what the offset changes and what 0 means"
    );
}


fn clearance_stock_contour() -> State {
    use camcraft_app::machining2d::{Lead, Side};
    let mut state = opened(OpParams::Contour {
        tilt: None, stock_to_leave: 0.0,
        clearance_z: Height::fixed(8.0), retract_z: Height::fixed(8.0),
        top_z: Height::fixed(5.0), bottom_z: Height::fixed(0.0),
        stepdown: 1.0, feed: 800.0, plunge_feed: 200.0,
        climb: true, side: Side::Outside, lead: Lead::None, tolerance: 0.01,
        tab_count: 0, tab_width: 0.0, tab_height: 0.0,
        entry: camcraft_app::machining3d::Entry3D::Plunge,
        helix_radius: 1.0, blend_turns: false, blend_radius: None,
        links: camcraft_app::links::Links::PLANAR,
        chain_overrides: Vec::new(),
        picks: Default::default(),
    });
    state.pending.push_back(CommandKind::SetStock {
        shape: StockShape::Box { dx: 40.0, dy: 40.0, dz: 10.0 },
        pos: [0.0, 0.0, 0.0], yaw_deg: 0.0,
    });
    state.drain_actions();
    state
}

fn clearance_word(state: &State) -> String {
    camcraft_app::operation::clearance_in_stock_advice(
        &state.world, None, &state.op_ui.src(), &state.op_ui.params, Some("P1"),
    ).expect("clearance inside stock has advice")
}

#[test]
fn clearance_stock_advice_follows_draft_height_and_prior_face_before_generate() {
    let state = clearance_stock_contour();
    let note = clearance_word(&state);
    assert!(note.contains("Clearance Z 8.000 mm"));
    assert!(note.contains("top is Z 10.000 mm"));
    let mut harness = heights_tab(state);
    harness.run();
    assert!(harness.query_by_label(&note).is_some(), "Heights shows the same advice as MCP before Generate");
    let entity = camcraft_app::model::find_by_name(&harness.state().world, "P1").expect("operation");
    assert!(camcraft_app::model::toolpath_of(&harness.state().world, &entity).is_none());
    let builds = crate::height_memo::advice_builds();
    harness.run_steps(3);
    assert_eq!(crate::height_memo::advice_builds(), builds, "still frames reuse the advice");
    if let OpParams::Contour { clearance_z, .. } = &mut harness.state_mut().op_ui.params {
        *clearance_z = Height::fixed(10.0);
    }
    harness.run();
    assert!(harness.query_by_label(&note).is_none(), "an unapplied clearance at the stock top is quiet");
    assert!(crate::height_memo::advice_builds() > builds, "a draft clearance edit invalidates the advice");
    if let OpParams::Contour { clearance_z, .. } = &mut harness.state_mut().op_ui.params {
        *clearance_z = Height::fixed(8.0);
    }
    harness.run();
    assert!(harness.query_by_label(&note).is_some(), "the draft height drives the advice");
    let builds = crate::height_memo::advice_builds();
    let state = harness.state_mut();
    let face = OpParams::Face {
        holes: Default::default(), blend_turns: false, blend_radius: None, tilt: None,
        stepover: 4.0, clearance_z: Height::fixed(20.0), cut_z: Height::fixed(5.0),
        top_z: None, stepdown: None, feed: 1200.0, plunge_feed: 300.0, climb: true,
        picks: Default::default(),
    };
    state.history.execute(&state.world, CreateOperation::new(
        &state.world, "Face", "EM-0400-2FL", SrcRef::Stock, face,
    ).expect("face"));
    state.history.execute(&state.world, camcraft_app::commands::ReorderOperation::new(
        &state.world, "Face", "P1", false,
    ).expect("face ahead"));
    harness.run();
    assert!(harness.query_by_label(&note).is_none(), "a Face ahead leaves the verdict to Generate");
    assert!(crate::height_memo::advice_builds() > builds, "a program order change invalidates the advice");
    assert!(harness.state().pending.is_empty(), "drawing advice queues no command");
}

#[test]
fn clearance_stock_advice_is_only_on_heights_and_covers_new_drafts() {
    let mut state = clearance_stock_contour();
    let note = clearance_word(&state);
    state.op_ui.name = "new contour".into();
    state.op_tab = super::super::op_tabs::SEC_HEIGHTS;
    let mut harness = Harness::builder().with_size(Vec2::new(600.0, 900.0)).build_ui_state(
        |ui, st: &mut State| super::super::params_page::params_fields(ui, st, st.op_tab), state,
    );
    harness.run();
    assert!(harness.query_by_label(&note).is_none(), "the saved operation ahead suppresses a new draft warning");
    let state = harness.state_mut();
    state.op_ui.name = "P1".into();
    harness.run();
    assert!(harness.query_by_label(&note).is_some(), "the first operation names its own place in the program");
    harness.state_mut().op_tab = super::super::op_tabs::SEC_GEO;
    harness.run();
    assert!(harness.query_by_label(&note).is_none(), "Geometry does not show Heights advice");
    let state = harness.state_mut();
    state.history.execute(&state.world, camcraft_app::commands::DeleteEntity::new(
        &state.world, "P1",
    ).expect("delete operation"));
    state.op_ui.name = "new contour".into();
    state.op_tab = super::super::op_tabs::SEC_HEIGHTS;
    harness.run();
    assert!(harness.query_by_label(&note).is_some(), "a first unsaved draft also shows the advice");
}


#[test]
fn clearance_stock_advice_follows_units_and_stock_edits() {
    let state = clearance_stock_contour();
    let mm_note = clearance_word(&state);
    let mut harness = heights_tab(state);
    harness.run();
    assert!(harness.query_by_label(&mm_note).is_some());
    let state = harness.state_mut();
    state.history.execute(&state.world, camcraft_app::commands::SetUnits::new(
        &state.world, camcraft_app::units::Unit::Inches,
    ).expect("inches"));
    let inch_note = clearance_word(state);
    assert_ne!(inch_note, mm_note);
    harness.run();
    assert!(harness.query_by_label(&inch_note).is_some(), "unit changes refresh the advice text");
    assert!(harness.query_by_label(&mm_note).is_none());
    let state = harness.state_mut();
    state.pending.push_back(CommandKind::SetStock {
        shape: StockShape::Box { dx: 40.0, dy: 40.0, dz: 8.0 },
        pos: [0.0, 0.0, 0.0], yaw_deg: 0.0,
    });
    state.drain_actions();
    harness.run();
    assert!(harness.query_by_label(&inch_note).is_none(), "stock top at clearance removes the advice");
}
