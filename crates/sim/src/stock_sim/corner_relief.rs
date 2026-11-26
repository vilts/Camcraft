//! Program stock-sim credit for declared inside-corner reliefs (#2400).
//!
//! Provenance: re-judges declared corner-relief pockets using this
//! crate's shared [`super::edge_break::credit_floors`] walk; bounded to
//! the operation's tool radius, cut floor, and stock bounds.

use super::deviate::Deviation;
use super::dexel::Dexel;
use crate::geo2d::Point2;
use crate::geometry::Triangle;
use crate::verify::CornerReliefZone;

#[inline]
fn dist_pt_seg(p: Point2, a: Point2, b: Point2) -> f64 {
    let (dx, dy) = (b.x - a.x, b.y - a.y);
    let dd = dx * dx + dy * dy;
    let t = if dd <= 1e-18 {
        0.0
    } else {
        (((p.x - a.x) * dx + (p.y - a.y) * dy) / dd).clamp(0.0, 1.0)
    };
    (p.x - (a.x + t * dx)).hypot(p.y - (a.y + t * dy))
}

/// Re-judge `dev`'s gouge columns inside valid declared `reliefs`
/// against each relief's cut floor `z_bottom`. Never excuses cuts below
/// `z_bottom` or outside cutter-radius capsule `(start..end, radius)`.
pub fn credit_corner_reliefs(
    dev: &mut Deviation,
    dex: &Dexel,
    facets: &[Triangle],
    reliefs: &[CornerReliefZone],
) -> usize {
    if reliefs.is_empty() {
        return 0;
    }
    super::edge_break::credit_floors(dev, dex, facets, |ix, iy, p, tol| {
        let c = dex.center(ix, iy);
        let q = Point2::new(c.x, c.y);
        reliefs
            .iter()
            .filter(|rc| {
                p <= rc.z_top + tol
                    && dist_pt_seg(q, rc.start, rc.end) <= rc.radius + tol
            })
            .map(|rc| rc.z_bottom)
            .fold(f64::INFINITY, f64::min)
    })
}
