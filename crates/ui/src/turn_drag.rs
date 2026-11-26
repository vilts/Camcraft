//! Drag-to-rotate rings on a selected body (#2047, #1198 slice c): with the
//! bar's Rotate mode on, the one selected part wears three rings, one in the
//! plane square to each axis, round its bounding-box centre. Pressing a ring
//! and dragging turns the body about that axis through the centre. The drag
//! is a preview (view state, never undoable); the release queues ONE
//! `CommandKind::TurnModelBy`, which the drain turns into the same
//! `MoveModel` the Model page's rotation fields run - so staleness, the
//! stock that follows the part and the fixture-clash warning behave exactly
//! as for a typed turn. Escape cancels with no history step; a drag that
//! ends at its start angle adds none.
//!
//! Provenance: our own code. The angle is the cursor ray's hit on the
//! ring's plane, measured about the centre (ray-plane intersection and
//! `atan2`, any vector geometry text); the rotation about a point is the
//! rigid-transform composition T(c) R T(-c). The constant-on-screen ring is
//! the usual CAD gizmo convention. No vendor material consulted.

use camcraft_app::geometry::{Point3, quat_from_axis_angle, quat_rotate_point};
use camcraft_app::schema::Location;
use camcraft_app::setup_align;
use flecs_ecs::prelude::Entity;

use crate::interact::{project_uv, ray_at};
use crate::move_drag::{self, aspect, one_part, px_per_mm, shown_center, to_px};
use crate::state::{CommandKind, State};

/// The ring radius on screen, logical px, whatever the zoom.
pub(crate) const RING_PX: f64 = 60.0;
/// How near (px) the pointer must be to a ring to grab it.
pub(crate) const RING_HIT_PX: f32 = 6.0;
/// Segments of a drawn (and hit-tested) ring.
pub(crate) const RING_SEGS: usize = 64;
/// Below this (radians) a release is "no turn" and queues nothing - a
/// press and release in place writes no rotation noise (#2177's law).
pub(crate) const MIN_TURN_RAD: f64 = 1e-9;

const AXES: [[f64; 3]; 3] = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];

/// A live ring drag - view state like the camera's orbit, never saved,
/// never in history.
#[derive(Clone, Debug, PartialEq)]
pub struct TurnDrag {
    /// The part being turned.
    pub name: String,
    /// 0 = X, 1 = Y, 2 = Z of the shown (active setup's) frame.
    pub axis: usize,
    /// The ring centre at press, shown frame: the pivot.
    pub center: [f64; 3],
    /// The in-plane angle under the cursor at the last motion, radians.
    pub last: f64,
    /// Unsnapped cumulative turn so far, radians (keeps Shift toggle smooth).
    pub raw_angle: f64,
    /// The signed turn so far (possibly snapped), radians (unwrapped: may pass 180 degrees).
    pub angle: f64,
}

/// The two in-plane unit vectors of the ring about `axis`, ordered so that
/// a positive turn about the axis carries `e1` toward `e2` (right hand).
fn plane(axis: usize) -> Option<([f64; 3], [f64; 3])> {
    Some((*AXES.get((axis + 1) % 3)?, *AXES.get((axis + 2) % 3)?))
}

fn add(a: [f64; 3], b: [f64; 3], k: f64) -> [f64; 3] {
    [a[0] + b[0] * k, a[1] + b[1] * k, a[2] + b[2] * k]
}

fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

/// The rings to draw this frame: body, centre, radius in mm.
#[derive(Clone, Debug)]
pub(crate) struct Rings {
    pub name: String,
    pub center: [f64; 3],
    pub radius: f64,
}

/// The rings, or None when nothing may be turned (Rotate off, no single
/// selected shown part, a degenerate view).
pub(crate) fn rings(state: &State) -> Option<Rings> {
    if !state.rotate_handles {
        return None;
    }
    let (name, entity) = one_part(state)?;
    let center = match state.body_turn.as_ref().filter(|d| d.name == name) {
        Some(d) => d.center,
        None => shown_center(state, &entity)?,
    };
    let radius = RING_PX / px_per_mm(state, center)?;
    Some(Rings { name, center, radius })
}

/// Each ring as a closed polyline in logical px: (axis, points). A point
/// behind the camera drops out of its ring.
pub(crate) fn rings_px(state: &State) -> Vec<(usize, Vec<[f32; 2]>)> {
    let (Some(r), Some(aspect)) = (rings(state), aspect(state)) else {
        return Vec::new();
    };
    let mvp = state.cam.mvp(aspect);
    (0..3)
        .filter_map(|axis| {
            let (e1, e2) = plane(axis)?;
            let pts: Vec<[f32; 2]> = (0..=RING_SEGS)
                .filter_map(|k| {
                    let phi = std::f64::consts::TAU * k as f64 / RING_SEGS as f64;
                    let p = add(add(r.center, e1, r.radius * phi.cos()), e2, r.radius * phi.sin());
                    project_uv(&mvp, p).map(|uv| to_px(state, uv))
                })
                .collect();
            (pts.len() > 1).then_some((axis, pts))
        })
        .collect()
}

/// Distance (px) from `p` to the segment `a`-`b`.
fn seg_dist(p: [f32; 2], a: [f32; 2], b: [f32; 2]) -> f32 {
    let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
    let len2 = dx * dx + dy * dy;
    let t = if len2 > 1e-12 { (((p[0] - a[0]) * dx + (p[1] - a[1]) * dy) / len2).clamp(0.0, 1.0) } else { 0.0 };
    (p[0] - a[0] - t * dx).hypot(p[1] - a[1] - t * dy)
}

/// The ring under `p` and the in-plane angle of the nearest ring point,
/// nearest ring first, among rings already computed this frame.
fn ring_hit(rings: &[(usize, Vec<[f32; 2]>)], p: [f32; 2]) -> Option<(usize, f64)> {
    rings
        .iter()
        .filter_map(|(axis, pts)| {
            let (k, d) = pts
                .windows(2)
                .enumerate()
                .map(|(k, w)| (k, seg_dist(p, w[0], w[1])))
                .min_by(|a, b| a.1.total_cmp(&b.1))?;
            (d <= RING_HIT_PX).then(|| (*axis, std::f64::consts::TAU * (k as f64 + 0.5) / RING_SEGS as f64, d))
        })
        .min_by(|a, b| a.2.total_cmp(&b.2))
        .map(|(axis, phi, _)| (axis, phi))
}

/// The ring under `p`, if any (the tests ask; the paint uses `ring_in`).
#[cfg(test)]
pub(crate) fn ring_at(state: &State, p: [f32; 2]) -> Option<usize> {
    ring_in(&rings_px(state), p)
}

/// The ring under `p` among rings already computed this frame (the paint
/// code computes them once and asks twice).
pub(crate) fn ring_in(rings: &[(usize, Vec<[f32; 2]>)], p: [f32; 2]) -> Option<usize> {
    ring_hit(rings, p).map(|(axis, _)| axis)
}

/// The in-plane angle of the cursor ray's hit on the ring plane. None (the
/// drag then holds its last value) when the ray runs within 3 degrees of
/// the plane, or when the hit lies within `min_r` mm of the pivot, where a
/// px of cursor swings the angle far.
pub(crate) fn plane_angle(state: &State, p: [f32; 2], center: [f64; 3], axis: usize, min_r: f64) -> Option<f64> {
    let u = *AXES.get(axis)?;
    let (e1, e2) = plane(axis)?;
    let (o, d) = ray_at(state, p)?;
    let (dn, dd) = (dot(d, u), dot(d, d));
    // NaN fails the comparison, so it is refused too.
    let seen_across = dd > 1e-24 && dn * dn >= move_drag::END_ON_SIN2 * dd;
    if !seen_across {
        return None;
    }
    let t = dot([center[0] - o[0], center[1] - o[1], center[2] - o[2]], u) / dn;
    let hit = add(o, d, t);
    let r = [hit[0] - center[0], hit[1] - center[1], hit[2] - center[2]];
    let (x, y) = (dot(r, e1), dot(r, e2));
    let r = x.hypot(y);
    if r.is_nan() || r <= min_r {
        return None;
    }
    let a = y.atan2(x);
    a.is_finite().then_some(a)
}

/// The radius (mm) about the pivot inside which the angle is held:
/// `RING_HIT_PX` on screen.
pub(crate) fn hold_radius(state: &State, center: [f64; 3]) -> Option<f64> {
    let r = f64::from(RING_HIT_PX) / px_per_mm(state, center)?;
    r.is_finite().then_some(r)
}

/// A ring seen within three degrees of its plane cannot give a stable turn.
pub(crate) fn ring_enabled(state: &State, center: [f64; 3], axis: usize) -> bool {
    let Some(normal) = AXES.get(axis) else { return false };
    let Some(aspect) = aspect(state) else { return false };
    let Some(uv) = project_uv(&state.cam.mvp(aspect), center) else { return false };
    let Some((_, direction)) = ray_at(state, to_px(state, uv)) else { return false };
    let (dn, dd) = (dot(direction, *normal), dot(direction, direction));
    dd > 1e-24 && dn * dn >= move_drag::END_ON_SIN2 * dd
}

pub(crate) fn edge_on_hint(axis: usize) -> String {
    format!("Orbit the view to turn about {}", ["X", "Y", "Z"].get(axis).copied().unwrap_or("the axis"))
}

/// Snap `raw_rad` to the nearest multiple of `step_deg` degrees.
#[must_use]
pub(crate) fn snap_angle_rad(raw_rad: f64, step_deg: f64) -> f64 {
    if !raw_rad.is_finite() || !step_deg.is_finite() || step_deg <= 0.0 {
        return raw_rad;
    }
    let out = crate::egui_app::viewport_bar::controls::snap_step(raw_rad.to_degrees(), step_deg).to_radians();
    if out.is_finite() { out } else { raw_rad }
}

/// A left press at `p`: a ring under it starts a turn and the press is
/// taken (true) - the ring wins over orbit and pick. Elsewhere: false.
pub(crate) fn press(state: &mut State, p: [f32; 2]) -> bool {
    let Some((axis, ring_phi)) = ring_hit(&rings_px(state), p) else { return false };
    let Some(r) = rings(state) else { return false };
    if !ring_enabled(state, r.center, axis) {
        state.status = edge_on_hint(axis);
        return true;
    }
    // A usable plane supplies the angle; the nearest ring point handles
    // a press within the hit tolerance of the pivot.
    let min_r = hold_radius(state, r.center).unwrap_or(f64::INFINITY);
    let last = plane_angle(state, p, r.center, axis, min_r).unwrap_or(ring_phi);
    state.body_turn = Some(TurnDrag { name: r.name, axis, center: r.center, last, raw_angle: 0.0, angle: 0.0 });
    crate::egui_app::viewport_bar::controls::sync_turn_drag(state, axis, 0.0);
    true
}

/// The pointer moved to `p` during a turn (without Shift snapping).
#[cfg(test)]
pub(crate) fn motion(state: &mut State, p: [f32; 2]) {
    motion_snapped(state, p, false);
}

/// The pointer moved to `p` during a turn: the preview follows, unwrapped
/// so a drag round past half a turn keeps counting, snapped to
/// `state.handle_bar.rotate_snap_deg` when `snap` is true.
pub(crate) fn motion_snapped(state: &mut State, p: [f32; 2], snap: bool) {
    let Some(d) = state.body_turn.clone() else { return };
    let Some(min_r) = hold_radius(state, d.center) else { return };
    if let Some(a) = plane_angle(state, p, d.center, d.axis, min_r) {
        let step = (a - d.last + std::f64::consts::PI).rem_euclid(std::f64::consts::TAU) - std::f64::consts::PI;
        let raw = d.raw_angle + step;
        if step.is_finite() && raw.is_finite() {
            let angle = if snap { snap_angle_rad(raw, state.handle_bar.rotate_snap_deg) } else { raw };
            if angle.is_finite()
                && let Some(live) = state.body_turn.as_mut()
            {
                live.raw_angle = raw;
                live.last = a;
                live.angle = angle;
            }
            crate::egui_app::viewport_bar::controls::sync_turn_drag(state, d.axis, angle);
        }
    }
}

/// Re-evaluate snapping on a held turn when Shift changes without motion.
pub(crate) fn resnap(state: &mut State, snap: bool) {
    let Some(d) = state.body_turn.clone() else { return };
    let next = if snap { snap_angle_rad(d.raw_angle, state.handle_bar.rotate_snap_deg) } else { d.raw_angle };
    if next != d.angle
        && next.is_finite()
        && let Some(live) = state.body_turn.as_mut()
    {
        live.angle = next;
        crate::egui_app::viewport_bar::controls::sync_turn_drag(state, d.axis, next);
    }
}

/// The release: one `TurnModelBy` for a real turn, nothing for none.
pub(crate) fn release(state: &mut State) {
    let Some(d) = state.body_turn.take() else { return };
    if d.angle.is_finite() && d.angle.abs() > MIN_TURN_RAD {
        crate::egui_app::viewport_bar::controls::sync_turn_drag(state, d.axis, d.angle);
        state.pending.push_back(CommandKind::TurnModelBy { name: d.name, axis: d.axis, angle_rad: d.angle });
    } else {
        crate::egui_app::viewport_bar::controls::clear_live_draft(state, false);
    }
}

/// Escape: drop the turn, queue nothing. True when there was one.
pub(crate) fn cancel(state: &mut State) -> bool {
    let had = state.body_turn.take().is_some();
    if had {
        crate::egui_app::viewport_bar::controls::clear_live_draft(state, false);
    }
    had
}

/// The shown-frame rigid turn by `angle` about `axis` through `center`.
fn turn_about(center: [f64; 3], axis: usize, angle: f64) -> Location {
    let rot = quat_from_axis_angle(axis, angle);
    let c = Point3::new(center[0], center[1], center[2]);
    let rc = quat_rotate_point(rot, c);
    Location { pos: Point3::new(c.x - rc.x, c.y - rc.y, c.z - rc.z), rot }
}

/// The preview turn for `name` while it is being turned (shown frame):
/// apply it after the part's shown pose.
#[must_use]
pub(crate) fn preview_turn(state: &State, name: &str) -> Option<Location> {
    state.body_turn.as_ref().filter(|d| d.name == name).map(|d| turn_about(d.center, d.axis, d.angle))
}

/// The bar's readout while turning: "Turn about Z: +30.0°".
#[must_use]
pub(crate) fn readout(state: &State) -> Option<String> {
    let d = state.body_turn.as_ref()?;
    Some(format!("Turn about {}: {}", ["X", "Y", "Z"].get(d.axis)?, shown_angle(d.angle)))
}

/// A signed angle in degrees, one decimal: "+30.0°".
#[must_use]
pub(crate) fn shown_angle(rad: f64) -> String {
    format!("{:+.1}\u{b0}", rad.to_degrees())
}

/// The placement the drain applies: the part's shown pose turned by
/// `angle` about shown `axis` through its bounding-box centre, brought back
/// into the document frame (an aligned setup shows the part turned).
#[must_use]
pub(crate) fn document_target(state: &State, entity: &Entity, axis: usize, angle: f64) -> Option<Location> {
    if axis > 2 || !angle.is_finite() {
        return None;
    }
    let center = shown_center(state, entity)?;
    let shown = setup_align::view_pose(&state.world, entity)?;
    let next = setup_align::compose(turn_about(center, axis, angle), shown);
    let doc = match setup_align::alignment(&state.world, camcraft_app::setup::active(&state.world).as_deref()) {
        Some(a) if setup_align::is_part(&state.world, entity) => setup_align::compose(setup_align::inverse(a), next),
        _ => next,
    };
    [doc.pos.x, doc.pos.y, doc.pos.z].iter().chain(doc.rot.iter()).all(|v| v.is_finite()).then_some(doc)
}
