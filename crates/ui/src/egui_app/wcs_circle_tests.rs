//! #739 in the surface: the WCS page's "Circle centre" anchor. Choosing it
//! shows "Pick a circular edge"; armed, a viewport click on a boss's rim
//! queues `SetWcsCircle`, the drain lands the origin on the rim's centre
//! (the app's shared `pick::circle_centre`), the page states the circle,
//! the pick disarms, and Ctrl+Z (the Undo command) restores the previous
//! origin. A face is refused by name and lands nothing.
//! Provenance: our own page law; no vendor material consulted.

use egui::accesskit::Role;
use egui_kittest::kittest::{NodeT, Queryable};

use camcraft_app::commands::ImportModel;
use camcraft_app::geometry::cylinder;
use camcraft_app::model;
use camcraft_app::operation::GeoSel;

use super::wcs_page_tests::page;
use crate::interact::{self, Mods};
use crate::state::{CommandKind, State};

/// A 20 mm boss, 10 tall, on screen in a 400 px viewport, with G54
/// placed at (1, 2, 3).
fn with_boss() -> State {
    let mut state = State::new();
    state.history.execute(&state.world, ImportModel::from_mesh("boss", cylinder(20.0, 10.0, 48)));
    state.viewport_rect = ([0.0, 0.0], [400.0, 400.0]);
    state.cam = crate::viewport::Camera { target: [0.0, 0.0, 5.0], yaw: 0.3, pitch: 0.4, dist: 90.0, ..Default::default() };
    state.pending.push_back(CommandKind::SetWcs { pos: [1.0, 2.0, 3.0], roll_deg: 0.0, pitch_deg: 0.0, yaw_deg: 0.0 });
    state.drain_actions();
    state
}

fn g54(state: &State) -> [f64; 3] {
    let e = model::find_by_name(&state.world, "G54").expect("G54 placed");
    let p = model::transform_of(&state.world, &e).expect("placed").pos;
    [p.x, p.y, p.z]
}

fn near(a: [f64; 3], b: [f64; 3]) -> bool {
    a.iter().zip(b).all(|(x, y)| (x - y).abs() < 1e-6)
}

/// The top rim's chord midpoint nearest the eye, in viewport px.
fn over_the_top_rim(state: &State) -> [f32; 2] {
    let e = state.cam.eye();
    let mid = (0..48)
        .map(|i| {
            let a = std::f64::consts::TAU * (f64::from(i) + 0.5) / 48.0;
            let r = 10.0 * (std::f64::consts::PI / 48.0).cos();
            [r * a.cos(), r * a.sin(), 10.0]
        })
        .min_by(|x, y| {
            let d = |q: &[f64; 3]| (q[0] - e[0]).powi(2) + (q[1] - e[1]).powi(2) + (q[2] - e[2]).powi(2);
            d(x).total_cmp(&d(y))
        })
        .expect("rim");
    let uv = interact::project_uv(&state.cam.mvp(1.0), mid).expect("on screen");
    [uv[0] * 400.0, uv[1] * 400.0]
}

#[test]
fn a_rim_click_anchors_the_origin_on_its_centre_and_undo_restores_it() {
    let mut h = page(with_boss());
    // Choose "Circle centre" from the Anchor on combo.
    h.get_by(|n| n.role() == Role::ComboBox && n.value().as_deref() == Some("Stock box")).click();
    h.run();
    h.get_by_label("Circle centre").click();
    h.run();
    assert_eq!(h.state().wcs_ui.anchor_source, 2);
    assert!(h.query_by_label("Pick markers in viewport").is_none(), "the box grid gives way to the circle pick");
    assert!(h.query_by_label("No circle picked yet.").is_some());
    let arm = h.get_by_label("Pick a circular edge");
    assert!(!arm.accesskit_node().is_disabled(), "the pick is live");
    arm.click();
    h.run();
    assert!(h.state().wcs_ui.pick_circle, "the button arms the edge pick");

    let p = over_the_top_rim(h.state());
    interact::pick_at(h.state_mut(), p, Mods::default());
    assert!(
        h.state().pending.iter().any(|c| matches!(c, CommandKind::SetWcsCircle { .. })),
        "the click queues the command ({})",
        h.state().status
    );
    h.run();
    assert!(near(g54(h.state()), [0.0, 0.0, 10.0]), "{:?} ({})", g54(h.state()), h.state().status);
    assert!(!h.state().wcs_ui.pick_circle, "a landed pick disarms");
    assert!(
        h.query_by_label_contains("Circle Ø 20.00 mm, centre (0.00, 0.00, 10.00)").is_some(),
        "the page states the circle: {}",
        super::wcs_page::circle_line(h.state())
    );

    h.state_mut().pending.push_back(CommandKind::Undo);
    h.run();
    assert!(near(g54(h.state()), [1.0, 2.0, 3.0]), "undo restores the previous origin: {:?}", g54(h.state()));
}

#[test]
fn a_face_is_refused_by_name_and_lands_nothing() {
    let mut state = with_boss();
    let g = camcraft_app::pick::graph_of(&state.world, "boss").expect("graph");
    let face = g.faces.iter().find_map(|f| f.sel).expect("a face key");
    crate::wcs_circle_pick::queue_circle(&mut state, GeoSel { entity: "boss".into(), feature: face });
    state.drain_actions();
    assert!(near(g54(&state), [1.0, 2.0, 3.0]));
    assert!(state.status.contains("not an edge"), "{}", state.status);
    assert!(super::wcs_page::circle_line(&state).starts_with("Not anchored:"), "the page says why");
    // A refusal lands nothing: the newest step is still the SetWcs that
    // created G54, so one undo removes it.
    assert!(state.history.undo(&state.world));
    assert!(model::find_by_name(&state.world, "G54").is_none(), "no circle step was recorded");
}
