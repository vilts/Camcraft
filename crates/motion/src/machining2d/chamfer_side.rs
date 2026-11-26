//! The model winding that gives an open Chamfer chain an Inside or
//! Outside air side (#3153).
//!
//! Provenance: the selected model sections and CamCraft's normalized
//! Region winding; this is CamCraft's own safety classification.

use crate::geo2d::{Point2, Region};
use crate::geometry::Point3;

use super::contour_run_side::PROBE;
use super::face::GenError;

/// Whether the selected open boundary encloses air (a clockwise hole)
/// or stands outside material (a counter-clockwise material boundary).
/// The model's winding, not the picked chain's arbitrary direction,
/// gives Inside and Outside the same meaning on an open wall.
pub(super) fn boundary_inside(
    run: &[Point2],
    levels: &[(f64, Region)],
    air_left: bool,
) -> Result<Option<bool>, GenError> {
    let mut found = None;
    for (z, region) in levels {
        for w in run.windows(2) {
            let (a, b) = (w[0], w[1]);
            let dx = b.x - a.x;
            let dy = b.y - a.y;
            let len = dx.hypot(dy);
            if len <= 2.0 * PROBE {
                continue;
            }
            let (nx, ny) = (-dy / len, dx / len);
            let m = Point2::new((a.x + b.x) * 0.5, (a.y + b.y) * 0.5);
            let sign = if air_left { 1.0 } else { -1.0 };
            let air = Point2::new(m.x + nx * PROBE * sign, m.y + ny * PROBE * sign);
            let material = Point2::new(m.x - nx * PROBE * sign, m.y - ny * PROBE * sign);
            let mut crossed = None;
            for loop_ in region.loops() {
                let single = Region::new(vec![loop_.clone()])?;
                if single.contains(air) != single.contains(material) {
                    if crossed.is_some() {
                        return Err(GenError::invalid_at(
                            "the selected open chamfer edge touches more than one model boundary; re-pick a single wall",
                            Point3::new(m.x, m.y, *z),
                        ));
                    }
                    crossed = Some(!loop_.is_ccw());
                }
            }
            let Some(here) = crossed else {
                continue;
            };
            if found.is_some_and(|old| old != here) {
                return Err(GenError::invalid_at(
                    "the selected open chamfer run crosses both an Inside and an Outside wall; split the selection at the side change",
                    Point3::new(m.x, m.y, *z),
                ));
            }
            found = Some(here);
        }
    }
    Ok(found)
}
