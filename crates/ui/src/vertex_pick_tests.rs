//! M13.4 law, on the one pick rule (#692 step 2): a Vertex-mode pick on a
//! cube seen down its body diagonal - the front corner and the hidden back
//! corner project onto the same pixel, and only the front one may win; a
//! corner hidden behind the part is never picked, in perspective or
//! orthographic, unless the app's explicit pick-through is asked for.
//!
//! Provenance: our own test, standard picking geometry.

use camcraft_app::commands::ImportModel;
use camcraft_app::geometry::{Point3, cube};
use camcraft_app::pick::{self, EdgeUnit, PickKinds, PickTarget};

use crate::feature_click::{Ask, feature_at, query_at};
use crate::state::State;
use crate::viewport::{Camera, ProjectionMode};

const CENTER: [f32; 2] = [200.0, 200.0];
const POINTS: Ask = Ask { kinds: PickKinds::POINTS, unit: EdgeUnit::Chain };

/// Looking from +X+Y+Z at the origin: (10,10,10) faces the eye and
/// (-10,-10,-10) sits straight behind it.
fn diagonal_cam() -> Camera {
    Camera {
        target: [0.0, 0.0, 0.0],
        yaw: std::f64::consts::FRAC_PI_4,
        pitch: (1.0f64 / 2.0f64.sqrt()).atan(),
        dist: 100.0,
        ..Camera::default()
    }
}

/// A 20 mm cube centred on the origin, the only thing in the view.
fn cube_state(projection: ProjectionMode) -> State {
    let mut state = State::new();
    state.history.execute(
        &state.world,
        ImportModel::from_mesh("c", cube(Point3::new(-10.0, -10.0, -10.0), Point3::new(10.0, 10.0, 10.0))),
    );
    state.viewport_rect = ([0.0, 0.0], [400.0, 400.0]);
    state.cam = Camera { projection, ..diagonal_cam() };
    state
}

/// The picked point's world position (`None` for no pick or no point).
fn point_at(state: &State, p: [f32; 2]) -> Option<[f64; 3]> {
    let (pick, _) = feature_at(state, p, POINTS)?;
    matches!(pick.target, PickTarget::Point(_)).then_some(pick.at)
}

fn near(a: [f64; 3], b: [f64; 3]) -> bool {
    (a[0] - b[0]).abs() + (a[1] - b[1]).abs() + (a[2] - b[2]).abs() < 1e-6
}

/// Where world point `q` draws, logical px.
fn px(state: &State, q: [f64; 3]) -> [f32; 2] {
    let uv = crate::interact::project_uv(&state.cam.mvp(1.0), q).expect("on screen");
    [uv[0] * 400.0, uv[1] * 400.0]
}

/// The back corner, tilted off the front one, and checked hidden: the
/// view pick finds nothing (or another corner) there, the explicit
/// pick-through finds it.
fn back_corner_is_never_picked(mut state: State) {
    state.cam.yaw += 0.25;
    let back = [-10.0, -10.0, -10.0];
    let at = px(&state, back);
    assert!(point_at(&state, at).is_none_or(|w| !near(w, back)), "a corner behind the part is never picked");
    let mut q = query_at(&state, at, POINTS).expect("a query");
    q.through = true;
    let through = pick::pick_feature(&state.world, &q, |_| false).expect("pick-through finds a corner");
    assert!(near(through.at, back), "proof the view pick refused on depth, not distance: {through:?}");
}

#[test]
fn nearest_visible_corner_wins_and_the_hidden_back_corner_never_does() {
    let state = cube_state(ProjectionMode::Perspective);
    // Both project to the image centre; the occlusion ray decides.
    let front = point_at(&state, CENTER).expect("the front corner");
    assert!(near(front, [10.0, 10.0, 10.0]), "{front:?}");
    // Far from every corner: nothing within the radius.
    assert_eq!(point_at(&state, [5.0, 5.0]), None);
    back_corner_is_never_picked(state);
}

#[test]
fn degenerate_inputs_pick_nothing() {
    let mut state = cube_state(ProjectionMode::Perspective);
    assert!(point_at(&state, [f32::NAN, 200.0]).is_none(), "NaN cursor");
    state.viewport_rect = ([0.0, 0.0], [0.0, 0.0]);
    assert!(point_at(&state, CENTER).is_none(), "no image rect");
    let mut empty = State::new();
    empty.viewport_rect = ([0.0, 0.0], [400.0, 400.0]);
    empty.cam = diagonal_cam();
    assert!(point_at(&empty, CENTER).is_none(), "nothing in the view");
    let mut nan = cube_state(ProjectionMode::Perspective);
    nan.cam.dist = f64::NAN;
    assert!(point_at(&nan, CENTER).is_none(), "a NaN camera");
}

#[test]
fn orthographic_picks_are_visible_first_too() {
    // #610: the ortho occlusion ray runs back along the parallel view
    // direction (the app's rule); the corner in front still wins and the
    // one behind is still never picked.
    let state = cube_state(ProjectionMode::Orthographic);
    let front = point_at(&state, CENTER).expect("the front corner");
    assert!(near(front, [10.0, 10.0, 10.0]), "{front:?}");
    back_corner_is_never_picked(state);
}

/// #692: round-edge centres are opt-in - the plain pick modes, the plane
/// row and an axis by edge never snap to one; only the point takers that
/// read a centre (three points, an axis by two points) ask for them.
#[test]
fn only_the_point_takers_that_read_a_centre_ask_for_centres() {
    use crate::axis_pick::AxisBy;
    use crate::feature_click::ask;
    use crate::interact::Mods;
    use crate::state::GeoPick;
    use camcraft_app::pick::InPlaneAxis;

    let mut state = cube_state(ProjectionMode::Perspective);
    for mode in [GeoPick::Vertex, GeoPick::Edge, GeoPick::Face, GeoPick::Model] {
        state.geo_pick = mode;
        assert!(!ask(&state, Mods::default()).kinds.centres, "{mode:?}: the pick mode alone never asks");
    }
    crate::points_pick::arm(&mut state, "Setup1");
    let k = ask(&state, Mods::default()).kinds;
    assert!(k.points && k.centres && !k.edges, "three points opt in: {k:?}");
    crate::points_pick::disarm(&mut state);
    crate::axis_pick::set_by(&mut state, "Setup1", InPlaneAxis::Y, AxisBy::Points);
    let k = ask(&state, Mods::default()).kinds;
    assert!(k.points && k.centres, "an axis by two points opts in: {k:?}");
    crate::axis_pick::set_by(&mut state, "Setup1", InPlaneAxis::Y, AxisBy::Edge);
    let k = ask(&state, Mods::default()).kinds;
    assert!(k.edges && !k.centres, "an axis by edge never snaps to a centre: {k:?}");
}
