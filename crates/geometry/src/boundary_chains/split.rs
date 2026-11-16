//! Open-chain splitting shared by single and multiple boundary chains.
use crate::geo2d::{GeoError, Loop, Point2, Region};

/// Domain failures, mapped to operation refusals by the application.
#[derive(Debug, thiserror::Error)]
pub enum BoundaryError {
    #[error("{0}")]
    Invalid(String),
    #[error("{0}")]
    Empty(String),
    #[error(transparent)]
    Geometry(#[from] GeoError),
}

/// Snap a turn to the exact quarter turns (the common straight edges),
/// so an axis-aligned chain rotates by exact signs, never by 1e-16.
pub fn snap_turn(a: f64) -> f64 {
    let q = a / std::f64::consts::FRAC_PI_2;
    if (q - q.round()).abs() < 1e-9 {
        let k = (q.round() as i64).rem_euclid(4);
        return k as f64 * std::f64::consts::FRAC_PI_2;
    }
    a
}

/// Close an open chain against a far circle on each side: the left
/// region (walking the chain from its lexicographically lower end) and
/// the right one, both clipped to `reach`, plus the chain's end-to-end
/// direction. The chain runs on straight past each end along its end
/// tangent - the textbook way to let an open curve split a plane.
pub fn sides_of_chain(pts: &[Point2], reach: &Region) -> Result<(Region, Region, f64), BoundaryError> {
    let mut p: Vec<Point2> = Vec::with_capacity(pts.len());
    for q in pts {
        if p.last().is_none_or(|l: &Point2| (l.x - q.x).hypot(l.y - q.y) > 1e-9) {
            p.push(*q);
        }
    }
    let (Some(&first), Some(&last)) = (p.first(), p.last()) else {
        return Err(BoundaryError::Invalid("the open boundary holds no points - re-pick it".into()));
    };
    if p.len() < 2 {
        return Err(BoundaryError::Invalid(
            "the open boundary is a single point - re-pick a real edge".into(),
        ));
    }
    if (last.x, last.y) < (first.x, first.y) {
        p.reverse();
    }
    let Some((lo, hi)) = reach.bounds() else {
        return Err(BoundaryError::Empty("the stock outline is empty".into()));
    };
    let (mut blo, mut bhi) = (lo, hi);
    for q in &p {
        blo = Point2::new(blo.x.min(q.x), blo.y.min(q.y));
        bhi = Point2::new(bhi.x.max(q.x), bhi.y.max(q.y));
    }
    let c = Point2::new((blo.x + bhi.x) * 0.5, (blo.y + bhi.y) * 0.5);
    let big = (bhi.x - blo.x).hypot(bhi.y - blo.y) + 10.0;
    let dir = |a: Point2, b: Point2| {
        let (dx, dy) = (a.x - b.x, a.y - b.y);
        let n = dx.hypot(dy);
        (dx / n, dy / n)
    };
    let n = p.len();
    let (sx, sy) = dir(p[0], p[1]);
    let (ex, ey) = dir(p[n - 1], p[n - 2]);
    let s = Point2::new(p[0].x + sx * 2.0 * big, p[0].y + sy * 2.0 * big);
    let e = Point2::new(p[n - 1].x + ex * 2.0 * big, p[n - 1].y + ey * 2.0 * big);
    let ang = |q: Point2| (q.y - c.y).atan2(q.x - c.x);
    let (ts, te) = (ang(s), ang(e));
    let tau = std::f64::consts::TAU;
    let side = |ccw: bool| -> Result<Region, BoundaryError> {
        // From the end's angle back to the start's, the long way round
        // the side we close.
        let mut sweep = if ccw { (ts - te).rem_euclid(tau) } else { -(te - ts).rem_euclid(tau) };
        if sweep.abs() < 1e-6 {
            sweep = if ccw { tau } else { -tau };
        }
        let mut ring: Vec<Point2> = Vec::with_capacity(n + 130);
        ring.push(s);
        ring.extend(p.iter().copied());
        ring.push(e);
        let steps = 128;
        for k in 0..=steps {
            let t = te + sweep * f64::from(k) / f64::from(steps);
            ring.push(Point2::new(c.x + 3.0 * big * t.cos(), c.y + 3.0 * big * t.sin()));
        }
        let lp = Loop::new(ring)?;
        let half = Region::new(vec![lp]).map_err(|_| {
            BoundaryError::Invalid(
                "the open boundary crosses itself once run on past its ends - pick a chain that splits the stock in two".into(),
            )
        })?;
        Ok(reach.intersect(&half)?)
    };
    let theta = (p[n - 1].y - p[0].y).atan2(p[n - 1].x - p[0].x);
    Ok((side(true)?, side(false)?, theta))
}
