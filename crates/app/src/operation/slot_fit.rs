//! The slot-in-selection law (#1503): a slot authored by two endpoints
//! and a width, with a selection picked, must fit inside the picked
//! region or is refused by name with the numbers - never generated
//! across a wall into the model and reported OK. Shared by every
//! endpoint-and-width slot kind (the Trochoidal slot today; the plain
//! Slot can call it unchanged, #1504).
//!
//! Provenance: our own design - plane geometry only (the capsule of a
//! segment fits a polygonal region exactly when both ends lie inside
//! and no boundary edge comes nearer the segment than the half-width;
//! segment-to-segment distance from any computational-geometry text,
//! e.g. Ericson, "Real-Time Collision Detection", 5.1.9). No vendor
//! material.

use crate::geo2d::{Point2, Region};
use crate::machining2d::GenError;

/// Slack on the clearance, mm: a slot exactly as wide as its pocket
/// (the cutter tangent to both walls) still fits.
const FIT_TOL: f64 = 1e-4;

/// Refuse a slot whose capsule - centre line `a`-`b`, half-width
/// `width / 2`, the band both slot generators keep the cutter inside -
/// does not lie inside `region` (the operation's picked selection).
/// `kind` is the operation's label, said first in the refusal.
pub(crate) fn refuse_unfit_slot(kind: &str, a: Point2, b: Point2, width: f64, region: &Region) -> Result<(), GenError> {
    let half = 0.5 * width;
    if !(half.is_finite() && half > 0.0) {
        return Err(GenError::Invalid(format!("{kind}: a slot width of {width} mm cannot be fitted to its selection")));
    }
    let loops = region.loops();
    if loops.is_empty() {
        return Err(GenError::Invalid(format!("{kind}: the selection resolves to no region, so nothing confines the slot")));
    }
    for (name, p) in [("A", a), ("B", b)] {
        if !region.contains(p) {
            return Err(GenError::Invalid(format!(
                "{kind}: endpoint {name} ({:.3}, {:.3}) lies outside the selected region - the slot would cut past the selection into the model; move the endpoints inside the selection or change the selection",
                p.x, p.y
            )));
        }
    }
    let mut nearest: Option<(f64, Point2)> = None;
    for l in loops {
        let pts = l.points();
        for (i, &p) in pts.iter().enumerate() {
            let q = pts[(i + 1) % pts.len()];
            let (d, at) = seg_seg(a, b, p, q);
            if nearest.is_none_or(|(n, _)| d < n) {
                nearest = Some((d, at));
            }
        }
    }
    match nearest {
        Some((d, at)) if d + FIT_TOL < half => Err(GenError::Invalid(format!(
            "{kind}: the {width:.3} mm wide slot from A ({:.3}, {:.3}) to B ({:.3}, {:.3}) does not fit its selection - it needs {half:.3} mm clear each side of its centre line, but the selection's edge comes within {d:.3} mm at ({:.3}, {:.3}), so the cut would run {:.3} mm past the selection into the model; shorten or narrow the slot, or select the region it should cut",
            a.x,
            a.y,
            b.x,
            b.y,
            at.x,
            at.y,
            half - d
        ))),
        _ => Ok(()),
    }
}

/// Distance between segments `a`-`b` and `p`-`q`, and the point on
/// `p`-`q` where it is reached. Disjoint segments reach their minimum
/// at an endpoint of one of them; crossing segments are 0 apart.
fn seg_seg(a: Point2, b: Point2, p: Point2, q: Point2) -> (f64, Point2) {
    if crosses(a, b, p, q) {
        return (0.0, crossing(a, b, p, q).unwrap_or(p));
    }
    let candidates = [
        (point_seg(p, a, b), p),
        (point_seg(q, a, b), q),
        foot(a, p, q),
        foot(b, p, q),
    ];
    candidates.into_iter().fold((f64::INFINITY, p), |best, c| if c.0 < best.0 { c } else { best })
}

/// Distance from `x` to segment `p`-`q`, and the nearest point on it.
fn foot(x: Point2, p: Point2, q: Point2) -> (f64, Point2) {
    let (dx, dy) = (q.x - p.x, q.y - p.y);
    let len2 = dx * dx + dy * dy;
    let t = if len2 > 0.0 { (((x.x - p.x) * dx + (x.y - p.y) * dy) / len2).clamp(0.0, 1.0) } else { 0.0 };
    let at = Point2::new(p.x + t * dx, p.y + t * dy);
    ((x.x - at.x).hypot(x.y - at.y), at)
}

fn point_seg(x: Point2, p: Point2, q: Point2) -> f64 {
    foot(x, p, q).0
}

fn orient(a: Point2, b: Point2, c: Point2) -> f64 {
    (b.x - a.x) * (c.y - a.y) - (b.y - a.y) * (c.x - a.x)
}

/// A proper or touching crossing of the two segments.
fn crosses(a: Point2, b: Point2, p: Point2, q: Point2) -> bool {
    let (d1, d2) = (orient(p, q, a), orient(p, q, b));
    let (d3, d4) = (orient(a, b, p), orient(a, b, q));
    (d1 * d2 < 0.0) && (d3 * d4 < 0.0)
}

fn crossing(a: Point2, b: Point2, p: Point2, q: Point2) -> Option<Point2> {
    let (rx, ry) = (b.x - a.x, b.y - a.y);
    let (sx, sy) = (q.x - p.x, q.y - p.y);
    let den = rx * sy - ry * sx;
    if den.abs() < 1e-15 {
        return None;
    }
    let u = ((p.x - a.x) * ry - (p.y - a.y) * rx) / den;
    Some(Point2::new(p.x + u * sx, p.y + u * sy))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geo2d::Loop;

    fn rect(hx: f64, hy: f64) -> Region {
        let l = Loop::new(vec![
            Point2::new(-hx, -hy),
            Point2::new(hx, -hy),
            Point2::new(hx, hy),
            Point2::new(-hx, hy),
        ])
        .expect("loop");
        Region::new(vec![l]).expect("region")
    }

    #[test]
    fn a_slot_inside_fits_and_one_past_the_end_wall_is_refused_with_numbers() {
        let pocket = rect(25.0, 12.5);
        let (a, b) = (Point2::new(-15.0, 0.0), Point2::new(15.0, 0.0));
        assert!(refuse_unfit_slot("Trochoidal slot", a, b, 16.0, &pocket).is_ok());
        let (a2, b2) = (Point2::new(-12.5, 0.0), Point2::new(12.5, 0.0));
        assert!(refuse_unfit_slot("Trochoidal slot", a2, b2, 25.0, &pocket).is_ok(), "exactly the pocket's size still fits");
        let (a, b) = (Point2::new(-20.0, 0.0), Point2::new(20.0, 0.0));
        let e = refuse_unfit_slot("Trochoidal slot", a, b, 16.0, &pocket).expect_err("refused").to_string();
        assert!(e.contains("Trochoidal slot") && e.contains("8.000") && e.contains("5.000") && e.contains("3.000"), "{e}");
    }

    #[test]
    fn an_endpoint_outside_or_a_centre_line_across_a_hole_is_refused() {
        let pocket = rect(25.0, 12.5);
        let e = refuse_unfit_slot("Slot", Point2::new(-30.0, 0.0), Point2::new(0.0, 0.0), 4.0, &pocket).expect_err("out");
        assert!(e.to_string().contains("endpoint A"), "{e}");
        let ring = Region::new(vec![
            rect(25.0, 12.5).loops()[0].clone(),
            Loop::new(vec![Point2::new(-2.0, -2.0), Point2::new(-2.0, 2.0), Point2::new(2.0, 2.0), Point2::new(2.0, -2.0)]).expect("hole"),
        ])
        .expect("ring");
        assert!(refuse_unfit_slot("Slot", Point2::new(-20.0, 0.0), Point2::new(20.0, 0.0), 4.0, &ring).is_err());
    }

    // Near misses (#1503 review): 0.01 mm past a wall is refused and the
    // exact fit beside it passes, so FIT_TOL cannot loosen to 0.01 mm or
    // more without a red test.
    #[test]
    fn a_slot_a_hundredth_past_the_end_wall_is_refused_and_the_exact_fit_passes() {
        let pocket = rect(25.0, 12.5);
        // 16 mm wide: the cutter edge reaches x = 17 + 8 = 25, the wall.
        let exact = refuse_unfit_slot("Slot", Point2::new(-17.0, 0.0), Point2::new(17.0, 0.0), 16.0, &pocket);
        assert!(exact.is_ok(), "{exact:?}");
        let e = refuse_unfit_slot("Slot", Point2::new(-17.0, 0.0), Point2::new(17.01, 0.0), 16.0, &pocket).expect_err("0.01 mm past B's wall");
        assert!(e.to_string().contains("0.010"), "{e}");
    }

    #[test]
    fn a_slot_a_hundredth_too_wide_for_its_region_is_refused_and_the_exact_width_passes() {
        let pocket = rect(25.0, 12.5);
        let (a, b) = (Point2::new(-5.0, 0.0), Point2::new(5.0, 0.0));
        let exact = refuse_unfit_slot("Slot", a, b, 25.0, &pocket);
        assert!(exact.is_ok(), "{exact:?}");
        let e = refuse_unfit_slot("Slot", a, b, 25.02, &pocket).expect_err("0.01 mm too wide each side");
        assert!(e.to_string().contains("0.010"), "{e}");
    }

    #[test]
    fn a_centre_line_a_hundredth_too_close_to_a_hole_is_refused_and_the_exact_clearance_passes() {
        let ring = Region::new(vec![
            rect(25.0, 12.5).loops()[0].clone(),
            Loop::new(vec![Point2::new(-2.0, -2.0), Point2::new(-2.0, 2.0), Point2::new(2.0, 2.0), Point2::new(2.0, -2.0)]).expect("hole"),
        ])
        .expect("ring");
        // 4 mm wide above the 4 x 4 hole: y = 4 leaves exactly 2 mm.
        let exact = refuse_unfit_slot("Slot", Point2::new(-20.0, 4.0), Point2::new(20.0, 4.0), 4.0, &ring);
        assert!(exact.is_ok(), "{exact:?}");
        let e = refuse_unfit_slot("Slot", Point2::new(-20.0, 3.99), Point2::new(20.0, 3.99), 4.0, &ring).expect_err("0.01 mm into the hole");
        assert!(e.to_string().contains("0.010"), "{e}");
    }
}
