//! The lifted plan's exact stock (issue #1169): the capsules its cut
//! pieces sweep, kept as segments, so a rapid's landing is proved on
//! what the posted moves leave and not on the planner's cells
//! (`raster`), where a wall strip thinner than a cell stands unseen
//! between two rows of cell centers. A disc is covered when every point
//! of it lies within the cutter radius, less [`FIT`], of a segment
//! already cut: proved square by square (a quadtree over the disc; the
//! distance to a segment set is 1-Lipschitz, so a square whose center
//! is within the bound less its half-diagonal is covered whole), and
//! refused where the proof does not close at [`MIN_SIDE`] - never
//! sampled. Point-to-segment distance and the uniform grid broad phase
//! are textbook computational geometry (O'Rourke, "Computational
//! Geometry in C"; de Berg et al., "Computational Geometry", quadtrees).
//! Ours, no vendor material.

use std::collections::HashMap;

use crate::geo2d::Point2;

use super::lift_split::seg_dist;

/// What the cut the verifier reads may stand off its planned chord
/// (mm): the adaptive arc fit (`arcs::ARC_FIT_TOL / 4`, 2.5 um) and the
/// chords the verifier flattens a posted arc to (its gouge tolerance,
/// 10 um, inside the arc); the output lattice (1e-6) is far under it.
pub(super) const FIT: f64 = 0.0125;

/// The landing disc proved is the cutter's less this (mm): the gouge
/// tolerance every disc reading of the mill laws grants the rim
/// (`verify::rapids`, the footprint of radius `r - gouge_tol`).
pub(super) const EDGE: f64 = 0.01;

/// The smallest square the proof divides to (mm, its side; half-diagonal
/// 0.7 um): what is not proved by then is refused. `FIT` is past `EDGE`,
/// so a landing ON an earlier cut's own path is not proved by that cut
/// alone - the stock about it must have been swept by its neighbours
/// too, as it is anywhere but along a wall.
const MIN_SIDE: f64 = 0.001;

/// Most squares one proof may open; past it the landing is refused.
const MAX_SQUARES: usize = 4_000_000;

/// The segments cut so far by a cutter of radius `r`.
#[derive(Clone, Debug)]
pub(super) struct Swept {
    r: f64,
    segs: Vec<(Point2, Point2)>,
    /// Segment indices by the `r`-sized buckets their boxes meet.
    grid: HashMap<(i64, i64), Vec<u32>>,
}

impl Swept {
    pub(super) fn new(r: f64) -> Self {
        Swept { r: if r.is_finite() && r > 0.0 { r } else { 0.0 }, segs: Vec::new(), grid: HashMap::new() }
    }

    fn bucket(&self, v: f64) -> i64 {
        // Saturating: a float past the i64 range clamps (`as`).
        (v / self.r).floor() as i64
    }

    /// The walk `pts` was cut at the floor (one point: where it stood).
    pub(super) fn cut(&mut self, pts: &[Point2]) {
        if self.r <= 0.0 {
            return;
        }
        let one = pts.first().map(|&p| [p, p]);
        let pairs = pts.windows(2).map(|w| (w[0], w[1])).chain(one.iter().filter(|_| pts.len() == 1).map(|w| (w[0], w[1])));
        for (a, b) in pairs {
            if !(a.x.is_finite() && a.y.is_finite() && b.x.is_finite() && b.y.is_finite()) {
                continue;
            }
            let Ok(id) = u32::try_from(self.segs.len()) else { return };
            let (x0, x1) = (self.bucket(a.x.min(b.x)), self.bucket(a.x.max(b.x)));
            let (y0, y1) = (self.bucket(a.y.min(b.y)), self.bucket(a.y.max(b.y)));
            // A chord across more buckets than any pocket holds is not
            // a cut this stock can index: left out (the proof then
            // refuses what only it would cover).
            if x1.saturating_sub(x0) > 4096 || y1.saturating_sub(y0) > 4096 {
                continue;
            }
            self.segs.push((a, b));
            for ix in x0..=x1 {
                for iy in y0..=y1 {
                    self.grid.entry((ix, iy)).or_default().push(id);
                }
            }
        }
    }

    /// Is a cut segment within `lim` (at most `r`) of `q`?
    fn near(&self, q: Point2, lim: f64) -> bool {
        let (bx, by) = (self.bucket(q.x), self.bucket(q.y));
        (-1..=1).any(|dx| {
            (-1..=1).any(|dy| {
                self.grid
                    .get(&(bx.saturating_add(dx), by.saturating_add(dy)))
                    .is_some_and(|ids| ids.iter().any(|&i| self.segs.get(i as usize).is_some_and(|s| seg_dist(q, s.0, s.1) <= lim)))
            })
        })
    }

    /// Is the whole disc of radius `rad` at `c` cut already - every
    /// point of it within `r - FIT` of a cut segment? Proved, or `false`.
    pub(super) fn covers(&self, c: Point2, rad: f64) -> bool {
        let lim = self.r - FIT;
        if !(c.x.is_finite() && c.y.is_finite() && rad.is_finite() && rad >= 0.0 && lim > 0.0) {
            return false;
        }
        let mut open = vec![(c, rad.max(0.5 * MIN_SIDE))];
        let mut spent = 0usize;
        while let Some((p, half)) = open.pop() {
            spent += 1;
            if spent > MAX_SQUARES {
                return false;
            }
            // Wholly off the disc: nothing of it to prove.
            let off = ((p.x - c.x).abs() - half).max(0.0).hypot(((p.y - c.y).abs() - half).max(0.0));
            if off > rad {
                continue;
            }
            let diag = half * std::f64::consts::SQRT_2;
            if lim > diag && self.near(p, lim - diag) {
                continue;
            }
            if 2.0 * half <= MIN_SIDE {
                return false;
            }
            let q = 0.5 * half;
            for (sx, sy) in [(-q, -q), (q, -q), (-q, q), (q, q)] {
                open.push((Point2::new(p.x + sx, p.y + sy), q));
            }
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::super::lift_link::{LinkLaw, Return};
    use super::super::lift_split::Piece;
    use super::super::raster::Raster;
    use super::*;
    use crate::geo2d::{Loop, Region};

    fn p(x: f64, y: f64) -> Point2 {
        Point2::new(x, y)
    }

    fn rect(x0: f64, y0: f64, x1: f64, y1: f64) -> Region {
        let Ok(l) = Loop::new(vec![p(x0, y0), p(x1, y0), p(x1, y1), p(x0, y1)]) else { panic!("loop") };
        let Ok(r) = Region::new(vec![l]) else { panic!("region") };
        r
    }

    #[test]
    fn a_disc_is_covered_on_the_cut_and_not_a_hair_past_it() {
        let mut s = Swept::new(5.0);
        assert!(!s.covers(p(10.0, 7.0), 4.99), "nothing cut: nothing covered");
        s.cut(&[p(10.0, 7.0), p(50.0, 7.0)]);
        // A disc inside the cut's capsule less `FIT` is covered ...
        assert!(s.covers(p(30.0, 7.0), 5.0 - 0.02));
        assert!(s.covers(p(30.0, 7.004), 5.0 - 0.02));
        // ... the footprint on the cut's own path is not, by that cut
        // alone (the verifier reads an arc's chords up to `FIT` inside),
        // and off the path its far rim stands on uncut stock.
        assert!(!s.covers(p(30.0, 7.0), 5.0 - EDGE));
        assert!(!s.covers(p(30.0, 7.04), 5.0 - EDGE));
        // A second pass takes the strip: covered between the two.
        s.cut(&[p(50.0, 7.075), p(10.0, 7.075)]);
        assert!(s.covers(p(30.0, 7.04), 5.0 - EDGE));
        assert!(!s.covers(p(30.0, 7.2), 5.0 - EDGE));
        // Off the pass's end: the disc reaches past the end cap.
        assert!(!s.covers(p(9.0, 7.04), 5.0 - EDGE));
        assert!(!s.covers(p(f64::NAN, 7.0), 4.0) && !s.covers(p(30.0, 7.0), f64::INFINITY));
    }

    /// Issue #1169, the narrow slot's landing: a 14 mm slot cleared by a
    /// D10 up to y = 8.96, the lap at y = 9.0 still owed a 0.04 mm strip
    /// along the wall - thinner than the planner's 0.083 mm cell and
    /// between two rows of cell centers, so the cells read a landing
    /// over it as clear (the slot of
    /// `adaptive_hsm::hsm_report_full_resolution` stood 0.075 mm). The
    /// return over the retract plane rapids down to its lift on the
    /// piece's lead (y = 8.98, the strip under its rim): refused on the
    /// swept stock, allowed once the lap has taken the strip.
    #[test]
    fn a_rapid_landing_on_a_sub_cell_wall_strip_is_refused() {
        let reach = rect(0.0, 0.0, 60.0, 14.0);
        let center = rect(5.0, 5.0, 55.0, 9.0);
        let (r, h) = (5.0, 10.0 / 120.0);
        let Ok(mut ras) = Raster::from_region(&reach, h, r) else { panic!("raster") };
        let rim = crate::quality::rim_for(10.0, ras.h);
        let cut = [p(5.0, 5.0), p(55.0, 5.0), p(55.0, 8.96), p(5.0, 8.96)];
        ras.walk(&cut, r, rim, 0.5);
        let mut swept = Swept::new(r);
        swept.cut(&cut);
        let lead: Vec<Point2> = (0..=10).map(|k| p(20.0 - 0.3 * k as f64, 8.98)).collect();
        let piece = Piece { pts: vec![p(17.0, 8.98), p(6.0, 8.98)], lead, tail: Vec::new(), fine: None };
        let land = p(17.9, 8.98);
        // The planner's cells read the landing as clear (the gap) ...
        assert!(ras.stock_clear(land, rim + ras.h), "the fixture no longer hides the strip from the cells");
        // ... the swept stock does not, and the return is refused.
        assert!(!swept.covers(land, r - EDGE));
        let bite = crate::load::deg_at_rim(0.005, 10.0, rim);
        let law = |swept| LinkLaw { ras: &ras, center: &center, links: &[], swept, r, rim, step: 0.5, bite, orbit: (p(0.0, 0.0), 0.0) };
        assert_eq!(law(&swept).high(&piece), None, "a rapid onto the wall strip");
        // The strip cut (the lap at y = 9 walked): the same return stands.
        let mut after = swept.clone();
        after.cut(&[p(55.0, 9.0), p(5.0, 9.0)]);
        let Some(Return { high: true, down, .. }) = law(&after).high(&piece) else { panic!("the landing on cut stock") };
        assert!(after.covers(down[0], r - EDGE));
        // The hop's landing takes a lead point the pieces swept before
        // one only the cells read clear; with none proved, the cells'.
        let landing = super::super::lift_link::landing;
        assert_eq!(landing(&ras, &swept, &piece, (r, rim)), piece.pts[0], "no point proved: the cells' choice");
        let mut bent = piece.clone();
        bent.lead.insert(0, p(20.0, 8.9));
        assert_eq!(landing(&ras, &swept, &bent, (r, rim)), p(20.0, 8.9), "the swept point of the lead");
        assert_eq!(landing(&ras, &after, &bent, (r, rim)), piece.pts[0], "the strip cut: the piece's own start");
    }
}
