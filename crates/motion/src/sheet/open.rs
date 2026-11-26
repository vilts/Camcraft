//! The one-sided parallel of an open run (#1109, shared by #1883): the
//! kerf side of a cut-off line or slot stated as LEFT or RIGHT of the
//! run's travel, the path its mitred parallel at the kerf offset. Wire
//! EDM rides it from the stock's edge; waterjet rides it between pierce
//! leads.
//!
//! Provenance: plain 2D geometry (a one-sided polyline offset with
//! mitred joins), moved out of `wire_edm::open` without change of
//! behaviour. Our own code.

use crate::geo2d::Point2;
use crate::geometry::Point3;
use crate::machining2d::GenError;

use super::ring::{dist, unit};
use super::Voice;

/// The refusal at a point of the run, by name and place.
fn at(v: Voice, why: String, q: Point2, z: f64) -> GenError {
    GenError::invalid_at(format!("{}: {why}", v.who), Point3::new(q.x, q.y, z))
}

/// The run's one-sided parallel at signed offset `s` (positive: left of
/// travel), its joins mitred. A run of fewer than two points, a run
/// that doubles back, or a turn the cut's side folds over, is refused
/// by name.
pub fn offset(run: &[Point2], s: f64, z: f64, v: Voice) -> Result<Vec<Point2>, GenError> {
    let n = run.len();
    if n < 2 || run.windows(2).any(|w| dist(w[0], w[1]) <= 0.0) {
        return Err(v.invalid("the open run has no length - there is nothing to cut along".into()));
    }
    let normal = |i: usize| {
        let u = unit(run[i], run[i + 1]);
        Point2::new(-u.y, u.x)
    };
    let mut out = Vec::with_capacity(n);
    let n0 = normal(0);
    out.push(Point2::new(run[0].x + s * n0.x, run[0].y + s * n0.y));
    for (i, &q) in run.iter().enumerate().take(n - 1).skip(1) {
        let (a, b) = (normal(i - 1), normal(i));
        let c = 1.0 + a.x * b.x + a.y * b.y;
        if !(c.is_finite() && c > 1e-3) {
            return Err(at(
                v,
                format!("the open run doubles back on itself at X{:.3} Y{:.3} - the {} cannot turn back along its own kerf", q.x, q.y, v.tool),
                q,
                z,
            ));
        }
        out.push(Point2::new(q.x + s * (a.x + b.x) / c, q.y + s * (a.y + b.y) / c));
    }
    let nl = normal(n - 2);
    out.push(Point2::new(run[n - 1].x + s * nl.x, run[n - 1].y + s * nl.y));
    // The cut's side of a turn folds over itself when the turn is
    // tighter than the offset: the parallel then runs backwards.
    for i in 0..n - 1 {
        let (u, w) = (unit(run[i], run[i + 1]), (out[i + 1].x - out[i].x, out[i + 1].y - out[i].y));
        if u.x * w.0 + u.y * w.1 <= 0.0 {
            let q = run[i + 1];
            return Err(at(
                v,
                format!(
                    "a corner of the open run near X{:.3} Y{:.3} is tighter than the {} can follow on its {} side - the minimum inside radius is the kerf offset ({:.4} mm)",
                    q.x,
                    q.y,
                    v.tool,
                    if s > 0.0 { "left" } else { "right" },
                    s.abs()
                ),
                q,
                z,
            ));
        }
    }
    Ok(out)
}
