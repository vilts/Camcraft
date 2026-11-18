//! #1448 / #1473 item 2: viewport pick for 2D Contour Start point and Exit
//! point on the Lead in / Lead out tab. Real viewport clicks on the seed's
//! 60 x 60 x 30 block land in the draft params, Apply/Generate commit through
//! the command history (undo/redo restore the previous path and point), Clear
//! resets to automatic, switching tabs disarms picking, and clicking a fixture
//! is refused by name.
//! Provenance: our own viewport-pick test over our own shell; no vendor
//! material consulted.

use camcraft_app::commands::ImportModel;
use camcraft_app::geo2d::Point2;
use camcraft_app::geometry::{Point3, cube};
use egui::accesskit::Toggled;
use egui_kittest::kittest::{NodeT, Queryable};

use super::*;

fn points_of(p: &OpParams) -> (Option<Point2>, Option<Point2>) {
    match p {
        OpParams::Contour { links, .. } => (links.approach.start_point, links.approach.exit_point),
        _ => (None, None),
    }
}

fn live_points(st: &State) -> (Option<Point2>, Option<Point2>) {
    let op = model::find_by_name(&st.world, "Contour1").expect("seed has Contour1");
    model::operation_of(&st.world, &op).map(|o| points_of(&o.params)).unwrap_or((None, None))
}

fn first_cut_point(st: &State) -> [f64; 2] {
    let op = model::find_by_name(&st.world, "Contour1").expect("seed has Contour1");
    let tp = model::toolpath_of(&st.world, &op).expect("Contour1 has a toolpath");
    let m = tp
        .path
        .moves
        .iter()
        .find(|m| matches!(m.kind, camcraft_app::cl::MoveKind::Feed))
        .expect("Contour1 has a feed move");
    [m.to.x, m.to.y]
}

/// A visible off-center point on one of Block's vertical side walls (0..60 x
/// 0..60 x 0..30), returning its screen pixel and world XYZ.
fn visible_side_wall(sh: &ShellUi) -> (egui::Pos2, [f64; 3]) {
    let ([x0, y0], [x1, y1]) = sh.state.viewport_rect;
    let (w, hgt) = (x1 - x0, y1 - y0);
    let mvp = sh.state.cam.mvp(f64::from(w / hgt));
    let eye = sh.state.cam.eye();
    let candidates = [
        ([60.0, 18.0, 15.0], [1.0, 0.0, 0.0]),
        ([18.0, 0.0, 15.0], [0.0, -1.0, 0.0]),
        ([0.0, 42.0, 15.0], [-1.0, 0.0, 0.0]),
        ([42.0, 60.0, 15.0], [0.0, 1.0, 0.0]),
    ];
    for (pt, n) in candidates {
        let facing = (eye[0] - pt[0]) * n[0] + (eye[1] - pt[1]) * n[1] + (eye[2] - pt[2]) * n[2];
        if facing <= 0.0 {
            continue;
        }
        if let Some(uv) = interact::project_uv(&mvp, pt)
            && uv[0] > 0.05 && uv[0] < 0.95 && uv[1] > 0.05 && uv[1] < 0.95
        {
            return (egui::pos2(x0 + uv[0] * w, y0 + uv[1] * hgt), pt);
        }
    }
    panic!("no vertical wall of Block faces the default camera");
}

fn open_contour_leads() -> Harness<'static, ShellUi> {
    let mut harness = shell_harness();
    crate::actions::target(&mut harness.state_mut().state, UiTarget::Operation("Contour1".into()));
    harness.state_mut().state.op_tab = super::super::op_tabs::SEC_LEADS;
    harness.run_steps(2);
    harness
}

fn press(h: &mut Harness<'static, ShellUi>, label: &str) {
    h.get_by_label(label).scroll_to_me();
    h.run_steps(4);
    h.get_by_label(label).click();
    h.run_steps(3);
}

#[test]
fn picking_a_start_point_on_a_wall_regenerates_the_contour_and_undoes() {
    let mut harness = open_contour_leads();
    let initial_cut = first_cut_point(&harness.state().state);
    let (px, wall) = visible_side_wall(harness.state());

    press(&mut harness, "Pick start");
    assert!(
        harness.query_by_label_contains("Picking start point -").is_some(),
        "pick mode banner states what clicks do"
    );

    click_viewport(&mut harness, px);
    let draft = points_of(&harness.state().state.op_ui.params).0.expect("viewport click populated draft start_point");
    assert!(
        (draft.x - wall[0]).abs() < 0.5 && (draft.y - wall[1]).abs() < 0.5,
        "picked ({:.3}, {:.3}) should match wall ({:.3}, {:.3})",
        draft.x,
        draft.y,
        wall[0],
        wall[1]
    );
    assert_eq!(live_points(&harness.state().state).0, None, "viewport click edits draft only before Apply/Generate");
    assert!(harness.query_by_label("Start X").is_some() && harness.query_by_label("Start Y").is_some());

    harness.get_by_label("Generate").click();
    harness.run();
    harness.state_mut().state.drain_actions();
    harness.run();

    assert_eq!(live_points(&harness.state().state).0, Some(draft), "Generate committed the picked start point");
    assert_eq!(
        harness.get_by_label("Pick start").accesskit_node().toggled(),
        Some(Toggled::False),
        "Generate disarms the viewport pick"
    );
    let new_cut = first_cut_point(&harness.state().state);
    assert_ne!(new_cut, initial_cut, "regenerated path enters at the picked wall instead of the automatic start");

    // Two undo steps: GenerateOperation, then EditOperationParams.
    harness.state_mut().state.pending.push_back(CommandKind::Undo);
    harness.state_mut().state.pending.push_back(CommandKind::Undo);
    harness.run_steps(2);
    assert_eq!(live_points(&harness.state().state).0, None, "undo clears the picked start point");
    assert_eq!(first_cut_point(&harness.state().state), initial_cut, "undo restores the previous path");

    harness.state_mut().state.pending.push_back(CommandKind::Redo);
    harness.state_mut().state.pending.push_back(CommandKind::Redo);
    harness.run_steps(2);
    assert_eq!(live_points(&harness.state().state).0, Some(draft), "redo restores the picked start point");
    assert_eq!(first_cut_point(&harness.state().state), new_cut, "redo restores the regenerated path");
}

#[test]
fn picking_an_exit_point_on_a_wall_applies_and_clear_restores_automatic_exit() {
    let mut harness = open_contour_leads();
    let (px, wall) = visible_side_wall(harness.state());

    press(&mut harness, "Pick exit");
    click_viewport(&mut harness, px);

    let draft = points_of(&harness.state().state.op_ui.params).1.expect("viewport click populated draft exit_point");
    assert!((draft.x - wall[0]).abs() < 0.5 && (draft.y - wall[1]).abs() < 0.5);

    harness.get_by_label("Apply").click();
    harness.run();
    harness.state_mut().state.drain_actions();
    harness.run();
    assert_eq!(live_points(&harness.state().state).1, Some(draft));

    press(&mut harness, "Clear exit");
    assert_eq!(points_of(&harness.state().state.op_ui.params).1, None);
    harness.get_by_label("Apply").click();
    harness.run();
    harness.state_mut().state.drain_actions();
    assert_eq!(live_points(&harness.state().state).1, None);
}

#[test]
fn leaving_the_lead_tab_disarms_the_viewport_pick() {
    let mut harness = open_contour_leads();
    let (px, _) = visible_side_wall(harness.state());
    press(&mut harness, "Pick start");
    assert!(harness.query_by_label_contains("Picking start point -").is_some(), "armed before switching tab");
    harness.state_mut().state.op_tab = super::super::op_tabs::SEC_HEIGHTS;
    harness.run_steps(2);

    click_viewport(&mut harness, px);
    assert_eq!(points_of(&harness.state().state.op_ui.params).0, None, "leaving the Lead tab disarmed the pick");
    assert!(harness.state().state.status.contains("picking is off"), "{}", harness.state().state.status);

    harness.state_mut().state.op_tab = super::super::op_tabs::SEC_LEADS;
    harness.run_steps(2);
    assert_eq!(harness.state().state.op_ui.start_exit_pick, None, "returning to Lead tab stays disarmed");
}

#[test]
fn clicking_a_fixture_while_picking_start_point_is_refused_by_name() {
    let mut harness = open_contour_leads();
    {
        let st = &mut harness.state_mut().state;
        st.history.execute(
            &st.world,
            ImportModel::from_mesh("Clamp", cube(Point3::new(80.0, 10.0, 0.0), Point3::new(100.0, 50.0, 30.0))),
        );
        st.pending.push_back(CommandKind::TagFixture { name: "Clamp".into(), setup: None });
        st.drain_actions();
    }
    harness.run_steps(2);
    let ([x0, y0], [x1, y1]) = harness.state().state.viewport_rect;
    let (w, hgt) = (x1 - x0, y1 - y0);
    let mvp = harness.state().state.cam.mvp(f64::from(w / hgt));
    let uv = interact::project_uv(&mvp, [90.0, 30.0, 30.0]).expect("fixture top is on screen");
    let px = egui::pos2(x0 + uv[0] * w, y0 + uv[1] * hgt);

    press(&mut harness, "Pick start");
    click_viewport(&mut harness, px);
    assert_eq!(points_of(&harness.state().state.op_ui.params).0, None, "a fixture click sets no start point");
    assert!(harness.state().state.status.contains("fixture"), "{}", harness.state().state.status);
}
