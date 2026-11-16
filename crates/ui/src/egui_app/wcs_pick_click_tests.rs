//! #1562: the Setups page's "Pick for WCS" row from a fresh project. The
//! toolbar's pick mode is the default Model and no operation is open; the
//! row is armed by its button and every pick is driven through the
//! viewport's own click and hover (`interact::pick_at` /
//! `interact::update_hover`), never `wcs_pick::take`. A face, an edge and
//! a corner each land as the role in the box, the hover lights the
//! feature under the cursor, Apply places the WCS through the undoable
//! `SetWcs`, and stopping the row gives the whole-model click back.
//! Provenance: our own page law; no vendor material consulted.

use egui::Vec2;
use egui::accesskit::Role as ARole;
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;

use camcraft_app::commands::ImportModel;
use camcraft_app::geometry::{Point3, cube};
use camcraft_app::model;
use camcraft_app::operation::FeatureSel;
use camcraft_app::pick::Role;

use crate::interact::{self, Mods};
use crate::state::{CommandKind, GeoPick, State};

/// A 20 x 20 x 10 block on screen in a 400 px viewport, one setup, the
/// default pick mode.
fn fresh() -> State {
    let mut state = State::new();
    let part = ImportModel::from_mesh("part", cube(Point3::new(0.0, 0.0, 0.0), Point3::new(20.0, 20.0, 10.0)));
    state.history.execute(&state.world, part);
    state.pending.push_back(CommandKind::CreateSetup { name: None, datum_from: None });
    state.drain_actions();
    state.viewport_rect = ([0.0, 0.0], [400.0, 400.0]);
    state.cam = crate::viewport::Camera { target: [10.0, 10.0, 5.0], yaw: 0.3, pitch: 0.5, dist: 90.0, ..Default::default() };
    state
}

fn frame(ui: &mut egui::Ui, st: &mut State) {
    super::panes::shortcuts(ui, st);
    st.drain_actions();
    super::wcs_pick_row::row(ui, st, "Setup1");
}

fn px(state: &State, p: [f64; 3]) -> [f32; 2] {
    let uv = interact::project_uv(&state.cam.mvp(1.0), p).expect("on screen");
    [uv[0] * 400.0, uv[1] * 400.0]
}

/// The top corner nearest the eye, and its neighbour along x: their edge
/// faces the camera, so the corner and the edge's midpoint are visible.
fn front_top(state: &State) -> ([f64; 3], [f64; 3]) {
    let e = state.cam.eye();
    assert!(e[2] > 10.0, "the camera looks down on the top face: {e:?}");
    let x = if e[0] < 10.0 { 0.0 } else { 20.0 };
    let y = if e[1] < 10.0 { 0.0 } else { 20.0 };
    ([x, y, 10.0], [20.0 - x, y, 10.0])
}

fn set_role(h: &mut Harness<'static, State>, role: Role) {
    h.state_mut().wcs_pick.as_mut().expect("armed").role = role;
}

fn pick_of(state: &State, role: Role) -> Option<FeatureSel> {
    let d = state.wcs_pick.as_ref()?;
    d.picks.iter().find(|(_, r)| *r == role).map(|(g, _)| g.feature)
}

#[test]
fn an_armed_row_takes_viewport_picks_from_the_default_model_mode() {
    let mut h = Harness::builder().with_size(Vec2::new(900.0, 300.0)).build_ui_state(frame, fresh());
    h.run();
    assert_eq!(h.state().geo_pick, GeoPick::Model, "a fresh project picks whole models");
    h.get_by_label("Pick for WCS").click();
    h.run();
    assert!(crate::wcs_pick::collecting(h.state()), "the row is armed");
    assert_eq!(h.state().geo_pick, GeoPick::Model, "arming keeps the toolbar's mode");

    // The hover lights the face the click would take, not the model.
    let top = px(h.state(), [10.0, 10.0, 10.0]);
    h.state_mut().viewport_hot = true;
    interact::update_hover(h.state_mut(), top, false);
    let lit = h.state().hover.as_ref().and_then(|hh| hh.feature);
    assert!(matches!(lit, Some(FeatureSel::Face(_))), "the hover names the top face: {lit:?}");

    // Z: the top face (the box's default role).
    interact::pick_at(h.state_mut(), top, Mods::default());
    assert!(matches!(pick_of(h.state(), Role::Z), Some(FeatureSel::Face(_))), "{}", h.state().status);
    assert!(h.state().status.contains("picked as Z"), "{}", h.state().status);
    // X: the front top edge.
    let (corner, along) = front_top(h.state());
    let mid = [(corner[0] + along[0]) / 2.0, corner[1], 10.0];
    set_role(&mut h, Role::X);
    let at = px(h.state(), mid);
    interact::pick_at(h.state_mut(), at, Mods::default());
    assert!(matches!(pick_of(h.state(), Role::X), Some(FeatureSel::Chain(_))), "{}", h.state().status);
    // Origin: the corner.
    set_role(&mut h, Role::Origin);
    let at = px(h.state(), corner);
    interact::pick_at(h.state_mut(), at, Mods::default());
    assert!(matches!(pick_of(h.state(), Role::Origin), Some(FeatureSel::Vertex(_))), "{}", h.state().status);
    assert!(
        !h.state().pending.iter().any(|c| matches!(c, CommandKind::Select(_))),
        "role picks never select the whole model"
    );
    h.run();
    assert!(h.query_by_label_contains("Picked:").is_some(), "the row lists the picks");

    // Apply places the WCS on the picked corner.
    h.get_by(|n| n.role() == ARole::Button && n.label().as_deref() == Some("Apply")).click();
    h.run();
    h.run();
    let e = camcraft_app::setup::wcs_entity(&h.state().world, Some("Setup1")).expect("the WCS is placed");
    let pos = model::transform_of(&h.state().world, &e).expect("placed").pos;
    let pos = [pos.x, pos.y, pos.z];
    assert!(pos.iter().zip(corner).all(|(a, b)| (a - b).abs() < 1e-9), "on the corner {corner:?}: {pos:?}");
    assert!(h.state().wcs_pick.is_none(), "a landed alignment ends the draft");

    // Disarmed, a click is a whole-model select again.
    interact::pick_at(h.state_mut(), top, Mods::default());
    assert!(
        h.state().pending.iter().any(|c| matches!(c, CommandKind::Select(Some(n)) if n == "part")),
        "the previous click behaviour is back: {}",
        h.state().status
    );
}
