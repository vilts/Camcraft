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

/// What an offset's placement says beyond its translation (#327).
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum OffsetFrame {
    /// Finite, and within [`ROT_TOL_RAD`] of the identity.
    Square,
    /// Finite and turned: the angle (deg) and the axis as the user reads it.
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
        OffsetFrame::Square
    } else {
        OffsetFrame::Rotated { deg: angle.to_degrees(), axis: axis_word(axis) }
    }
}

/// One line per rotated (or non-finite, #327) work-offset entity a
/// posted operation uses, naming the setup, the G-word, the rotation and
/// the operations. Empty when every offset in the program is unrotated.
#[must_use]
pub(super) fn rotated_wcs_problems(world: &World) -> Vec<String> {
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
        let entity = crate::setup::wcs_entity_name(world, setup);
        let label = model::operation_label(world, &name, &e);
        if let Some(hit) = hits.iter_mut().find(|h| h.0 == entity) {
            hit.2.push(label);
            continue;
        }
        let word = crate::setup::offset_name(world, setup, u32::try_from(slot).map_or(u32::MAX, |s| s.saturating_add(1)));
        let owner = match crate::setup::effective(world, setup) {
            Some(s) => format!("setup {s}: its WCS ({word})"),
            None => format!("the WCS ({word})"),
        };
        let prefix = match frame {
            OffsetFrame::Rotated { deg, axis } => format!(
                "{owner} is rotated {deg:.3} deg about {axis}; posting a rotated work offset is not \
                 supported yet - re-align it without rotation or post from an unrotated setup"
            ),
            // #327: a NaN / infinite / zero-norm placement is no frame at
            // all - refused by name, never posted as unrotated.
            _ => format!(
                "{owner} has a non-finite placement (origin or rotation) - re-place the \
                 offset before posting"
            ),
        };
        hits.push((entity, prefix, vec![label]));
    }
    hits.into_iter()
        .map(|(_, prefix, ops)| format!("{prefix} (operations: {})", ops.join(", ")))
        .collect()
}
