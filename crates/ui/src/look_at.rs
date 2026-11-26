//! Look at (#2818): one click on a face turns the view to face it. The
//! Display menu arms it; the next left click in the viewport is the pick.
//! View state only: the camera turns about its target, nothing is selected
//! and no command is queued.
//! Provenance: our own code over our own pick ray and `Camera::toward`; the
//! face direction is the triangle normal (a cross product). No vendor
//! material.

use camcraft_app::{model, pick, setup_align};

use crate::state::State;

/// The status words: the tests read the same ones.
pub(crate) const ARMED: &str = "Look at: click a face in the view";
pub(crate) const OFF: &str = "Look at: off";
pub(crate) const DONE: &str = "Look at: the view faces the face";
pub(crate) const MISS: &str = "Look at: no face under the cursor - the view did not change";

/// Arm or disarm the one-click pick.
pub(crate) fn arm(state: &mut State, on: bool) {
    // One pick at a time (#2834): the plane pick gives way.
    state.look_at_plane_armed = false;
    state.look_at_armed = on;
    state.status = if on { ARMED } else { OFF }.to_owned();
}

/// The outward direction of the face under the logical-px point `p`: the
/// unit normal of the first visible triangle that the pick ray hits, turned
/// toward the eye. `None` on a miss or on a degenerate triangle.
#[must_use]
pub(crate) fn face_direction(state: &State, p: [f32; 2]) -> Option<[f64; 3]> {
    let (origin, dir) = crate::interact::ray_at(state, p)?;
    let (name, facet, _) = pick::first_hit_f(&state.world, origin, dir, |n| state.hidden_in_view(n))?;
    let entity = model::find_by_name(&state.world, &name)?;
    let tri = *model::facets_of(&state.world, &entity)?.get(facet)?;
    // One triangle through the shown pose, as picking and drawing do:
    // never a copy of the whole mesh for a click.
    let loc = setup_align::view_pose(&state.world, &entity).unwrap_or_default();
    let at = |q| camcraft_app::geometry::transform_point(q, loc.pos, loc.rot);
    let (a, b, c) = (at(tri.a), at(tri.b), at(tri.c));
    let (u, v) = ([b.x - a.x, b.y - a.y, b.z - a.z], [c.x - a.x, c.y - a.y, c.z - a.z]);
    let n = [u[1] * v[2] - u[2] * v[1], u[2] * v[0] - u[0] * v[2], u[0] * v[1] - u[1] * v[0]];
    let len = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
    if !len.is_finite() || len <= f64::EPSILON {
        return None;
    }
    // Toward the eye, whatever the winding of the triangle is.
    let facing = n[0] * dir[0] + n[1] * dir[1] + n[2] * dir[2];
    let s = if facing > 0.0 { -1.0 / len } else { 1.0 / len };
    Some([n[0] * s, n[1] * s, n[2] * s])
}

/// The armed click: turn the view to the face under `p`, or say that there
/// is none. One shot: the pick disarms in each case.
pub(crate) fn click(state: &mut State, p: [f32; 2]) {
    state.look_at_armed = false;
    match face_direction(state, p) {
        Some(d) => {
            crate::actions::orient_view(state, d);
            state.status = DONE.to_owned();
        }
        None => state.status = MISS.to_owned(),
    }
}
