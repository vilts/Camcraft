//! Vertex picks (M13.4): where a picked feature point sits, and the
//! work-origin moves a picked vertex or face drives (the WCS chips). The
//! pick itself is the app's one rule (#692 step 2, `crate::feature_click`):
//! a feature point - a corner, a sharp turn, an open edge's end - visible
//! under the cursor; never a vertex inside a face or along a round rim.
//!
//! Provenance: our own code over the app's pick and origin targets; the
//! screen marker is plain viewport projection. No vendor material
//! consulted.

use camcraft_app::geometry::feature::{self, VertexId};
use camcraft_app::geometry::{Point3, transform_point};
use camcraft_app::operation::FeatureSel;
use camcraft_app::{model, pick};

use crate::interact::project_uv;

use crate::state::{CommandKind, State};

/// The world position of a picked vertex, if it still resolves.
pub(crate) fn vertex_world(state: &State, entity: &str, id: VertexId) -> Option<[f64; 3]> {
    let found = model::find_by_name(&state.world, entity)?;
    let features = model::features_of(&state.world, &found)?;
    let vi = feature::resolve_vertex(&features, id)?;
    let v = features.verts.get(vi as usize)?;
    let loc = camcraft_app::setup_align::view_pose(&state.world, &found).unwrap_or_default(); // #326
    let w = transform_point(Point3::new(v[0], v[1], v[2]), loc.pos, loc.rot);
    Some([w.x, w.y, w.z])
}

/// Queue the work origin onto a picked vertex: the WCS pane's angles, the vertex's world position, through the one `SetWcs` command
/// the anchor grid also queues (undoable like any origin move).
pub fn queue_origin_at_vertex(state: &mut State, entity: &str, id: VertexId) {
    let what = camcraft_app::pick::geo_label(
        &state.world,
        &camcraft_app::operation::GeoSel { entity: entity.to_owned(), feature: FeatureSel::Vertex(id) },
    );
    let Some(pos) = vertex_world(state, entity, id) else {
        state.status = format!("{what} is no longer in the model");
        return;
    };
    state.status = format!("WCS origin set at {what}");
    state.pending.push_back(CommandKind::SetWcs {
        pos,
        roll_deg: state.wcs_ui.roll_deg,
        pitch_deg: state.wcs_ui.pitch_deg,
        yaw_deg: state.wcs_ui.yaw_deg,
    });
}

/// Queue the work origin onto a picked face (M13.4): its area-weighted
/// centroid, or - `z_only`, horizontal planar faces only - the pane's
/// current XY at the face's plane height. Same `SetWcs` command, same
/// angles as the vertex chip; the targets come from the app.
pub fn queue_origin_at_face(state: &mut State, entity: &str, sel: FeatureSel, z_only: bool) {
    let target = match pick::face_origin(&state.world, entity, sel.clone()) {
        Ok(t) => t,
        Err(why) => {
            state.status = format!("origin not set: {why}");
            return;
        }
    };
    let pos = if z_only {
        let Some(z) = target.plane_z else {
            state.status = String::from("origin not set: Z only needs a horizontal planar face");
            return;
        };
        // The WCS's placed origin is "the current XY"; an unplaced WCS
        // falls back to the pane's draft numbers.
        let active = camcraft_app::setup::active(&state.world);
        let placed = camcraft_app::setup::wcs_entity(&state.world, active.as_deref())
            .and_then(|e| model::transform_of(&state.world, &e));
        match placed {
            Some(loc) => [loc.pos.x, loc.pos.y, z],
            None => [state.wcs_ui.x, state.wcs_ui.y, z],
        }
    } else {
        target.centroid
    };
    let what = camcraft_app::pick::geo_label(
        &state.world,
        &camcraft_app::operation::GeoSel { entity: entity.to_owned(), feature: sel },
    );
    state.status = format!("WCS origin set on {what}");
    state.pending.push_back(CommandKind::SetWcs {
        pos,
        roll_deg: state.wcs_ui.roll_deg,
        pitch_deg: state.wcs_ui.pitch_deg,
        yaw_deg: state.wcs_ui.yaw_deg,
    });
}

/// Queue the 3-2-1 alignment (#187) of the open operation's picks onto the
/// active setup's WCS: position and angles both from the app's frame, through
/// the same undoable `SetWcs` command as the origin chips.
/// Result of 3-2-1 WCS alignment from the active operation's picks, refusing
/// any face tilt on a mill setup where the tool axis stays machine +Z (#760).
pub fn wcs_321_checked(state: &State) -> Result<camcraft_app::pick::Alignment, String> {
    let aligned = camcraft_app::pick::align_321(&state.world, &state.op_ui.geo)?;
    let active = camcraft_app::setup::active(&state.world);
    if let Some(why) =
        camcraft_app::setup_wcs_tilt::align_321_tilt_problem(&state.world, active.as_deref(), aligned.z)
    {
        return Err(why);
    }
    Ok(aligned)
}

pub fn queue_wcs_321(state: &mut State) {
    let a = match wcs_321_checked(state) {
        Ok(a) => a,
        Err(why) => {
            state.status = format!("WCS not aligned: {why}");
            return;
        }
    };
    state.status = String::from("WCS aligned 3-2-1 to the picked face, edge and origin");
    state.pending.push_back(CommandKind::SetWcs {
        pos: a.pos,
        roll_deg: a.roll_deg,
        pitch_deg: a.pitch_deg,
        yaw_deg: a.yaw_deg,
    });
}

/// #709 hand check: every shown picked point's screen diamond (see
/// [`vertex_marker`]), drawn violet over the image. The 3D cross of
/// `interact::selection_feature_lines` is a ~1 mm, depth-tested 1 px
/// line star half sunk in the corner it marks: at a framed part it
/// spans ~3 px and z-fights the faces and wire it lies on, so a picked
/// corner could vanish. This mark is never occluded and keeps its size
/// at any zoom. Hidden models and stale ids draw nothing.
pub(crate) fn picked_point_marks(state: &State, aspect: f64) -> Vec<([f32; 2], [f32; 2])> {
    let mut out = Vec::new();
    for g in crate::interact::shown_picks(state) {
        if matches!(g.feature, FeatureSel::Vertex(_)) {
            crate::interact::feature_segments(state, &g.entity, g.feature, aspect, &mut out);
        }
    }
    out
}

/// Screen-space marker for a vertex: a small diamond around its
/// projection, as viewport-uv segment pairs like `feature_segments`.
pub(crate) fn vertex_marker(
    mvp: &[[f32; 4]; 4],
    world: [f64; 3],
    aspect: f64,
    out: &mut Vec<([f32; 2], [f32; 2])>,
) {
    let Some(c) = project_uv(mvp, world) else {
        return;
    };
    let r = 0.008_f32; // of the image height
    let rx = r / (aspect.max(1e-3) as f32);
    let pts = [[c[0] + rx, c[1]], [c[0], c[1] + r], [c[0] - rx, c[1]], [c[0], c[1] - r]];
    for i in 0..4 {
        out.push((pts[i], pts[(i + 1) % 4]));
    }
}
