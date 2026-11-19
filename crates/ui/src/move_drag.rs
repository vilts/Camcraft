//! Drag-to-move handles on a selected body (#1198 slice b): with the bar's
//! Move mode on, the one selected part wears three axis arms, each ending
//! in a round ball; pressing a ball and dragging slides the body along that
//! axis. The drag is a preview (view state, never undoable); the release
//! queues ONE `CommandKind::MoveModelBy`, which the drain turns into the
//! same `MoveModel` the Model page's fields run - so staleness, the stock
//! that follows the part and the fixture-clash warning all behave exactly
//! as for a typed-in move. Escape cancels with no history step; a drag
//! that ends where it began adds none.
//!
//! Provenance: our own code. The drag distance is the closest approach of
//! the cursor ray to the axis line (two-line closest points, any vector
//! geometry text); the constant-on-screen handle is the usual CAD gizmo
//! convention. No vendor material consulted.

use std::sync::Arc;

use camcraft_app::geometry::{Point3, Triangle, transform_point};
use camcraft_app::schema::Location;
use camcraft_app::model;
use camcraft_app::setup_align;
use flecs_ecs::prelude::Entity;

use crate::interact::{project_uv, ray_at};
use crate::state::{CommandKind, State};

/// How long an arm looks on screen, logical px, whatever the zoom.
pub(crate) const ARM_PX: f64 = 70.0;
/// The ball's drawn radius and its (more forgiving) hit radius, px.
pub(crate) const BALL_PX: f32 = 7.0;
pub(crate) const BALL_HIT_PX: f32 = 11.0;
/// Below this (mm) a release is "no move" and queues nothing.
pub(crate) const MIN_MOVE_MM: f64 = 1e-6;

const AXES: [[f64; 3]; 3] = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];

/// A live handle drag - view state like the camera's orbit, never saved,
/// never in history.
#[derive(Clone, Debug, PartialEq)]
pub struct BodyDrag {
    /// The part being dragged.
    pub name: String,
    /// 0 = X, 1 = Y, 2 = Z of the shown (active setup's) frame.
    pub axis: usize,
    /// The gizmo centre at press, shown frame - the axis line's anchor.
    pub origin: [f64; 3],
    /// The axis parameter under the cursor at press.
    pub start_t: f64,
    /// The signed distance dragged so far, mm.
    pub delta: f64,
}

impl BodyDrag {
    /// The preview offset, shown frame.
    #[must_use]
    pub fn offset(&self) -> [f64; 3] {
        let mut v = [0.0; 3];
        if let Some(c) = v.get_mut(self.axis) {
            *c = self.delta;
        }
        v
    }
}

/// The handles to draw this frame: the body, its centre (preview offset
/// included) and the arm length in mm that makes `ARM_PX` on screen.
#[derive(Clone, Debug)]
pub(crate) struct Handles {
    pub name: String,
    pub center: [f64; 3],
    pub len: f64,
}

/// The one body the handles may grab: Move mode on, exactly one selected,
/// shown part (not a fixture, the stock or a work offset - those have their
/// own pages and commands).
fn draggable(state: &State) -> Option<(String, Entity)> {
    if !state.move_handles {
        return None;
    }
    one_part(state)
}

/// The one selected, shown part - the body a Move or a Rotate handle grabs
/// (#1198 b, #2047). None for no selection, several, a fixture or the stock.
pub(crate) fn one_part(state: &State) -> Option<(String, Entity)> {
    let mut picked = model::model_entities(&state.world).into_iter().filter(|(name, e)| {
        setup_align::is_part(&state.world, e)
            && model::is_selected(&state.world, e)
            && !state.hidden_models.iter().any(|h| h == name)
    });
    let one = picked.next()?;
    picked.next().is_none().then_some(one)
}

/// The shown bounding-box centre of the handled body (Move or Rotate), kept between frames
/// (#2238 frame-cost law: a still frame walks no facets). The key is the
/// input itself: the entity, the facet `Arc` (held, so its address cannot
/// be reused; a geometry change replaces it) and the shown pose bit for bit
/// (a move, an undo or a setup switch changes it). View cache - never
/// truth, never saved.
#[derive(Default)]
pub struct CenterCache {
    key: Option<(Entity, Arc<[Triangle]>, [u64; 7])>,
    center: Option<[f64; 3]>,
    /// How many times the facets were walked (the counter test reads it).
    pub computes: u64,
}

fn pose_bits(loc: &Location) -> [u64; 7] {
    let [a, b, c, d] = loc.rot.map(f64::to_bits);
    [loc.pos.x.to_bits(), loc.pos.y.to_bits(), loc.pos.z.to_bits(), a, b, c, d]
}

/// The body's bounding-box centre where the viewport draws it, from the
/// cache when its input is unchanged.
pub(crate) fn shown_center(state: &State, entity: &Entity) -> Option<[f64; 3]> {
    let tris = model::facets_of(&state.world, entity)?;
    let loc = setup_align::view_pose(&state.world, entity)?;
    let id = *entity;
    let bits = pose_bits(&loc);
    let mut cache = state.move_center.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    if let Some((k_id, k_tris, k_bits)) = &cache.key
        && *k_id == id
        && Arc::ptr_eq(k_tris, &tris)
        && *k_bits == bits
    {
        return cache.center;
    }
    let center = walk_center(&tris, &loc);
    cache.computes = cache.computes.saturating_add(1);
    cache.key = Some((id, tris, bits));
    cache.center = center;
    center
}

fn walk_center(tris: &[Triangle], loc: &Location) -> Option<[f64; 3]> {
    let mut lo = [f64::INFINITY; 3];
    let mut hi = [f64::NEG_INFINITY; 3];
    for p in tris.iter().flat_map(|t| [t.a, t.b, t.c]) {
        let q = transform_point(p, loc.pos, loc.rot);
        for (i, v) in [q.x, q.y, q.z].into_iter().enumerate() {
            lo[i] = lo[i].min(v);
            hi[i] = hi[i].max(v);
        }
    }
    let c = [(lo[0] + hi[0]) * 0.5, (lo[1] + hi[1]) * 0.5, (lo[2] + hi[2]) * 0.5];
    c.iter().all(|v| v.is_finite()).then_some(c)
}

pub(crate) fn aspect(state: &State) -> Option<f64> {
    let ([x0, y0], [x1, y1]) = state.viewport_rect;
    let (w, h) = (f64::from(x1 - x0), f64::from(y1 - y0));
    (w > 1.0 && h > 1.0).then(|| w / h)
}

/// Logical px of the viewport per mm at `at`, measured along the camera's
/// right vector (so no arm can blow up when it points at the eye).
pub(crate) fn px_per_mm(state: &State, at: [f64; 3]) -> Option<f64> {
    let aspect = aspect(state)?;
    let mvp = state.cam.mvp(aspect);
    let (r, _, _) = state.cam.view_basis();
    let a = project_uv(&mvp, at)?;
    let b = project_uv(&mvp, [at[0] + r.0, at[1] + r.1, at[2] + r.2])?;
    let ([x0, _], [x1, _]) = state.viewport_rect;
    let k = f64::from((b[0] - a[0]).abs() * (x1 - x0));
    (k.is_finite() && k > 1e-9).then_some(k)
}

/// The handles to draw, or None when nothing may be dragged.
pub(crate) fn handles(state: &State) -> Option<Handles> {
    let (name, entity) = draggable(state)?;
    let mut center = shown_center(state, &entity)?;
    if let Some(d) = state.body_drag.as_ref().filter(|d| d.name == name) {
        let o = d.offset();
        for i in 0..3 {
            center[i] += o[i];
        }
    }
    let len = ARM_PX / px_per_mm(state, center)?;
    Some(Handles { name, center, len })
}

pub(crate) fn to_px(state: &State, uv: [f32; 2]) -> [f32; 2] {
    let ([x0, y0], [x1, y1]) = state.viewport_rect;
    [x0 + uv[0] * (x1 - x0), y0 + uv[1] * (y1 - y0)]
}

/// The centre and the three balls in logical px: (axis, centre px, ball px).
pub(crate) fn arms_px(state: &State) -> Vec<(usize, [f32; 2], [f32; 2])> {
    let (Some(h), Some(aspect)) = (handles(state), aspect(state)) else {
        return Vec::new();
    };
    let mvp = state.cam.mvp(aspect);
    let Some(c) = project_uv(&mvp, h.center) else {
        return Vec::new();
    };
    let c = to_px(state, c);
    AXES.iter()
        .enumerate()
        .filter_map(|(i, a)| {
            let end = [h.center[0] + a[0] * h.len, h.center[1] + a[1] * h.len, h.center[2] + a[2] * h.len];
            project_uv(&mvp, end).map(|uv| (i, c, to_px(state, uv)))
        })
        .collect()
}

/// The ball under `p`, nearest first.
pub(crate) fn ball_at(state: &State, p: [f32; 2]) -> Option<usize> {
    ball_in(&arms_px(state), p)
}

/// The ball under `p` among arms already computed this frame (the paint
/// code computes them once and asks twice).
pub(crate) fn ball_in(arms: &[(usize, [f32; 2], [f32; 2])], p: [f32; 2]) -> Option<usize> {
    arms.iter()
        .map(|&(i, _, b)| (i, (b[0] - p[0]).hypot(b[1] - p[1])))
        .filter(|(_, d)| *d <= BALL_HIT_PX)
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .map(|(i, _)| i)
}

/// Below this angle between the cursor ray and the axis the axis is seen
/// almost end-on: a px of cursor is many mm along it, so the drag holds its
/// last value rather than jump (sin^2 of 3 degrees).
pub(crate) const END_ON_SIN2: f64 = 0.002_739_052_315_863_355;

/// The axis parameter of the point on the line `origin + t * u` (`u` a unit
/// axis) closest to the ray `o + s * d`. None when the ray runs within
/// 3 degrees of the axis.
pub(crate) fn ray_axis_t(o: [f64; 3], d: [f64; 3], origin: [f64; 3], u: [f64; 3]) -> Option<f64> {
    let dot = |a: [f64; 3], b: [f64; 3]| a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
    let w0 = [origin[0] - o[0], origin[1] - o[1], origin[2] - o[2]];
    let (b, c) = (dot(u, d), dot(d, d));
    let (dd, e) = (dot(u, w0), dot(d, w0));
    let denom = c - b * b;
    // NaN fails both comparisons, so it is refused too.
    let seen_across = c > 1e-24 && denom >= END_ON_SIN2 * c;
    if !seen_across {
        return None;
    }
    let t = (b * e - c * dd) / denom;
    t.is_finite().then_some(t)
}

/// One motion step moves the drag by at most `limit` mm (one view width):
/// a near end-on axis can never throw the part off screen in one event.
pub(crate) fn clamp_step(prev: f64, next: f64, limit: f64) -> f64 {
    if !limit.is_finite() || limit <= 0.0 {
        return prev;
    }
    next.clamp(prev - limit, prev + limit)
}

fn axis_t(state: &State, p: [f32; 2], origin: [f64; 3], axis: usize) -> Option<f64> {
    let u = *AXES.get(axis)?;
    let (o, d) = ray_at(state, p)?;
    ray_axis_t(o, d, origin, u)
}

/// One view width in mm at `at`: the most one motion event may drag.
fn step_limit(state: &State, at: [f64; 3]) -> Option<f64> {
    let ([x0, _], [x1, _]) = state.viewport_rect;
    let k = px_per_mm(state, at)?;
    Some(f64::from(x1 - x0) / k)
}

/// A left press at `p`: a ball under it starts a drag and the press is
/// taken (true) - the ball wins over orbit and pick. Elsewhere: false.
pub(crate) fn press(state: &mut State, p: [f32; 2]) -> bool {
    let Some(axis) = ball_at(state, p) else { return false };
    let Some(h) = handles(state) else { return false };
    let Some(start_t) = axis_t(state, p, h.center, axis) else { return false };
    state.body_drag = Some(BodyDrag { name: h.name, axis, origin: h.center, start_t, delta: 0.0 });
    true
}

/// The pointer moved to `p` during a drag: the preview follows.
pub(crate) fn motion(state: &mut State, p: [f32; 2]) {
    let Some(d) = state.body_drag.clone() else { return };
    let Some(t) = axis_t(state, p, d.origin, d.axis) else { return };
    let Some(limit) = step_limit(state, d.origin) else { return };
    let next = clamp_step(d.delta, t - d.start_t, limit);
    if let Some(live) = state.body_drag.as_mut()
        && next.is_finite()
    {
        live.delta = next;
    }
}

/// The release: one `MoveModelBy` for a real move, nothing for none.
pub(crate) fn release(state: &mut State) {
    let Some(d) = state.body_drag.take() else { return };
    if d.delta.is_finite() && d.delta.abs() > MIN_MOVE_MM {
        state.pending.push_back(CommandKind::MoveModelBy { delta: d.offset(), name: d.name });
    }
}

/// Escape: drop the drag, queue nothing. True when there was one.
pub(crate) fn cancel(state: &mut State) -> bool {
    state.body_drag.take().is_some()
}

/// The preview offset for `name` while it is being dragged (shown frame).
#[must_use]
pub(crate) fn preview_offset(state: &State, name: &str) -> Option<[f64; 3]> {
    state.body_drag.as_ref().filter(|d| d.name == name).map(BodyDrag::offset)
}

/// The bar's readout while dragging: "X +12.500 mm".
#[must_use]
pub(crate) fn readout(state: &State) -> Option<String> {
    let d = state.body_drag.as_ref()?;
    let u = state.units();
    let axis = ["X", "Y", "Z"].get(d.axis)?;
    Some(format!("{axis} {} {}", camcraft_app::units::len_signed(d.delta, u, 3), u.suffix()))
}

/// The status line's "by X +12.500 mm" for a drag of `delta` (shown frame):
/// the axis or axes it moved along, signed.
#[must_use]
pub(crate) fn shown_delta(delta: [f64; 3], u: camcraft_app::units::Unit) -> String {
    let parts: Vec<String> = ["X", "Y", "Z"]
        .iter()
        .zip(delta)
        .filter(|(_, v)| v.abs() > MIN_MOVE_MM)
        .map(|(a, v)| format!("{a} {}", camcraft_app::units::len_signed(v, u, 3)))
        .collect();
    if parts.is_empty() { "by nothing".to_string() } else { format!("by {} {}", parts.join(", "), u.suffix()) }
}

/// The move the drain applies: the shown-frame `delta` brought back into
/// the document frame (an aligned setup shows the part turned), added to
/// the part's stored position; the rotation is kept bit for bit.
#[must_use]
pub(crate) fn document_target(
    state: &State,
    entity: &Entity,
    delta: [f64; 3],
) -> Option<camcraft_app::schema::Location> {
    let prev = model::transform_of(&state.world, entity).unwrap_or_default();
    let shown = Point3::new(delta[0], delta[1], delta[2]);
    let d = match camcraft_app::setup_align::alignment(&state.world, camcraft_app::setup::active(&state.world).as_deref()) {
        Some(a) => camcraft_app::geometry::quat_rotate_point(camcraft_app::geometry::quat_conjugate(a.rot), shown),
        None => shown,
    };
    let pos = Point3::new(prev.pos.x + d.x, prev.pos.y + d.y, prev.pos.z + d.z);
    [pos.x, pos.y, pos.z].iter().all(|v| v.is_finite()).then_some(camcraft_app::schema::Location { pos, rot: prev.rot })
}
