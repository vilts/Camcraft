//! Provenance: our own blind-slot law (#1762), using the existing open-chain plane split.

use super::{BoundaryError, qkey, sides_of_chain};
use crate::geo2d::{Loop, Point2, Region};

/// Area, mm², under which a cap is empty.
const AREA_MIN: f64 = 1e-4;

/// A convex U-shaped cap fixes which side of the open run is the slot.
/// Parallel outward tangents extend the mouth without rounding a wall.
pub fn region(points: &[Point2], stock: &Region, extend: f64) -> Result<Region, BoundaryError> {
    let unclear = || {
        BoundaryError::Invalid(
        "the picked open run does not bound a clear blind slot - pick a convex U-shaped run with parallel side ends and one open side, or close the chain".into(),
    )
    };
    let mut p = points.to_vec();
    p.dedup_by(|a, b| qkey(a) == qkey(b));
    if p.len() < 4 || p.iter().any(|q| !q.x.is_finite() || !q.y.is_finite()) {
        return Err(unclear());
    }
    let n = p.len();
    let outward = |a: Point2, b: Point2| {
        let (x, y) = (a.x - b.x, a.y - b.y);
        let d = x.hypot(y);
        (x / d, y / d)
    };
    let (a, b) = (outward(p[0], p[1]), outward(p[n - 1], p[n - 2]));
    let (mx, my) = (p[n - 1].x - p[0].x, p[n - 1].y - p[0].y);
    let width = mx.hypot(my);
    if !width.is_finite()
        || width <= 1e-6
        || [a.0, a.1, b.0, b.1].iter().any(|v| !v.is_finite())
        || a.0 * b.0 + a.1 * b.1 < 1.0 - 1e-9
        || (a.0 * my - a.1 * mx).abs() < width * (1.0 - 1e-9)
    {
        return Err(unclear());
    }
    // Every edge of the chord-closed cap must support the entire cap.
    // This rejects folds, crossings and a run with more than one mouth.
    let cap_loop = Loop::new(p.clone())?;
    let sign = cap_loop.area().signum();
    if cap_loop.area().abs() < AREA_MIN
        || (0..n).any(|i| {
            let (s, t) = (p[i], p[(i + 1) % n]);
            p.iter()
                .any(|q| sign * ((t.x - s.x) * (q.y - s.y) - (t.y - s.y) * (q.x - s.x)) < -1e-6)
        })
    {
        return Err(unclear());
    }
    let cap = Region::new(vec![cap_loop])?;
    let reach = if extend > 0.0 {
        stock.offset_sharp(extend)?
    } else {
        stock.clone()
    };
    let seed = Point2::new(
        p.iter().map(|q| q.x).sum::<f64>() / n as f64,
        p.iter().map(|q| q.y).sum::<f64>() / n as f64,
    );
    let mouth = Point2::new((p[0].x + p[n - 1].x) * 0.5, (p[0].y + p[n - 1].y) * 0.5);
    let (left, right, _) = sides_of_chain(&p, &reach)?;
    let slot = match (left.contains(seed), right.contains(seed)) {
        (true, false) => left,
        (false, true) => right,
        _ => return Err(unclear()),
    };
    if slot.loops().len() != 1
        || !slot.contains(mouth)
        || cap.intersect(&slot)?.area().abs() < AREA_MIN
    {
        return Err(BoundaryError::Invalid(
            "the stock outline does not close the blind slot in one clear region - check the stock or pick a closed chain".into(),
        ));
    }
    Ok(slot)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rect(x0: f64, y0: f64, x1: f64, y1: f64) -> Region {
        Region::new(vec![
            Loop::new(vec![
                Point2::new(x0, y0),
                Point2::new(x1, y0),
                Point2::new(x1, y1),
                Point2::new(x0, y1),
            ])
            .expect("rectangle"),
        ])
        .expect("region")
    }

    fn u() -> Vec<Point2> {
        vec![
            Point2::new(-8.0, -20.0),
            Point2::new(-8.0, 8.0),
            Point2::new(8.0, 8.0),
            Point2::new(8.0, -20.0),
        ]
    }

    fn refusal(p: &[Point2]) -> String {
        region(p, &rect(-35.0, -35.0, 35.0, 35.0), 0.0)
            .expect_err("unclear slot")
            .to_string()
    }

    #[test]
    fn blind_slot_empty_and_nonfinite_runs_are_refused_by_name() {
        assert!(refusal(&[]).contains("does not bound a clear blind slot"));
        let mut p = u();
        p[1].x = f64::NAN;
        assert!(refusal(&p).contains("does not bound a clear blind slot"));
    }

    #[test]
    fn blind_slot_nonparallel_ends_and_skew_mouth_are_refused() {
        let mut p = u();
        p[3].x = 12.0;
        assert!(refusal(&p).contains("does not bound a clear blind slot"));
        let mut p = u();
        p[3].y = -15.0;
        assert!(refusal(&p).contains("does not bound a clear blind slot"));
    }

    #[test]
    fn blind_slot_folded_cap_is_refused() {
        let p = [
            Point2::new(-8.0, -20.0),
            Point2::new(-8.0, 8.0),
            Point2::new(0.0, 4.0),
            Point2::new(8.0, 8.0),
            Point2::new(8.0, -20.0),
        ];
        assert!(refusal(&p).contains("does not bound a clear blind slot"));
    }

    #[test]
    fn blind_slot_split_stock_is_refused() {
        let stock = rect(-35.0, -35.0, 35.0, 35.0)
            .difference(&rect(-36.0, -30.0, 36.0, -25.0))
            .expect("split stock");
        let err = region(&u(), &stock, 0.0)
            .expect_err("stock closes two pieces")
            .to_string();
        assert!(
            err.contains("does not close the blind slot in one clear region"),
            "{err}"
        );
    }

    #[test]
    fn blind_slot_stock_without_seed_cannot_choose_a_side() {
        let stock = rect(-35.0, -35.0, 35.0, -10.0);
        let err = region(&u(), &stock, 0.0)
            .expect_err("stock misses the cap seed")
            .to_string();
        assert!(err.contains("does not bound a clear blind slot"), "{err}");
    }
}
