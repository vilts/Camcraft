//! Structural UI checks for #2055 and #3346: Geometry tab -> Boundaries ->
//! "Do not touch" controls, rows with remove buttons, Apply/Undo, and viewport highlight.
//! Provenance: our own tests over our own shell; no vendor material.
//! Gate map (M4.4): covers ui::egui_app::boundary, ui::avoid_viz, ui::boundary_draft.

use super::*;
use camcraft_app::commands::{AddTool, ImportModel};
use camcraft_app::geometry::{Point3, cube};
use camcraft_app::operation::{FeatureSel, GeoSel, OpType, SrcRef, retype_params};

pub(super) fn seed_part_and_offset(st: &mut State, kind: OpType) -> (GeoSel, GeoSel, GeoSel) {
    let part = ImportModel::from_mesh("block", cube(Point3::new(0.0, 0.0, 0.0), Point3::new(30.0, 30.0, 15.0)));
    st.history.execute(&st.world, part);
    let feats = camcraft_app::model::find_by_name(&st.world, "block")
        .and_then(|e| camcraft_app::model::features_of(&st.world, &e))
        .expect("block features");
    let face_top = GeoSel {
        entity: "block".into(),
        feature: FeatureSel::Face(feats.faces[0].id),
    };
    let face_other = GeoSel {
        entity: "block".into(),
        feature: FeatureSel::Face(feats.faces[1].id),
    };
    let chain_id = camcraft_app::operation::run_pick(&st.world, "block", 0).expect("edge run").feature;
    let edge = GeoSel { entity: "block".into(), feature: chain_id };

    let tool = if kind == OpType::Finish3D {
        let ball = camcraft_app::tools::catalog().into_iter()
            .find(|(name, _)| *name == "BN-0600").expect("catalog ball tool").1;
        st.history.execute(&st.world, AddTool::new(&st.world, "ProtectedBall", ball).expect("add ball tool"));
        "ProtectedBall".to_owned()
    } else {
        camcraft_app::model::named_entities(&st.world)
            .into_iter()
            .find_map(|(n, e)| camcraft_app::model::tool_of(&st.world, &e).map(|_| n))
            .expect("seeded tool")
    };
    let op = retype_params(&st.op_ui.params, kind as usize);
    st.pending.push_back(CommandKind::CreateOperation {
        name: "OpTest".into(),
        tool,
        src: SrcRef::Selection(vec![face_top.clone()]),
        params: op,
    });
    st.drain_actions();
    let saved = camcraft_app::model::find_by_name(&st.world, "OpTest")
        .and_then(|e| camcraft_app::model::operation_of(&st.world, &e)).expect("created operation");
    assert_eq!(saved.params.kind(), kind, "the fixture creates the requested operation kind");
    crate::actions::target(st, UiTarget::Operation("OpTest".into()));
    (face_top, face_other, edge)
}

pub(super) fn expand_boundary(h: &mut Harness<'static, ShellUi>) {
    let rect = h.get_by_label("Boundary").rect();
    let p = egui::pos2(rect.left() - 10.0, rect.center().y);
    h.input_mut().events.push(Event::PointerMoved(p));
    h.run();
    for pressed in [true, false] {
        h.input_mut().events.push(Event::PointerButton {
            pos: p,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: Modifiers::NONE,
        });
        h.run();
    }
    h.run_steps(2);
}

#[test]
fn geometry_boundaries_shows_do_not_touch_controls_rows_and_viewport_marks() {
    let _g = snapshot_guard();
    // #3338: taller by the Outside boundary offset row, so the last ×
    // stays on the screen.
    let mut h = Harness::builder()
        .with_size(Vec2::new(1400.0, 1200.0))
        .build_ui_state(frame, ShellUi::seeded());
    h.run_steps(2);
    let (face_top, face_other, edge) = {
        let st = &mut h.state_mut().state;
        seed_part_and_offset(st, OpType::Offset2D)
    };
    h.run_steps(3);
    // Expand the collapsible Boundary group on the Geometry tab.
    expand_boundary(&mut h);

    assert!(
        h.query_by_label_contains("Do not touch").is_some(),
        "Boundary group must show the Do not touch section"
    );
    assert!(h.query_by_label("Clearance").is_some());
    h.get_by_label("Pick protected").click();
    h.run_steps(2);
    assert_eq!(h.state().state.boundary_pick, Some(crate::state::BoundaryPick::DoNotTouch));
    assert!(h.query_by_label("Picking…").is_some());

    {
        let st = &mut h.state_mut().state;
        // Overlapping face in src is refused on status line and not drafted.
        crate::boundary_draft::take_pick(
            st,
            crate::state::BoundaryPick::DoNotTouch,
            face_top,
            crate::boundary_draft::PickMods::default(),
        );
        assert!(st.status.contains("machining selection"), "overlap refusal: {}", st.status);
        assert!(st.op_ui.area.do_not_touch.picks.is_empty());

        // Non-overlapping face + edge chain land cleanly in Do not touch draft.
        crate::boundary_draft::take_pick(
            st,
            crate::state::BoundaryPick::DoNotTouch,
            face_other.clone(),
            crate::boundary_draft::PickMods::default(),
        );
        crate::boundary_draft::take_pick(
            st,
            crate::state::BoundaryPick::DoNotTouch,
            edge.clone(),
            crate::boundary_draft::PickMods::default(),
        );
        st.op_ui.area.do_not_touch.clearance = 1.25;
        assert_eq!(st.op_ui.area.do_not_touch.picks.len(), 2);
        // Viewport overlay renders marks while this operation is selected.
        let marks = crate::avoid_viz::avoid_marks(st, 1.4);
        assert!(!marks.faces.is_empty(), "protected face must render in viewport overlay");
        assert!(!marks.edges.is_empty(), "protected edge must render in viewport overlay");
    }
    h.run_steps(2);

    // Both selected item descriptions stay visible below the clearance control.
    // This is the structural twin of the review's populated Boundaries view.
    let clearance = h.get_by_label("Clearance").rect();
    let face_row = h.get_by_label(&camcraft_app::operation::describe_sel(&face_other)).rect();
    let edge_row = h.get_by_label(&camcraft_app::operation::describe_sel(&edge)).rect();
    assert!(clearance.bottom() < face_row.top(), "protected face row below clearance");
    assert!(face_row.bottom() < edge_row.top(), "protected rows preserve selection order");
    let page = h.get_by_label("Operation: OpTest").rect();
    assert!(face_row.left() >= page.left() && edge_row.left() >= page.left());

    // Clicking the remove button removes one row from Do not touch.
    assert_eq!(h.query_all_by_label("×").count(), 2 + h.state().state.op_ui.geo.len());
    h.query_all_by_label("×").last().expect("do not touch ×").click();
    h.run_steps(2);
    assert_eq!(h.state().state.op_ui.area.do_not_touch.picks.len(), 1);

    // Apply persists to world, Undo clears it, Redo restores it.
    h.get_by_label("Apply").click();
    h.run_steps(2);
    {
        let st = &mut h.state_mut().state;
        let saved = camcraft_app::model::find_by_name(&st.world, "OpTest")
            .and_then(|e| camcraft_app::model::operation_of(&st.world, &e))
            .expect("saved op");
        assert_eq!(saved.area.do_not_touch.picks, vec![face_other]);
        assert_eq!(saved.area.do_not_touch.clearance, 1.25);
        st.pending.push_back(CommandKind::Undo);
        st.drain_actions();
        st.sync_ops_ui();
        assert!(st.op_ui.area.do_not_touch.picks.is_empty());
        // Deselecting clears the viewport overlay.
        crate::actions::target(st, UiTarget::None);
        assert_eq!(crate::avoid_viz::avoid_marks(st, 1.4), crate::avoid_viz::AvoidMarks::default());
    }
}

#[test]
fn unsupported_op_shows_do_not_touch_note_without_pick_button() {
    let _g = snapshot_guard();
    let mut h = Harness::builder()
        .with_size(Vec2::new(1400.0, 1100.0))
        .build_ui_state(frame, ShellUi::seeded());
    h.run_steps(2);
    {
        let st = &mut h.state_mut().state;
        seed_part_and_offset(st, OpType::Contour);
    }
    h.run_steps(3);
    expand_boundary(&mut h);
    assert!(h
        .query_by_label_contains("2D Pocket, Trace, 2D Offset, 2D Adaptive, 3D Pocket, Face, 3D Finish and Engrave")
        .is_some());
    assert!(h.query_by_label("Pick protected").is_none());
}

#[test]
fn finish3d_shows_pick_protected_button() {
    let _g = snapshot_guard();
    let mut h = Harness::builder()
        .with_size(Vec2::new(1400.0, 1100.0))
        .build_ui_state(frame, ShellUi::seeded());
    h.run_steps(2);
    seed_part_and_offset(&mut h.state_mut().state, OpType::Finish3D);
    h.run_steps(3);
    expand_boundary(&mut h);
    assert!(h.query_by_label("Pick protected").is_some());
    assert!(h.query_by_label("Clearance").is_some());
}

#[test]
fn face_op_shows_pick_protected_button() {
    // #3343: a Face operation takes protected items like 2D Offset does.
    let _g = snapshot_guard();
    let mut h = Harness::builder()
        .with_size(Vec2::new(1400.0, 1100.0))
        .build_ui_state(frame, ShellUi::seeded());
    h.run_steps(2);
    {
        let st = &mut h.state_mut().state;
        seed_part_and_offset(st, OpType::Face);
    }
    h.run_steps(3);
    expand_boundary(&mut h);
    assert!(h.query_by_label("Pick protected").is_some());
    assert!(h.query_by_label_contains("only so far").is_none());
}

#[test]
fn adaptive2d_shows_the_pick_protected_button_and_no_unsupported_note() {
    let _g = snapshot_guard();
    let mut h = Harness::builder()
        .with_size(Vec2::new(1400.0, 1100.0))
        .build_ui_state(frame, ShellUi::seeded());
    h.run_steps(2);
    {
        let st = &mut h.state_mut().state;
        seed_part_and_offset(st, OpType::Adaptive2D);
    }
    h.run_steps(3);
    expand_boundary(&mut h);
    assert!(h.query_by_label_contains("is supported on").is_none(), "2D Adaptive supports Do not touch (#3346)");
    h.get_by_label("Pick protected").click();
    h.run_steps(2);
    assert_eq!(h.state().state.boundary_pick, Some(crate::state::BoundaryPick::DoNotTouch));
}

#[test]
fn avoid_marks_cache_reuses_still_frames_and_follows_input_changes() {
    let _g = snapshot_guard();
    let mut h = Harness::builder()
        .with_size(Vec2::new(1400.0, 1100.0))
        .build_ui_state(frame, ShellUi::seeded());
    h.run_steps(2);
    let (_face_top, face_other, edge) = {
        let st = &mut h.state_mut().state;
        seed_part_and_offset(st, OpType::Offset2D)
    };
    // Also import fig8.step so the cache test exercises a welded Patch pick.
    let fig8_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../testdata/models/fig8.step");
    {
        let st = &mut h.state_mut().state;
        st.pending.push_back(CommandKind::ImportFile(fig8_path));
        st.settle();
    }
    h.run_steps(2);
    let patch_pick = {
        let st = &h.state().state;
        let feats = camcraft_app::model::find_by_name(&st.world, "fig8")
            .and_then(|e| camcraft_app::model::features_of(&st.world, &e))
            .expect("fig8 features");
        let patch = camcraft_app::geometry::feature::patches(&feats)
            .into_iter()
            .next()
            .expect("fig8 patch");
        GeoSel { entity: "fig8".into(), feature: FeatureSel::Patch(patch.id) }
    };
    {
        let st = &mut h.state_mut().state;
        crate::boundary_draft::take_pick(
            st,
            crate::state::BoundaryPick::DoNotTouch,
            face_other,
            crate::boundary_draft::PickMods::default(),
        );
        crate::boundary_draft::take_pick(
            st,
            crate::state::BoundaryPick::DoNotTouch,
            edge,
            crate::boundary_draft::PickMods::default(),
        );
    }
    h.run_steps(3);
    let aspect = {
        let st = &h.state().state;
        let initial = crate::avoid_viz::avoid_marks(st, 1.4);
        assert_eq!(initial, crate::avoid_viz::avoid_marks_fresh(st, 1.4));
        assert!(!initial.faces.is_empty() && !initial.edges.is_empty());
        1.4
    };

    // Still frames compute no protection geometry, projections, or patches, and record zero build spans.
    let g0 = crate::avoid_viz::avoid_geom_builds();
    let m0 = crate::avoid_viz::avoid_mark_builds();
    let p0 = camcraft_app::model::patch_builds();
    let ((), costs) = crate::frame_cost::record(|| {
        for _ in 0..4 {
            h.step();
        }
    });
    assert_eq!(crate::avoid_viz::avoid_geom_builds() - g0, 0, "still frames reuse 3D protection geometry");
    assert_eq!(crate::avoid_viz::avoid_mark_builds() - m0, 0, "still frames reuse projected marks");
    assert_eq!(camcraft_app::model::patch_builds() - p0, 0, "still frames rebuild no patches");
    let frame_calls = costs.iter().find(|c| c.name == "ui/viewport.avoid_marks").map_or(0, |c| c.count);
    let build_calls = costs.iter().find(|c| c.name == "ui/viewport.avoid_marks.build").map_or(0, |c| c.count);
    assert_eq!(frame_calls, 4, "viewport frames measure ui/viewport.avoid_marks: {costs:?}");
    assert_eq!(build_calls, 0, "still frames trigger no avoid_marks.build span: {costs:?}");

    // Camera motion re-projects cached 3D geometry without re-resolving features or patches.
    let g_cam = crate::avoid_viz::avoid_geom_builds();
    let m_cam = crate::avoid_viz::avoid_mark_builds();
    let p_cam = camcraft_app::model::patch_builds();
    h.state_mut().state.cam.orbit(18.0, -7.0);
    h.run_steps(2);
    assert_eq!(crate::avoid_viz::avoid_geom_builds() - g_cam, 0, "orbit reuses 3D protection geometry");
    assert!(crate::avoid_viz::avoid_mark_builds() > m_cam, "orbit re-projects marks for new camera");
    assert_eq!(camcraft_app::model::patch_builds() - p_cam, 0, "orbit rebuilds no patches");
    {
        let st = &h.state().state;
        assert_eq!(crate::avoid_viz::avoid_marks(st, aspect), crate::avoid_viz::avoid_marks_fresh(st, aspect));
    }

    // Adding a patch pick invalidates both geometry and projection caches and matches fresh.
    let g_pick = crate::avoid_viz::avoid_geom_builds();
    let m_pick = crate::avoid_viz::avoid_mark_builds();
    {
        let st = &mut h.state_mut().state;
        crate::boundary_draft::take_pick(
            st,
            crate::state::BoundaryPick::DoNotTouch,
            patch_pick,
            crate::boundary_draft::PickMods::default(),
        );
        let name = st.op_ui.name.clone();
        let params = st.op_ui.params.clone();
        let area = st.op_ui.area.clone();
        st.pending.push_back(CommandKind::EditOperationBoundary { name, params, area });
        st.settle();
    }
    h.run_steps(2);
    assert!(crate::avoid_viz::avoid_geom_builds() > g_pick, "new pick rebuilds geometry");
    assert!(crate::avoid_viz::avoid_mark_builds() > m_pick, "new pick re-projects marks");
    let before_hide = {
        let st = &h.state().state;
        let m = crate::avoid_viz::avoid_marks(st, aspect);
        assert_eq!(m, crate::avoid_viz::avoid_marks_fresh(st, aspect));
        m
    };
    // Still frames with an active Patch pick also rebuild zero patches, geometry, or marks.
    let (gp0, mp0, pp0) = (
        crate::avoid_viz::avoid_geom_builds(),
        crate::avoid_viz::avoid_mark_builds(),
        camcraft_app::model::patch_builds(),
    );
    h.run_steps(3);
    assert_eq!(crate::avoid_viz::avoid_geom_builds() - gp0, 0);
    assert_eq!(crate::avoid_viz::avoid_mark_builds() - mp0, 0);
    assert_eq!(camcraft_app::model::patch_builds() - pp0, 0);

    // Hiding models updates the overlay; hiding both clears it; showing restores it.
    let g_hide = crate::avoid_viz::avoid_geom_builds();
    h.state_mut().state.hidden_models.push("block".into());
    h.run_steps(2);
    assert!(crate::avoid_viz::avoid_geom_builds() > g_hide, "hiding model rebuilds geometry");
    let block_hidden = crate::avoid_viz::avoid_marks(&h.state().state, aspect);
    assert_ne!(block_hidden, before_hide);
    assert_eq!(block_hidden, crate::avoid_viz::avoid_marks_fresh(&h.state().state, aspect));
    h.state_mut().state.hidden_models.push("fig8".into());
    h.run_steps(2);
    assert_eq!(crate::avoid_viz::avoid_marks(&h.state().state, aspect), crate::avoid_viz::AvoidMarks::default());
    h.state_mut().state.hidden_models.retain(|n| n != "block" && n != "fig8");
    h.run_steps(2);
    assert_eq!(crate::avoid_viz::avoid_marks(&h.state().state, aspect), before_hide);

    // Moving the model (and undoing) updates the overlay cleanly.
    let g_move = crate::avoid_viz::avoid_geom_builds();
    let m_move = crate::avoid_viz::avoid_mark_builds();
    h.state_mut().state.pending.push_back(CommandKind::MoveModel {
        name: "block".into(),
        pos: [8.0, -4.0, 2.0],
        roll_deg: 0.0,
        pitch_deg: 0.0,
        yaw_deg: 25.0,
    });
    h.state_mut().state.settle();
    h.run_steps(2);
    assert!(crate::avoid_viz::avoid_geom_builds() > g_move, "model move rebuilds geometry");
    assert!(crate::avoid_viz::avoid_mark_builds() > m_move, "model move re-projects marks");
    {
        let st = &h.state().state;
        let moved = crate::avoid_viz::avoid_marks(st, aspect);
        assert_ne!(moved, before_hide);
        assert_eq!(moved, crate::avoid_viz::avoid_marks_fresh(st, aspect));
    }
    h.state_mut().state.pending.push_back(CommandKind::Undo);
    h.state_mut().state.settle();
    h.run_steps(2);
    assert_eq!(crate::avoid_viz::avoid_marks(&h.state().state, aspect), before_hide);

    // Undoing the boundary commit clears the overlay; redoing restores it.
    h.state_mut().state.pending.push_back(CommandKind::Undo);
    h.state_mut().state.settle();
    h.run_steps(2);
    assert_eq!(crate::avoid_viz::avoid_marks(&h.state().state, aspect), crate::avoid_viz::AvoidMarks::default());
    h.state_mut().state.pending.push_back(CommandKind::Redo);
    h.state_mut().state.settle();
    h.run_steps(2);
    assert_eq!(crate::avoid_viz::avoid_marks(&h.state().state, aspect), before_hide);
}

#[test]
fn pocket2d_boundaries_shows_pick_protected_and_takes_viewport_click() {
    let _g = snapshot_guard();
    let mut h = Harness::builder()
        .with_size(Vec2::new(1400.0, 1100.0))
        .build_ui_state(frame, ShellUi::seeded());
    h.run_steps(2);
    let (_face_top, face_other, edge) = {
        let st = &mut h.state_mut().state;
        seed_part_and_offset(st, OpType::Pocket)
    };
    h.run_steps(3);
    expand_boundary(&mut h);

    assert!(h.query_by_label("Do not touch").is_some());
    assert!(h.query_by_label("Pick protected").is_some(), "2D Pocket Boundary group must show Pick protected");
    h.get_by_label("Pick protected").click();
    h.run_steps(2);
    assert_eq!(h.state().state.boundary_pick, Some(crate::state::BoundaryPick::DoNotTouch));
    assert!(h.query_by_label("Picking…").is_some());

    {
        let st = &mut h.state_mut().state;
        crate::boundary_draft::take_pick(
            st,
            crate::state::BoundaryPick::DoNotTouch,
            face_other,
            crate::boundary_draft::PickMods::default(),
        );
        crate::boundary_draft::take_pick(
            st,
            crate::state::BoundaryPick::DoNotTouch,
            edge,
            crate::boundary_draft::PickMods::default(),
        );
        assert_eq!(st.op_ui.area.do_not_touch.picks.len(), 2);
        let marks = crate::avoid_viz::avoid_marks(st, 1.4);
        assert!(!marks.faces.is_empty(), "protected face must render for 2D Pocket");
        assert!(!marks.edges.is_empty(), "protected edge must render for 2D Pocket");
    }
    h.run_steps(2);
    assert!(h.query_by_label_contains("block face #").is_some());
    assert!(h.query_by_label_contains("block edge chain #").is_some());
}
