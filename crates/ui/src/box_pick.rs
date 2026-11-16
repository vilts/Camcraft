//! The viewport box select (#979): Ctrl (or Shift) + left drag in Edge or
//! Face mode, with an operation open, draws a rubber band instead of
//! orbiting; its release adds every visible feature edge chain (Edge
//! mode) or welded face (Face mode) in the box - hidden ones too while
//! Pick through is on - to the operation's pick
//! list as ONE `SetOperationGeometry` - one undo step. A drag to the
//! right is a window (wholly inside), to the left a crossing box (inside
//! or touching). The geometry rule is the app's (`pick::box_select`, the
//! #692 one pick rule: feature chains only, nothing hidden unless Pick
//! through is on); this module
//! only arms the gesture and routes the answer.
//!
//! Provenance: rubber-band window/crossing selection as public CAD user
//! documentation describes it; our own routing over our own pick API. No
//! vendor material consulted.

use camcraft_app::operation::{GeoSel, SrcRef};
use camcraft_app::pick::{self, BoxMode, BoxQuery, PickKinds, PickRay, Projection};
use camcraft_app::model;

use crate::interact::{Mods, project_uv, ray_at};
use crate::state::{CommandKind, GeoPick, State};
use crate::viewport::ProjectionMode;

/// Does a left drag with these modifiers draw a pick box rather than
/// orbit? Only when it would add operation geometry: Ctrl or Shift held
/// (the add-to-selection keys), Edge or Face mode, an operation open, and
/// no other pick draft (boundary, locate, plane, axis, points) collecting.
pub(crate) fn armed(state: &State, mods: Mods) -> bool {
    (mods.command || mods.shift)
        && matches!(state.geo_pick, GeoPick::Edge | GeoPick::Face)
        && !state.op_ui.name.is_empty()
        && !crate::interact::op_pick_locked(state)
        && state.boundary_pick.is_none()
        && state.locate_pick.is_none()
        && state.points_pick.is_none()
        && !crate::plane_pick::collecting(state)
        && !crate::axis_pick::collecting(state)
}

/// Window or crossing, by the drag's direction.
pub(crate) fn mode_of(b: [[f32; 2]; 2]) -> BoxMode {
    BoxMode::of_drag(f64::from(b[0][0]), f64::from(b[1][0]))
}

/// What the box takes in the viewport: the visible feature chains (Edge
/// mode) or welded faces (Face mode) in `b`, logical px - the hidden ones
/// too while `state.pick_through` is on.
pub(crate) fn picks_in(state: &State, b: [[f32; 2]; 2]) -> Vec<GeoSel> {
    let ([x0, y0], [x1, y1]) = state.viewport_rect;
    let (w, h) = (x1 - x0, y1 - y0);
    if w <= 1.0 || h <= 1.0 {
        return Vec::new();
    }
    let kinds = match state.geo_pick {
        GeoPick::Face => PickKinds::FACES,
        _ => PickKinds::EDGES,
    };
    // The sight line the occlusion test runs back along: the eye in
    // perspective, the view direction in orthographic.
    let mid = [(b[0][0] + b[1][0]) * 0.5, (b[0][1] + b[1][1]) * 0.5];
    let Some((origin, dir)) = ray_at(state, mid) else { return Vec::new() };
    let projection = match state.cam.projection {
        ProjectionMode::Orthographic => Projection::Ortho,
        ProjectionMode::Perspective => Projection::Perspective,
    };
    let mvp = state.cam.mvp(f64::from(w / h));
    let project = |p: [f64; 3]| {
        project_uv(&mvp, p).map(|uv| [f64::from(x0 + uv[0] * w), f64::from(y0 + uv[1] * h)])
    };
    let corners = b.map(|p| [f64::from(p[0]), f64::from(p[1])]);
    let q = BoxQuery {
        project: &project,
        corners,
        mode: mode_of(b),
        kinds,
        view: PickRay { origin, dir, projection },
        // The viewport's Pick through toggle, as a click reads it
        // (`feature_click`): a click and a box agree on hidden features.
        through: state.pick_through,
    };
    pick::box_select(&state.world, &q, |name| state.hidden_in_view(name))
}

/// The release of a pick box: add what it holds to the open operation's
/// picks, as one command. Says what it did in the status, a miss too.
pub(crate) fn finish(state: &mut State, b: [[f32; 2]; 2]) {
    let name = state.op_ui.name.clone();
    if name.is_empty() {
        state.status = String::from("open an operation first - geometry picks fill the open one");
        return;
    }
    let what = if state.geo_pick == GeoPick::Face { "face" } else { "edge chain" };
    let how = match mode_of(b) {
        BoxMode::Window => "inside",
        BoxMode::Crossing => "touching",
    };
    let found = picks_in(state, b);
    if found.is_empty() {
        let seen = if state.pick_through { "" } else { "visible " };
        state.status = format!("no {seen}{what} {how} the box");
        return;
    }
    let mut items: Vec<GeoSel> = model::find_by_name(&state.world, &name)
        .and_then(|e| model::operation_of(&state.world, &e))
        .map_or(Vec::new(), |op| match op.src {
            SrcRef::Selection(items) => items,
            _ => Vec::new(),
        });
    let before = items.len();
    for g in found {
        if !items.contains(&g) {
            items.push(g);
        }
    }
    let added = items.len() - before;
    if added == 0 {
        state.status = format!("every {what} {how} the box is already selected in {name}");
        return;
    }
    state.status = format!("{added} {what}(s) {how} the box added to {name}");
    state.pending.push_back(CommandKind::SetOperationGeometry { name, items });
}

/// Paint the rubber band: a window box solid with a light veil, a
/// crossing box dashed - the two read apart at a glance.
pub(crate) fn paint(painter: &egui::Painter, b: [[f32; 2]; 2], ink: egui::Color32) {
    let r = egui::Rect::from_two_pos(egui::pos2(b[0][0], b[0][1]), egui::pos2(b[1][0], b[1][1]));
    let stroke = egui::Stroke::new(1.0, ink);
    match mode_of(b) {
        BoxMode::Window => {
            painter.rect_filled(r, 0.0, ink.gamma_multiply(0.12));
            painter.rect_stroke(r, 0.0, stroke, egui::StrokeKind::Inside);
        }
        BoxMode::Crossing => {
            let c = [r.left_top(), r.right_top(), r.right_bottom(), r.left_bottom(), r.left_top()];
            painter.extend(egui::Shape::dashed_line(&c, stroke, 5.0, 4.0));
        }
    }
}
