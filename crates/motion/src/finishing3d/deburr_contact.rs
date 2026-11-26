//! Cone-to-face sizing from analytic line/plane intersection. Provenance:
//! Euclidean plane geometry; our own swept-cone construction, no vendor source.

use super::crease::{scale3, sub3};
use super::deburr::DeburrParams;
use super::deburr_size::BreakMode;
use crate::geometry::Point3;
use crate::machining2d::GenError;

/// Actual chord for one unit of tip drop. The cone swept along a straight
/// edge has flank slope sqrt(slope² - edge_slope²) across the horizontal
/// edge direction. Intersect that envelope with each inward face ray.
pub(super) fn contact_drop(
    e0: Point3,
    e1: Point3,
    face_a: Point3,
    face_b: Option<Point3>,
    p: &DeburrParams,
) -> Result<f64, GenError> {
    let delta = sub3(e1, e0);
    let length = delta.dot(delta).sqrt();
    let tangent = scale3(delta, 1.0 / length);
    let plan = delta.x.hypot(delta.y);
    let slope = 1.0 / (0.5 * p.tip_angle_deg.to_radians()).tan();
    let rise = delta.z / plan;
    let across_slope = (slope * slope - rise * rise).sqrt();
    let ray = |v: Point3| {
        let perpendicular = sub3(v, scale3(tangent, v.dot(tangent)));
        scale3(perpendicular, 1.0 / perpendicular.dot(perpendicular).sqrt())
    };
    let contact = |direction: Point3| {
        let transverse = (-delta.y * direction.x + delta.x * direction.y) / plan;
        let along = (delta.x * direction.x + delta.y * direction.y) / plan;
        let denominator = across_slope * transverse.abs() + rise * along - direction.z;
        (denominator.is_finite() && denominator > 1e-12)
            .then(|| scale3(direction, 1.0 / denominator))
    };
    let first = ray(sub3(face_a, e0));
    let a = contact(first);
    // An open rim supplies one face. Complete its stated right-angle
    // convention with a ray perpendicular to both the edge and that face.
    // Select the lower ray, so a vertical real face gets a horizontal
    // virtual face, and a horizontal real face gets a downward wall.
    let second = face_b.map_or_else(
        || {
            let perpendicular = Point3::new(
                tangent.y * first.z - tangent.z * first.y,
                tangent.z * first.x - tangent.x * first.z,
                tangent.x * first.y - tangent.y * first.x,
            );
            scale3(
                perpendicular,
                if perpendicular.z > 0.0 { -1.0 } else { 1.0 },
            )
        },
        |q| ray(sub3(q, e0)),
    );
    let b = contact(second);
    let unsupported = || {
        GenError::Invalid(
        "deburr: unsupported face orientation for cone contact - select edges whose two faces meet the cutter".into(),
    )
    };
    let (Some(a), Some(b)) = (a, b) else {
        return Err(unsupported());
    };
    let chord = sub3(b, a);
    let chord_len = chord.dot(chord).sqrt();
    let at = sub3(a, scale3(chord, a.dot(chord) / chord.dot(chord)));
    let depth = at.dot(at).sqrt();
    let drop = match p.sizing.mode {
        BreakMode::ConstantWidth => p.width / chord_len,
        BreakMode::ConstantDepth => p.sizing.depth / depth,
    };
    let max_drop = p.sizing.max_setback(p.width) * slope;
    if !drop.is_finite() || drop <= 0.0 || drop > max_drop * (1.0 + 1e-12) {
        return Err(GenError::Invalid(
            "deburr: unsupported face orientation exceeds the declared break bound".into(),
        ));
    }
    Ok(drop)
}
