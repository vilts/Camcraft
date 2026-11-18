//! Stock caps for the #2004 level law in the cutting frame.
//! Provenance: rigid-frame projection of box corners; our own implementation.

use crate::cl::TiltedFrame;
use crate::geometry::{Point3, Triangle};

/// The stock and material bounds after the full setup-to-cutting transform.
/// Upright operations keep the exact old reading. An indexed operation
/// projects every box corner onto its tool axis, including the origin shift.
pub fn stock_top_in_frame(
    part: &[Triangle],
    part_top: f64,
    stock: Option<(Point3, Point3)>,
    frame: Option<&TiltedFrame>,
) -> Option<f64> {
    let Some(f) = frame else {
        let top = part_top.max(stock.map_or(f64::NEG_INFINITY, |(_, hi)| hi.z));
        return top.is_finite().then_some(top);
    };
    let mut bounds: Option<(Point3, Point3)> = None;
    for p in part.iter().flat_map(|t| [t.a, t.b, t.c]) {
        let (lo, hi) = bounds.get_or_insert((p, p));
        lo.x = lo.x.min(p.x);
        lo.y = lo.y.min(p.y);
        lo.z = lo.z.min(p.z);
        hi.x = hi.x.max(p.x);
        hi.y = hi.y.max(p.y);
        hi.z = hi.z.max(p.z);
    }
    let top = stock
        .into_iter()
        .chain(bounds)
        .flat_map(|(lo, hi)| {
            (0..8).map(move |i| {
                f.z_local(Point3::new(
                    [lo.x, hi.x][(i >> 2) & 1],
                    [lo.y, hi.y][(i >> 1) & 1],
                    [lo.z, hi.z][i & 1],
                ))
            })
        })
        .fold(f64::NEG_INFINITY, f64::max);
    top.is_finite().then_some(top)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn indexed_material_bounds_supply_the_stock_cap_without_setup_stock() {
        let part = [Triangle::new(
            Point3::new(-40.0, -30.0, 0.0),
            Point3::new(40.0, 30.0, 20.0),
            Point3::new(40.0, -30.0, 0.0),
        )];
        let f = TiltedFrame::from_angles(Point3::new(3.0, 0.0, 2.0), 0.0, 90.0).expect("frame");
        let cap = stock_top_in_frame(&part, 20.0, None, Some(&f)).expect("part cap");
        assert!((cap - 37.0).abs() < 1e-9, "{cap}");
        assert_eq!(stock_top_in_frame(&part, 20.0, None, None), Some(20.0));
    }
}
