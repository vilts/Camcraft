//! Issue #1950: the air-side stock ring plan (`air_rings`) never posts
//! a pass that crosses an earlier one. A lead-in that starts one load
//! width back stands on the last front's path, and a run that meets the
//! part wall cuts across the wall laps; each such chord is dropped and
//! the pass goes on as a new piece past it. The test is the plan's own
//! crossing law (`guard::crosses`, the verifier's law (a)), not a
//! weaker one; segments bucketed on a uniform grid. Ours.

use std::collections::HashMap;

use crate::geo2d::Point2;

use super::guard::crosses;

/// Grid cell edge (mm).
const CELL: f64 = 2.0;

/// The chords of the passes kept so far (each with the pass it is of),
/// bucketed by cell.
#[derive(Default)]
pub(super) struct Kept {
    segs: Vec<(Point2, Point2, usize)>,
    cells: HashMap<(i64, i64), Vec<usize>>,
}

/// A chord spanning more cells than this on an axis is not bucketed: it
/// reads as a crossing (`split` drops it), never as clear.
const MAX_SPAN: i64 = 4096;

/// The cells chord `ab` spans, or `None` (non-finite or too long).
fn cell_range(a: Point2, b: Point2) -> Option<(i64, i64, i64, i64)> {
    let f = |v: f64| {
        let c = (v / CELL).floor();
        (c.is_finite() && c.abs() < 1e12).then_some(c as i64)
    };
    let (x0, y0, x1, y1) = (f(a.x.min(b.x))?, f(a.y.min(b.y))?, f(a.x.max(b.x))?, f(a.y.max(b.y))?);
    (x1 - x0 <= MAX_SPAN && y1 - y0 <= MAX_SPAN).then_some((x0, y0, x1, y1))
}

impl Kept {
    /// Does chord `ab` cross a kept chord? A non-finite chord always does.
    fn hits(&self, a: Point2, b: Point2) -> bool {
        self.hits_but(a, b, None)
    }

    /// Does chord `ab` cross a kept chord of a pass other than `own`
    /// (`None`: of any pass)?
    fn hits_but(&self, a: Point2, b: Point2, own: Option<usize>) -> bool {
        let Some((x0, y0, x1, y1)) = cell_range(a, b) else { return true };
        for cx in x0..=x1 {
            for cy in y0..=y1 {
                let Some(ids) = self.cells.get(&(cx, cy)) else { continue };
                if ids.iter().filter_map(|&i| self.segs.get(i)).any(|&(c, d, w)| Some(w) != own && crosses(a, b, c, d)) {
                    return true;
                }
            }
        }
        false
    }

    /// Keep the chords of `pass`.
    pub(super) fn add(&mut self, pass: &[Point2]) {
        self.add_as(pass, usize::MAX);
    }

    /// Keep the chords of `pass` as pass `own`'s.
    pub(super) fn add_as(&mut self, pass: &[Point2], own: usize) {
        for s in pass.windows(2) {
            let Some((x0, y0, x1, y1)) = cell_range(s[0], s[1]) else { continue };
            let id = self.segs.len();
            self.segs.push((s[0], s[1], own));
            for cx in x0..=x1 {
                for cy in y0..=y1 {
                    self.cells.entry((cx, cy)).or_default().push(id);
                }
            }
        }
    }

    /// Does a chord of `pass` cross a kept chord of a pass other than
    /// `own`? A join that grows a pass by its lead is kept only if not.
    pub(super) fn crosses_but(&self, pass: &[Point2], own: usize) -> bool {
        pass.windows(2).any(|s| cell_range(s[0], s[1]).is_none() || self.hits_but(s[0], s[1], Some(own)))
    }

    /// Does a chord of `pass` cross a kept chord of any pass?
    pub(super) fn crosses_any(&self, pass: &[Point2]) -> bool {
        pass.windows(2).any(|s| self.hits(s[0], s[1]))
    }

    /// `pass` cut into the pieces whose chords cross no kept chord: each
    /// crossing chord is dropped, a piece of fewer than two points too.
    /// The pieces are not kept here (`add` each one that is posted).
    pub(super) fn split(&self, pass: &[Point2]) -> Vec<Vec<Point2>> {
        let mut out = Vec::new();
        let mut cur: Vec<Point2> = Vec::new();
        for s in pass.windows(2) {
            if self.hits(s[0], s[1]) {
                if cur.len() >= 2 {
                    out.push(std::mem::take(&mut cur));
                }
                cur.clear();
                continue;
            }
            if cur.is_empty() {
                cur.push(s[0]);
            }
            cur.push(s[1]);
        }
        if cur.len() >= 2 {
            out.push(cur);
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A pass across a kept one loses only the crossing chord; a pass
    /// beside it is whole; the pieces cross nothing kept.
    #[test]
    fn a_crossing_chord_is_dropped_and_the_pass_goes_on() {
        let mut k = Kept::default();
        k.add(&[Point2::new(-5.0, 0.0), Point2::new(5.0, 0.0)]);
        let across: Vec<Point2> = (0..=10).map(|i| Point2::new(0.3, -2.25 + 0.5 * i as f64)).collect();
        let pieces = k.split(&across);
        assert_eq!(pieces.len(), 2, "{pieces:?}");
        assert!(pieces.iter().all(|p| p.windows(2).all(|s| !k.hits(s[0], s[1]))));
        assert_eq!(pieces.iter().map(Vec::len).sum::<usize>(), across.len());
        let beside = vec![Point2::new(-5.0, 1.0), Point2::new(5.0, 1.0)];
        assert_eq!(k.split(&beside), vec![beside.clone()]);
        // A walk ending exactly on the kept chord touches, it does not cross.
        let touch = vec![Point2::new(0.0, 3.0), Point2::new(0.0, 0.0)];
        assert_eq!(k.split(&touch).len(), 1);
        assert!(k.split(&[Point2::new(f64::NAN, 0.0), Point2::new(0.0, 1.0)]).is_empty());
    }
}
