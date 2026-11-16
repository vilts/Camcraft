//! #1502 in the surface: with the WCS page's "Pick markers in viewport"
//! armed, a click on one of the part's own corners or on a hole/boss
//! rim's centre anchors the origin there, through the undoable `SetWcs`
//! / `SetWcsCircle` commands; a click on bare space stays with the pick
//! mode. The nine box markers keep winning where they sit.
//! Provenance: our own page law; no vendor material consulted.

use camcraft_app::commands::ImportModel;
use camcraft_app::geometry::{Point3, cube, cylinder};
use camcraft_app::model;

use crate::interact::{self, Mods};
use crate::state::{CommandKind, State};

fn scene(mesh: camcraft_app::geometry::Mesh, target: [f64; 3]) -> State {
    let mut state = State::new();
    state.history.execute(&state.world, ImportModel::from_mesh("part", mesh));
    state.viewport_rect = ([0.0, 0.0], [400.0, 400.0]);
    state.cam = crate::viewport::Camera { target, yaw: 0.3, pitch: 0.9, dist: 120.0, ..Default::default() };
    state.pending.push_back(CommandKind::SetWcs { pos: [1.0, 2.0, 3.0], roll_deg: 0.0, pitch_deg: 0.0, yaw_deg: 0.0 });
    state.drain_actions();
    state.wcs_ui.anchor_z = 0; // markers on the bottom level, clear of the top features
    state.wcs_ui.pick_anchor = true;
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

fn px(state: &State, at: [f64; 3]) -> [f32; 2] {
    let uv = interact::project_uv(&state.cam.mvp(1.0), at).expect("on screen");
    [uv[0] * 400.0, uv[1] * 400.0]
}

#[test]
fn a_click_on_a_part_corner_anchors_the_origin_there_and_undo_restores_it() {
    let mut state = scene(cube(Point3::new(0.0, 0.0, 0.0), Point3::new(20.0, 20.0, 40.0)), [10.0, 10.0, 20.0]);
    let corner = [20.0, 20.0, 40.0]; // top corner: no marker sits on the bottom level here
    let p = px(&state, corner);
    assert!(interact::anchor_hit(&state, p, 12.0).is_none(), "a bare corner, no marker under it");
    interact::pick_at(&mut state, p, Mods::default());
    assert!(
        state.pending.iter().any(|c| matches!(c, CommandKind::SetWcs { .. })),
        "the click queues the origin ({})",
        state.status
    );
    state.drain_actions();
    assert!(near(g54(&state), corner), "{:?} ({})", g54(&state), state.status);
    state.pending.push_back(CommandKind::Undo);
    state.drain_actions();
    assert!(near(g54(&state), [1.0, 2.0, 3.0]), "undo restores: {:?}", g54(&state));
}

#[test]
fn a_click_on_a_boss_rim_centre_anchors_the_origin_on_it() {
    let mut state = scene(cylinder(20.0, 40.0, 48), [0.0, 0.0, 20.0]);
    let centre = [0.0, 0.0, 40.0];
    let p = px(&state, centre);
    assert!(interact::anchor_hit(&state, p, 12.0).is_none(), "no marker under the top centre");
    interact::pick_at(&mut state, p, Mods::default());
    assert!(
        state.pending.iter().any(|c| matches!(c, CommandKind::SetWcsCircle { .. })),
        "the click queues the centre ({})",
        state.status
    );
    state.drain_actions();
    assert!(near(g54(&state), centre), "{:?} ({})", g54(&state), state.status);
}

#[test]
fn with_the_markers_off_a_corner_click_does_not_move_the_origin() {
    let mut state = scene(cube(Point3::new(0.0, 0.0, 0.0), Point3::new(20.0, 20.0, 40.0)), [10.0, 10.0, 20.0]);
    state.wcs_ui.pick_anchor = false;
    let p = px(&state, [20.0, 20.0, 40.0]);
    interact::pick_at(&mut state, p, Mods::default());
    state.drain_actions();
    assert!(near(g54(&state), [1.0, 2.0, 3.0]), "{:?}", g54(&state));
}

#[test]
fn a_vertex_that_no_longer_resolves_is_refused_by_name_and_queues_nothing() {
    let mut state = scene(cube(Point3::new(0.0, 0.0, 0.0), Point3::new(20.0, 20.0, 40.0)), [10.0, 10.0, 20.0]);
    let gone = camcraft_app::operation::GeoSel {
        entity: "part".into(),
        feature: camcraft_app::operation::FeatureSel::Vertex(u64::MAX),
    };
    crate::wcs_circle_pick::queue_vertex(&mut state, &gone);
    assert!(state.pending.is_empty(), "nothing queued");
    assert!(state.status.starts_with("origin not placed:"), "{}", state.status);
    assert!(near(g54(&state), [1.0, 2.0, 3.0]));
}
