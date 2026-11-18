//! Provenance: Held's published zigzag pocketing model and our scan-row implementation (#1490).
//! Rest rows end inside the center boundary; closing rings remove wall cusps.

use crate::geo2d::{Point2, Region};
use super::face::{GenError, band};
use super::slots::chord_clears;
use super::waterscan::scan_row;

/// Rows followed by each rest area's closed boundary. A gap up to 0.01 mm
/// prevents rounded scan endpoints crossing the closing ring; the ring
/// cuts that gap. Limit it to a quarter step so tiny tools retain rows.
/// All passes stay inside the original center region.
pub(super) fn rest_passes(center: &Region, step: f64, climb: bool) -> Result<Vec<Vec<Point2>>, GenError> {
    let rows = center.offset(-0.01_f64.min(step * 0.25))?;
    let mut passes = if rows.is_empty() { Vec::new() } else { zigzag_passes(&rows, step, climb)? };
    for ring in center.loops() {
        let mut points = ring.points().to_vec();
        if !climb {
            points.reverse();
        }
        if let Some(&first) = points.first() {
            points.push(first);
        }
        passes.push(points);
    }
    Ok(passes)
}

/// Serpentine rows across the cutter-center area, each scanline split
/// where the even-odd parity says material stands (islands fall out of
/// the interval walk for free). Rows band [lo.y, hi.y] at `stepover` with
/// the extremes kept: a row on the area's own boundary is the pass whose
/// edge grazes the wall, and dropping it would leave the wall's last band
/// standing.
pub(super) fn zigzag_passes(center: &Region, stepover: f64, climb: bool) -> Result<Vec<Vec<Point2>>, GenError> {
    let (lo, hi) = match center.bounds() {
        Some(b) => b,
        None => return Err(GenError::Empty("empty pocket region".into())),
    };
    let ys = band(lo.y, hi.y, 0.0, stepover)?;
    let mut passes: Vec<Vec<Point2>> = Vec::new();
    let mut cur: Vec<Point2> = Vec::new();
    for (i, &y) in ys.iter().enumerate() {
        let segs = scan_row(center, y, y == hi.y);
        let left_to_right = climb == (i % 2 == 0);
        for k in 0..segs.len() {
            let (x0, x1) = segs[if left_to_right { k } else { segs.len() - 1 - k }];
            let (s, e) = if left_to_right {
                (Point2::new(x0, y), Point2::new(x1, y))
            } else {
                (Point2::new(x1, y), Point2::new(x0, y))
            };
            let join = match cur.last() {
                Some(&prev) => chord_clears(center, prev, s),
                None => false,
            };
            if join {
                // The corner matters: the connection feeds prev -> s (the
                // tested chord) and only then along the row to e. Pushing
                // only e would cut prev -> e, a diagonal the join test
                // never proved.
                cur.push(s);
                cur.push(e);
            } else {
                if cur.len() >= 2 {
                    passes.push(std::mem::take(&mut cur));
                }
                cur = vec![s, e];
            }
        }
    }
    if cur.len() >= 2 {
        passes.push(cur);
    }
    Ok(passes)
}
