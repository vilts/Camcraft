//! The wire contour's two laws (#532), run before the path leaves the
//! generator:
//!
//! - **Cover law - the corner minimum** (#290 D8). The wire is a circle
//!   of the kerf offset's radius rolling along the profile on the waste
//!   side, so the tightest inside corner it can leave has that radius:
//!   wire radius plus spark gap. Wherever the profile turns tighter -
//!   a sharp inside corner, a fillet under the minimum, a slot or notch
//!   narrower than the kerf - the wire-centre path stands farther than
//!   the offset from the profile and material the drawing calls cut is
//!   left standing. Every point of the profile must therefore lie
//!   within the offset (plus [`COVER_TOL`]) of the path; the worst
//!   point that does not is refused by name, with its spot.
//! - **Kerf law.** No move - the leads included - brings the wire's
//!   centre nearer the profile than the offset (less [`PATH_TOL`]): the
//!   kerf never enters the product. The path is built to hold this; the
//!   law is its certificate, and a failure is refused, never trimmed.
//!
//! Provenance: plain 2D distance geometry over the rolling-circle
//! reading of a wire cut (see the module head). Our own code.

use crate::cl::Toolpath;
use crate::geo2d::Point2;
use crate::geometry::Point3;
use crate::machining2d::GenError;

use super::ring::{dist, pt_poly, pt_ring};
use super::{WireParams, COVER_TOL, PATH_TOL};

/// The most samples either law reads along the profile or the path.
const MAX_SAMPLES: f64 = 40_000.0;

/// The sampling step (mm) along a run of `total` mm for offset `k`:
/// half the offset, widened so the whole run takes at most
/// [`MAX_SAMPLES`] - vertices are always read, whatever the step.
fn step_for(k: f64, total: f64) -> f64 {
    (k * 0.5).max(total / MAX_SAMPLES).max(1e-4)
}

/// The segments of `pts` as index pairs: closing back to the first
/// point for a ring, not for an open run (#1109).
fn segs(n: usize, closed: bool) -> impl Iterator<Item = (usize, usize)> {
    let m = if closed { n } else { n.saturating_sub(1) };
    (0..m).map(move |i| (i, (i + 1) % n.max(1)))
}

fn perimeter(pts: &[Point2], closed: bool) -> f64 {
    segs(pts.len(), closed).map(|(i, j)| dist(pts[i], pts[j])).sum()
}

/// Distance from `q` to `pts`, a ring or an open run.
fn to(q: Point2, pts: &[Point2], closed: bool) -> f64 {
    if closed {
        pt_ring(q, pts)
    } else {
        pt_poly(q, pts)
    }
}

/// The corner minimum: every point of the profile `given` lies within
/// the kerf offset of the wire-centre `path_ring`.
pub(super) fn cover(given: &[Point2], path_ring: &[Point2], p: &WireParams) -> Result<(), GenError> {
    cover_run(given, true, path_ring, true, p)
}

/// [`cover`] over a ring or an open run (#1109): `given_closed` and
/// `path_closed` say which each polyline is.
pub(super) fn cover_run(
    given: &[Point2],
    given_closed: bool,
    path: &[Point2],
    path_closed: bool,
    p: &WireParams,
) -> Result<(), GenError> {
    let k = p.kerf_offset();
    let reach = k + COVER_TOL;
    let step = step_for(k, perimeter(given, given_closed));
    let mut worst: Option<(f64, Point2)> = None;
    for (i, j) in segs(given.len(), given_closed) {
        let (a, b) = (given[i], given[j]);
        let len = dist(a, b);
        let steps = (len / step).ceil().clamp(1.0, 65_536.0) as u32;
        for j in 0..steps {
            let f = f64::from(j) / f64::from(steps);
            let q = Point2::new(a.x + (b.x - a.x) * f, a.y + (b.y - a.y) * f);
            let d = to(q, path, path_closed);
            // A NaN distance is never "covered".
            if d.is_nan() {
                return Err(GenError::Invalid(
                    "wire contour: the profile's distance to the wire path is not a number - refused".into(),
                ));
            }
            if d > reach && worst.is_none_or(|(w, _)| d > w) {
                worst = Some((d, q));
            }
        }
    }
    let Some((d, q)) = worst else {
        return Ok(());
    };
    Err(GenError::invalid_at(
        format!(
            "wire contour: an inside corner or slot of the profile near X{:.3} Y{:.3} is tighter than the wire can cut on the {} side - the minimum inside radius is {k:.4} mm (wire radius {:.4} + spark gap {:.4}), and the kerf would leave {:.4} mm standing there; round it to at least that radius, or cut with a thinner wire",
            q.x,
            q.y,
            p.side.word(),
            p.wire_diameter * 0.5,
            p.gap,
            d - k
        ),
        Point3::new(q.x, q.y, p.plane_z),
    ))
}

/// The certificate: no move of `path` brings the wire's centre nearer
/// the profile `given` (a ring, or an open run when `!given_closed`,
/// #1109) than the kerf offset `k`.
pub(super) fn kerf(path: &Toolpath, given: &[Point2], given_closed: bool, k: f64, z: f64) -> Result<(), GenError> {
    let total: f64 = path.moves.iter().map(|m| (m.to.x - m.from.x).hypot(m.to.y - m.from.y)).sum();
    let step = step_for(k, total);
    let floor = k - PATH_TOL;
    for (i, m) in path.moves.iter().enumerate() {
        let (a, b) = (Point2::new(m.from.x, m.from.y), Point2::new(m.to.x, m.to.y));
        let steps = (dist(a, b) / step).ceil().clamp(1.0, 65_536.0) as u32;
        for j in 0..=steps {
            let f = f64::from(j) / f64::from(steps);
            let q = Point2::new(a.x + (b.x - a.x) * f, a.y + (b.y - a.y) * f);
            let d = to(q, given, given_closed);
            if d.is_nan() || d < floor {
                return Err(GenError::invalid_at(
                    format!(
                        "wire contour: move {i} would bring the wire within {d:.4} mm of the profile near X{:.3} Y{:.3} - under the kerf offset ({k:.4} mm), so the kerf would burn into the product; refused",
                        q.x, q.y
                    ),
                    Point3::new(q.x, q.y, z),
                ));
            }
        }
    }
    Ok(())
}
