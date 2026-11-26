//! The rotated-work-offset clause of the verification gate (#315). A
//! work offset may carry a rotation (`SetWcsOrigin`, the 3-2-1
//! alignment of #187), but the post speaks translation only: the
//! stream carries `OriginX/Y/Z` and every shipped script subtracts that
//! origin from world coordinates. A rotated offset would therefore post
//! a program turned against the part. Until the rotation is carried
//! (baked into the coordinates or a control cycle - #315 step 3), the
//! post refuses it by name; an offset within [`ROT_TOL_RAD`] of the
//! identity posts exactly as before.
//!
//! Provenance: our own design notes (the G54 register is translation
//! only - public RS-274 / ISO 6983 work-offset semantics); no vendor
//! material consulted.

use flecs_ecs::prelude::World;

use crate::model;
use crate::schema::Location;

use super::wcs_slot::wcs_slot;

/// Rotation angle below which an offset counts as unrotated: 1e-9 rad
/// turns a point 1 m out by 1 nm - far under any machine resolution,
/// and far over the float noise of a normalized identity quaternion.
pub(crate) const ROT_TOL_RAD: f64 = 1e-9;

/// Angle (rad, in [0, pi]) and unit axis of a unit quaternion (x, y, z, w).
/// `atan2` rather than `acos(w)`, which is ill-conditioned near identity.
fn angle_axis(q: [f64; 4]) -> (f64, [f64; 3]) {
    let s = (q[0] * q[0] + q[1] * q[1] + q[2] * q[2]).sqrt();
    if !s.is_finite() || s <= 0.0 {
        return (0.0, [0.0, 0.0, 1.0]);
    }
    let sign = if q[3] < 0.0 { -1.0 } else { 1.0 };
    let angle = 2.0 * s.atan2(q[3].abs());
    (angle, [sign * q[0] / s, sign * q[1] / s, sign * q[2] / s])
}

/// The axis as the user reads it: `X`/`Y`/`Z` (signed) when it is a
/// principal axis, else its components.
fn axis_word(a: [f64; 3]) -> String {
    for (i, w) in ["X", "Y", "Z"].iter().enumerate() {
        if (a[i].abs() - 1.0).abs() < 1e-9 {
            return if a[i] < 0.0 { format!("-{w}") } else { (*w).to_string() };
        }
    }
    format!("({:.4}, {:.4}, {:.4})", a[0], a[1], a[2])
}

/// What an offset's placement says beyond its translation (#327, #760).
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum OffsetFrame {
    /// Finite, and within [`ROT_TOL_RAD`] of the identity.
    Square,
    /// Pure Z-axis rotation (`WCS +Z` matches machine `+Z`), signed `yaw_deg`
    /// from setup `+X` to WCS `+X` (#760).
    Yaw { deg: f64, yaw_deg: f64 },
    /// Finite and tilted off machine `+Z`: the angle (deg) and axis.
    Rotated { deg: f64, axis: String },
    /// A NaN / infinite origin or rotation, or a zero-norm quaternion:
    /// no frame at all. Never "unrotated" - refused by name.
    NonFinite,
}

/// Classify a work-offset placement. A non-finite or degenerate value is
/// its own verdict, never folded into "unrotated" (#327).
#[must_use]
pub(crate) fn offset_frame(loc: &Location) -> OffsetFrame {
    let q = loc.rot;
    let norm2 = q.iter().map(|v| v * v).sum::<f64>();
    if !loc.pos.is_finite() || !q.iter().all(|v| v.is_finite()) || !norm2.is_finite() || norm2 <= 0.0 {
        return OffsetFrame::NonFinite;
    }
    let (angle, axis) = angle_axis(q);
    if angle <= ROT_TOL_RAD {
        return OffsetFrame::Square;
    }
    let z = crate::geometry::transform_point(
        crate::geometry::Point3::new(0.0, 0.0, 1.0),
        crate::geometry::Point3::ZERO,
        q,
    );
    let tilt = (z.x * z.x + z.y * z.y).sqrt().atan2(z.z);
    if tilt <= ROT_TOL_RAD {
        let x = crate::geometry::transform_point(
            crate::geometry::Point3::new(1.0, 0.0, 0.0),
            crate::geometry::Point3::ZERO,
            q,
        );
        let mut yaw_deg = x.y.atan2(x.x).to_degrees();
        if (yaw_deg + 180.0).abs() < 1e-9 {
            yaw_deg = 180.0;
        }
        if yaw_deg == -0.0 {
            yaw_deg = 0.0;
        }
        return OffsetFrame::Yaw { deg: yaw_deg.abs(), yaw_deg };
    }
    OffsetFrame::Rotated { deg: angle.to_degrees(), axis: axis_word(axis) }
}

fn unsupported_op_reason(
    op: &crate::schema::Operation,
    cache: Option<crate::schema::Toolpath>,
) -> Option<&'static str> {
    use crate::operation::{OpParams, Section};
    if matches!(
        op.params,
        OpParams::Probe { .. }
            | OpParams::WireContour(_)
            | OpParams::SinkerBurn(_)
            | OpParams::SinkerSideBurn(_)
            // #1444: not verified under a rotated offset.
            | OpParams::SlotUndercut(_)
            | OpParams::RotaryContour { .. }
            | OpParams::RotaryParallel { .. }
            | OpParams::RotaryOffset { .. }
            | OpParams::MultiAxisRough { .. }
            | OpParams::Flow5D { .. }
            | OpParams::MultiAxisContour { .. }
            | OpParams::MultiAxisDeburr { .. }
            // #2826: the band's cylinders and rotary words are not
            // verified under a rotated offset.
            | OpParams::RotaryFlow { .. }
    ) || op.params.kind().info().section == Section::Turning
    {
        return Some("that operation type is not supported under a rotated work offset yet");
    }
    if op.params.tilt().is_some() {
        return Some("3+2 tilted toolpaths are not supported under a rotated work offset yet");
    }
    if let Some(c) = cache {
        if c.path.moves.iter().any(|m| m.arc.is_some_and(|a| !a.plane.is_xy())) {
            return Some("an XZ/YZ arc cannot rotate in XY under a rotated work offset");
        }
        if c.path
            .moves
            .iter()
            .any(|m| m.rotary.is_some() || m.axis.is_some_and(|a| !crate::cl::is_vertical(&a)))
        {
            return Some("multiaxis or rotary moves are not supported under a rotated work offset yet");
        }
    }
    None
}

fn control_refusal(
    world: &World,
    setup: Option<&str>,
    owner: &str,
    explicit_cps: Option<&str>,
) -> Option<String> {
    if crate::setup_post_options::of(world, setup).wcs_rotation != crate::schema::WcsRotationOutput::Control {
        return None;
    }
    let (_, mch) = crate::machine::of_setup(world, setup);
    match mch.as_ref() {
        Some(m) if m.coordinate_rotation == Some(true) => {
            if !m.inverted_axes().is_empty() {
                return Some(format!(
                    "{owner}: Control rotation (G68 / ROT) is not supported with inverted machine axes - choose Bake into XY"
                ));
            }
        }
        Some(m) => {
            return Some(format!(
                "{owner}: WcsRotation is Control (G68 / ROT), but mounted machine '{}' does not state controller.coordinate_rotation: true - choose Bake into XY or declare coordinate_rotation",
                m.model
            ));
        }
        None => {
            return Some(format!(
                "{owner}: WcsRotation is Control (G68 / ROT), which requires a mounted machine declaring controller.coordinate_rotation: true - choose Bake into XY"
            ));
        }
    }
    let cps_ok = match explicit_cps {
        Some(cps) => crate::post::dialect::voices_wcs_rotation(cps),
        None => match super::script_of(&crate::setup_mount::post_of(world, setup)) {
            Ok((_, text)) => crate::post::dialect::voices_wcs_rotation(&text),
            Err(_) => return None,
        },
    };
    if matches!(cps_ok, Ok(false)) {
        return Some(format!(
            "{owner}: WcsRotation is Control (G68 / ROT), but the post script does not declare wcsRotation = true - choose Bake into XY or use Fanuc, Haas, Mazak, Doosan DNM, or Sinumerik"
        ));
    }
    None
}

/// One line per blocked rotated (or non-finite, #327) work-offset entity
/// a posted operation uses. Empty when every offset is unrotated or a
/// supported Z-rotated milling offset (#760).
#[must_use]
pub(super) fn rotated_wcs_problems(world: &World) -> Vec<String> {
    rotated_wcs_problems_with(world, None)
}

/// Explicit-script Control capability check for standalone `post_script` calls.
#[must_use]
pub(super) fn explicit_script_problems(world: &World, cps: &str) -> Vec<String> {
    rotated_wcs_problems_with(world, Some(cps))
}

fn rotated_wcs_problems_with(world: &World, explicit_cps: Option<&str>) -> Vec<String> {
    // (entity name, line prefix, ops), in first-use program order.
    let mut hits: Vec<(String, String, Vec<String>)> = Vec::new();
    for (name, e) in crate::setup::operation_order(world) {
        let Some(op) = model::operation_of(world, &e) else {
            continue;
        };
        if op.suppressed {
            continue; // not in the program (#110)
        }
        let setup = op.setup.as_deref();
        let slot = wcs_slot(world, setup);
        let Some(wcs) = crate::setup::wcs_entity(world, setup) else {
            continue; // never placed: the identity at the origin
        };
        let Some(loc) = model::transform_of(world, &wcs) else {
            continue;
        };
        let frame = offset_frame(&loc);
        if frame == OffsetFrame::Square {
            continue;
        }
        let word = crate::setup::offset_name(world, setup, u32::try_from(slot).map_or(u32::MAX, |s| s.saturating_add(1)));
        let owner = match crate::setup::effective(world, setup) {
            Some(s) => format!("setup {s}: its WCS ({word})"),
            None => format!("the WCS ({word})"),
        };
        let prefix = match frame {
            OffsetFrame::Square => continue,
            OffsetFrame::Yaw { deg, .. } => {
                if explicit_cps.is_none()
                    && let Some(why) = unsupported_op_reason(&op, model::toolpath_of(world, &e))
                {
                    format!("{owner} is rotated {deg:.3} deg about Z; {why}")
                } else if let Some(why) = control_refusal(world, setup, &owner, explicit_cps) {
                    why
                } else {
                    continue;
                }
            }
            OffsetFrame::Rotated { deg, axis } if explicit_cps.is_none() => format!(
                "{owner} is rotated {deg:.3} deg about {axis}; posting a tilted work offset is not \
                 supported - align WCS Z to machine +Z (use Part alignment on the Setup page) or post from an unrotated setup"
            ),
            OffsetFrame::NonFinite if explicit_cps.is_none() => format!(
                "{owner} has a non-finite placement (origin or rotation) - re-place the \
                 offset before posting"
            ),
            _ => continue,
        };
        let entity = crate::setup::wcs_entity_name(world, setup);
        let label = model::operation_label(world, &name, &e);
        if let Some(hit) = hits.iter_mut().find(|h| h.0 == entity && h.1 == prefix) {
            hit.2.push(label);
        } else {
            hits.push((entity, prefix, vec![label]));
        }
    }
    hits.into_iter()
        .map(|(_, prefix, ops)| format!("{prefix} (operations: {})", ops.join(", ")))
        .collect()
}
