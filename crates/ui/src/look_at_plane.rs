//! Look at a work plane (#2834): one click on an axis of an origin in the
//! view turns the view to face the plane that the axis is normal to. A
//! click on Z gives the XY work plane of that origin, X gives YZ, Y gives
//! ZX; a click on the origin point gives the XY plane. The origins are the
//! work offsets (WCS) and the world origin: the reference planes that a
//! document has. View state only: the camera turns about its target,
//! nothing is selected and no command is queued.
//! Provenance: our own code over our own origin gizmos, `project_uv` and
//! `Camera::toward`; the pick is the distance from a point to a segment on
//! the screen. No vendor material.

use camcraft_app::geometry::{Point3, transform_point};

use crate::interact::Gizmo;
use crate::state::State;

/// The status words: the tests read the same ones.
pub(crate) const ARMED: &str = "Look at: click an axis of an origin in the view";
pub(crate) const MISS: &str = "Look at: no origin axis under the cursor - the view did not change";

/// The status after a pick: the plane and whose origin it is.
pub(crate) fn done(plane: &str, origin: &str) -> String {
    format!("Look at: the view faces the {plane} plane of {origin}")
}

/// The plane that each axis (X, Y, Z) is normal to.
const PLANES: [&str; 3] = ["YZ", "ZX", "XY"];

/// A click this near (logical px) to an axis line takes that axis.
const REACH_PX: f32 = 10.0;
/// A click this near to an origin point, in the first quarter of the axis
/// line that it is nearest to, takes the origin and not that axis.
const ORIGIN_PX: f32 = 5.0;
const ORIGIN_SHARE: f32 = 0.25;

/// One axis of one origin as the view shows it.
#[derive(Clone, Debug)]
pub(crate) struct AxisMark {
    /// Whose origin: the work offset name, or the world origin label.
    pub(crate) origin: String,
    /// 0, 1, 2 for X, Y, Z.
    pub(crate) axis: usize,
    /// The unit world direction of the axis: the normal of its plane.
    pub(crate) normal: [f64; 3],
    /// The axis line on the screen, origin to tip, in logical px.
    pub(crate) from: [f32; 2],
    pub(crate) to: [f32; 2],
}

/// Arm or disarm the one-click pick. One pick at a time: the face pick
/// gives way.
pub(crate) fn arm(state: &mut State, on: bool) {
    state.look_at_armed = false;
    state.look_at_plane_armed = on;
    state.status = if on { ARMED } else { crate::look_at::OFF }.to_owned();
}

/// The world point at the tip of axis `axis` of `gizmo`, `len` from its
/// origin, with the reversed axes of the WCS applied.
fn tip(gizmo: &Gizmo, axis: usize, len: f64) -> Point3 {
    let mut dir = [0.0; 3];
    if let Some(d) = dir.get_mut(axis) {
        *d = 1.0;
    }
    let dir = gizmo.axis_dir(axis, &dir);
    let base = Point3::new(gizmo.pos[0], gizmo.pos[1], gizmo.pos[2]);
    transform_point(Point3::new(dir[0] * len, dir[1] * len, dir[2] * len), base, gizmo.rot)
}

/// The unit world direction of axis `axis` of `gizmo`; `None` when the
/// stored rotation gives no direction.
pub(crate) fn normal_of(gizmo: &Gizmo, axis: usize) -> Option<[f64; 3]> {
    let t = tip(gizmo, axis, 1.0);
    let d = [t.x - gizmo.pos[0], t.y - gizmo.pos[1], t.z - gizmo.pos[2]];
    let len = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt();
    (len.is_finite() && len > 1e-9).then(|| [d[0] / len, d[1] / len, d[2] / len])
}

/// Each origin axis that the view draws, with its line on the screen. An
/// axis with an end behind the camera is not listed, like its drawn line.
pub(crate) fn axis_marks(state: &State) -> Vec<AxisMark> {
    let ([x0, y0], [x1, y1]) = state.viewport_rect;
    let (w, h) = (x1 - x0, y1 - y0);
    if w <= 1.0 || h <= 1.0 {
        return Vec::new();
    }
    let mvp = state.cam.mvp(f64::from(w / h));
    let px = |uv: [f32; 2]| [x0 + uv[0] * w, y0 + uv[1] * h];
    let len = crate::interact::gizmo_len(state);
    let shown = crate::view_frame::axes_shown(state);
    let mut out = Vec::new();
    for gizmo in crate::interact::gizmos(state) {
        let Some(from) = crate::interact::project_uv(&mvp, gizmo.pos) else { continue };
        for axis in (0..3).filter(|a| shown.get(*a).copied().unwrap_or(false)) {
            let t = tip(&gizmo, axis, len);
            let (Some(to), Some(normal)) = (crate::interact::project_uv(&mvp, [t.x, t.y, t.z]), normal_of(&gizmo, axis))
            else {
                continue;
            };
            out.push(AxisMark { origin: gizmo.name.clone(), axis, normal, from: px(from), to: px(to) });
        }
    }
    out
}

/// The distance from `p` to the segment `a`-`b`, in px, and where along
/// the segment the nearest point is (0 at `a`, 1 at `b`).
fn segment_distance(p: [f32; 2], a: [f32; 2], b: [f32; 2]) -> (f32, f32) {
    let (ab, ap) = ([b[0] - a[0], b[1] - a[1]], [p[0] - a[0], p[1] - a[1]]);
    let len2 = ab[0] * ab[0] + ab[1] * ab[1];
    let t = if len2 > 1e-12 { ((ap[0] * ab[0] + ap[1] * ab[1]) / len2).clamp(0.0, 1.0) } else { 0.0 };
    ((ap[0] - ab[0] * t).hypot(ap[1] - ab[1] * t), t)
}

/// The axis that a click at the logical-px point `p` takes: the nearest
/// axis line within reach. A click on the origin point of that line takes
/// the Z axis of the same origin: its XY work plane. `None` when no axis
/// is within reach.
pub(crate) fn pick(state: &State, p: [f32; 2]) -> Option<AxisMark> {
    let marks = axis_marks(state);
    let (_, at, best) = marks
        .iter()
        .map(|m| {
            let (d, t) = segment_distance(p, m.from, m.to);
            (d, t, m)
        })
        .filter(|(d, _, _)| d.is_finite() && *d <= REACH_PX)
        .min_by(|a, b| a.0.total_cmp(&b.0))?;
    let on_origin = (p[0] - best.from[0]).hypot(p[1] - best.from[1]) <= ORIGIN_PX && at <= ORIGIN_SHARE;
    if on_origin
        && let Some(z) = marks.iter().find(|m| m.axis == 2 && m.origin == best.origin && m.from == best.from)
    {
        return Some(z.clone());
    }
    Some(best.clone())
}

/// The armed click: turn the view to the plane of the axis under `p`, or
/// say that there is none. One shot: the pick disarms in each case.
pub(crate) fn click(state: &mut State, p: [f32; 2]) {
    state.look_at_plane_armed = false;
    match pick(state, p) {
        Some(m) => {
            // The eye stands on the axis and looks back along it.
            crate::actions::orient_view(state, m.normal);
            state.status = done(PLANES.get(m.axis).copied().unwrap_or("work"), &m.origin);
        }
        None => state.status = MISS.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::{normal_of, segment_distance};
    use crate::interact::Gizmo;

    fn gizmo(rot: [f64; 4], flip: [bool; 3]) -> Gizmo {
        Gizmo { pos: [5.0, -3.0, 2.0], rot, world_axes: false, name: "G54".into(), flip }
    }

    #[test]
    fn the_plane_normal_follows_the_rotation_and_the_reversed_axes_of_the_wcs() {
        let plain = gizmo([0.0, 0.0, 0.0, 1.0], [false; 3]);
        assert_eq!(normal_of(&plain, 0), Some([1.0, 0.0, 0.0]));
        assert_eq!(normal_of(&plain, 2), Some([0.0, 0.0, 1.0]));
        // A quarter turn about X: X stays, Z lies along the Y axis.
        let s = std::f64::consts::FRAC_1_SQRT_2;
        let turned = gizmo([s, 0.0, 0.0, s], [false; 3]);
        let x = normal_of(&turned, 0).expect("x");
        assert!((x[0] - 1.0).abs() < 1e-9, "{x:?}");
        let z = normal_of(&turned, 2).expect("z");
        assert!(z[0].abs() < 1e-9 && (z[1].abs() - 1.0).abs() < 1e-9 && z[2].abs() < 1e-9, "{z:?}");
        // A reversed Z axis gives the other side of the same plane.
        let flipped = normal_of(&gizmo([s, 0.0, 0.0, s], [false, false, true]), 2).expect("flipped z");
        assert!((flipped[1] + z[1]).abs() < 1e-9, "{flipped:?} vs {z:?}");
        // A rotation that is not a number gives no direction and no failure.
        assert_eq!(normal_of(&gizmo([f64::NAN; 4], [false; 3]), 2), None);
    }

    #[test]
    fn the_distance_to_an_axis_line_stops_at_its_ends() {
        let (a, b) = ([0.0, 0.0], [10.0, 0.0]);
        assert_eq!(segment_distance([5.0, 3.0], a, b), (3.0, 0.5));
        assert_eq!(segment_distance([14.0, 3.0], a, b), (5.0, 1.0), "past the tip: the distance to the tip");
        assert_eq!(segment_distance([-3.0, 4.0], a, b), (5.0, 0.0), "before the origin: the distance to the origin");
        assert_eq!(segment_distance([3.0, 4.0], a, a), (5.0, 0.0), "a line of no length is its point");
    }
}
