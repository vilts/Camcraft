//! #3335: real viewport clicks fill the Boundaries "Do not touch" group of
//! 2D Offset, Trace and Finish3D operations. The click goes through the shell's own pointer route
//! (`interact::pick_at` -> `feature_click::click` -> `take_pick`), not a
//! direct `take_pick` call: a face click and an edge click each add one row,
//! the status line names the pick, and the row's remove button takes it
//! out. The same must hold when the toolbar's "Click selects" mode is
//! Models while the group is in "Picking…".
//! Provenance: our own pick-route test over our own shell; no vendor
//! material consulted.
//! Gate map (M4.4): covers ui::interact, ui::feature_click, ui::boundary_draft, ui::egui_app::boundary.

use camcraft_app::commands::{AddTool, ImportModel};
use camcraft_app::geometry::{Point3, cube};
use camcraft_app::operation::{self, FeatureSel, GeoSel, OpType};

use super::*;

const X0: f64 = 200.0;
const TOP: f64 = 15.0;

fn px_of(sh: &ShellUi, q: [f64; 3]) -> egui::Pos2 {
    let ([x0, y0], [x1, y1]) = sh.state.viewport_rect;
    let (w, hgt) = (x1 - x0, y1 - y0);
    let uv = interact::project_uv(&sh.state.cam.mvp(f64::from(w / hgt)), q).expect("on screen");
    egui::pos2(x0 + uv[0] * w, y0 + uv[1] * hgt)
}

/// A press and release at `p`, then the frames that drain what it queued.
fn click(h: &mut Harness<'static, ShellUi>, p: egui::Pos2) {
    h.input_mut().events.push(Event::PointerMoved(p));
    h.run();
    for pressed in [true, false] {
        h.input_mut().events.push(Event::PointerButton { pos: p, button: egui::PointerButton::Primary, pressed, modifiers: Modifiers::NONE });
        h.run();
    }
    h.run_steps(2);
}

fn dnt(h: &Harness<'static, ShellUi>) -> Vec<GeoSel> {
    h.state().state.op_ui.area.do_not_touch.picks.clone()
}

/// A block beside the seed's part, an operation on the stock open on its
/// Geometry tab, the Boundary group expanded and "Pick protected" armed,
/// the Top view in orthographic framing the block.
fn armed_operation(kind: OpType) -> Harness<'static, ShellUi> {
    let mut h = shell_harness();
    // Finish3D has more geometry controls above the protected rows. Trace
    // has the "Not taken" Start point and Exit point groups (#3324) above
    // them. Keep both rows and their remove buttons inside the test viewport.
    let height = if matches!(kind, OpType::Finish3D | OpType::Trace) { 1400.0 } else { 1100.0 };
    h.set_size(egui::vec2(1440.0, height));
    {
        let st = &mut h.state_mut().state;
        let block = cube(Point3::new(X0, 0.0, 0.0), Point3::new(X0 + 30.0, 30.0, TOP));
        st.history.execute(&st.world, ImportModel::from_mesh("guard", block));
        let seed = model::find_by_name(&st.world, "Contour1").and_then(|e| model::operation_of(&st.world, &e)).expect("seed op");
        let tool = if kind == OpType::Finish3D {
            let ball = camcraft_app::tools::catalog().into_iter()
                .find(|(name, _)| *name == "BN-0600").expect("catalog ball tool").1;
            st.history.execute(&st.world, AddTool::new(&st.world, "ProtectedBall", ball).expect("add ball tool"));
            "ProtectedBall".to_owned()
        } else {
            seed.tool.clone()
        };
        st.pending.push_back(CommandKind::CreateOperation {
            name: "Protected1".into(),
            tool,
            src: SrcRef::Stock,
            params: operation::retype_params(&seed.params, kind.ordinal()),
        });
        st.drain_actions();
        let saved = model::find_by_name(&st.world, "Protected1")
            .and_then(|e| model::operation_of(&st.world, &e)).expect("created protected operation");
        assert_eq!(saved.params.kind(), kind, "the fixture creates the requested operation kind");
        crate::actions::target(st, UiTarget::Operation("Protected1".into()));
    }
    h.run_steps(3);
    let boundary = h.get_by_label("Boundary").rect();
    click(&mut h, egui::pos2(boundary.left() - 10.0, boundary.center().y));
    h.get_by_label("Pick protected").scroll_to_me();
    h.run_steps(4);
    h.get_by_label("Pick protected").click();
    h.run_steps(2);
    assert_eq!(h.state().state.boundary_pick, Some(crate::state::BoundaryPick::DoNotTouch), "the group is armed");
    let st = &mut h.state_mut().state;
    let cam = crate::viewport::Camera::frame([X0 + 15.0, 15.0, TOP * 0.5], 60.0).toward([0.0, 0.0, 1.0]);
    st.cam = crate::viewport::Camera { projection: crate::viewport::ProjectionMode::Orthographic, ..cam };
    h.run_steps(2);
    h
}

/// A face click and an edge click each add one row; the status names the
/// pick; the remove button takes the last row out again.
fn face_then_edge(mut h: Harness<'static, ShellUi>) {
    let face = px_of(h.state(), [X0 + 15.0, 15.0, TOP]);
    click(&mut h, face);
    let one = dnt(&h);
    assert_eq!(one.len(), 1, "a face click adds one row: {one:?} / {}", h.state().state.status);
    assert!(matches!(one[0].feature, FeatureSel::Face(_)), "the row is a face: {one:?}");
    assert!(h.state().state.status.contains("added to Do not touch"), "the status names the pick: {}", h.state().state.status);
    let edge = px_of(h.state(), [X0 + 15.0, 0.0, TOP]);
    click(&mut h, edge);
    let two = dnt(&h);
    assert_eq!(two.len(), 2, "an edge click adds one row: {two:?} / {}", h.state().state.status);
    assert!(matches!(two[1].feature, FeatureSel::Chain(_)), "the second row is an edge: {two:?}");
    assert!(h.state().state.status.contains("added to Do not touch"), "the status names the pick: {}", h.state().state.status);
    h.run_steps(2);
    h.query_all_by_label("×").last().expect("a Do not touch remove button").scroll_to_me();
    h.run_steps(4);
    h.query_all_by_label("×").last().expect("a Do not touch remove button").click();
    h.run_steps(2);
    assert_eq!(dnt(&h), vec![one[0].clone()], "the remove button takes the edge row out");
}

#[test]
fn do_not_touch_takes_a_face_and_an_edge_by_viewport_click() {
    face_then_edge(armed_operation(OpType::Offset2D));
}

/// The toolbar's "Click selects" set to Models while the group shows
/// "Picking…": the click still fills the group, not the model selection.
#[test]
fn do_not_touch_takes_viewport_clicks_in_models_mode() {
    let mut h = armed_operation(OpType::Offset2D);
    h.state_mut().state.geo_pick = GeoPick::Model;
    h.run_steps(2);
    assert_eq!(h.state().state.boundary_pick, Some(crate::state::BoundaryPick::DoNotTouch), "the group stays armed");
    face_then_edge(h);
}

#[test]
fn trace_takes_a_protected_face_and_edge_by_viewport_click() {
    face_then_edge(armed_operation(OpType::Trace));
}

#[test]
fn finish3d_takes_a_protected_face_and_edge_by_viewport_click() {
    face_then_edge(armed_operation(OpType::Finish3D));
}
