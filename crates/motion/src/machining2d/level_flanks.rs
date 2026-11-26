//! The full-width law of the start-side plan (#3788): a cut takes the full
//! tool width where both of its flanks, 0.9 tool radii either side of the
//! walk, stand in material that no earlier cut removed. The same law as
//! the full-width meter of the op tests, so that each cut that the meter
//! finds is named on the path.
//!
//! Provenance: radial engagement of an end mill from its two flank
//! points (general machining practice; our own law). No vendor material
//! consulted.

use crate::geo2d::{Point2, Region};

use super::face::GenError;
use super::ring::dist;

/// How far (in tool radii) a flank point stands from the walk.
const FLANK: f64 = 0.9;

/// How far back (in tool radii of travel) the walk's own disc does not
/// yet count as cut: the disc that stands on the sample now.
const OWN_LAG: f64 = 2.5;

/// The material a level cuts: the stock footprint (all of it when none is
/// known) inside the area that the tool's disc may sweep.
pub(super) struct Material<'a> {
    stock: Option<&'a Region>,
    reach: Region,
}

impl<'a> Material<'a> {
    /// The material of `center` (the cutter-centre area) for a tool of
    /// radius `tool_r`.
    pub(super) fn new(center: &Region, stock: Option<&'a Region>, tool_r: f64) -> Result<Material<'a>, GenError> {
        Ok(Material { stock, reach: center.offset(tool_r)? })
    }

    fn holds(&self, q: Point2) -> bool {
        self.reach.contains(q) && self.stock.is_none_or(|s| s.contains(q))
    }
}

/// The first sample of the walk `samples` that cuts the full tool width:
/// both flank points stand in `material`, and no disc of an earlier cut
/// (`cut_near`) or of the walk's own samples more than [`OWN_LAG`] radii
/// back reaches them.
pub(super) fn full_width_at(
    samples: &[Point2],
    material: &Material<'_>,
    tool_r: f64,
    cut_near: impl Fn(Point2, f64) -> bool,
) -> Option<Point2> {
    let mut travel = Vec::with_capacity(samples.len());
    let mut s = 0.0;
    for (i, &p) in samples.iter().enumerate() {
        if i > 0 {
            s += dist(samples[i - 1], p);
        }
        travel.push(s);
    }
    for i in 1..samples.len() {
        let (a, b) = (samples[i - 1], samples[i]);
        let len = dist(a, b);
        if len.is_nan() || len <= 1e-9 {
            continue;
        }
        let (tx, ty) = ((b.x - a.x) / len, (b.y - a.y) / len);
        let f = FLANK * tool_r;
        let flanks = [Point2::new(b.x - ty * f, b.y + tx * f), Point2::new(b.x + ty * f, b.y - tx * f)];
        let before = travel[i] - OWN_LAG * tool_r;
        let own = |q: Point2| samples.iter().zip(&travel).take_while(|(_, t)| **t < before).any(|(&o, _)| dist(o, q) <= tool_r);
        let uncut = |q: Point2| material.holds(q) && !cut_near(q, tool_r) && !own(q);
        if flanks.iter().all(|&q| uncut(q)) {
            return Some(b);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::{Material, full_width_at};
    use crate::geo2d::{Loop, Point2, Region};

    fn square(h: f64) -> Region {
        let pts = vec![Point2::new(-h, -h), Point2::new(h, -h), Point2::new(h, h), Point2::new(-h, h)];
        Region::new(vec![Loop::new(pts).unwrap_or_else(|e| panic!("{e:?}"))]).unwrap_or_else(|e| panic!("{e:?}"))
    }

    fn line(y: f64) -> Vec<Point2> {
        (0..=40).map(|k| Point2::new(-10.0 + 0.5 * f64::from(k), y)).collect()
    }

    /// A walk through the middle of uncut stock is a slot; the same walk
    /// beside an earlier cut, or at the stock edge, is not.
    #[test]
    fn the_flanks_law_reads_a_slot_and_passes_a_side_cut() {
        let stock = square(20.0);
        let Ok(m) = Material::new(&square(25.0), Some(&stock), 4.0) else { panic!("material") };
        let slot = full_width_at(&line(0.0), &m, 4.0, |_, _| false);
        assert!(slot.is_some(), "a slot in the middle of the stock");
        let beside = |q: Point2, r: f64| line(6.0).iter().any(|c| (c.x - q.x).hypot(c.y - q.y) <= r);
        assert_eq!(full_width_at(&line(2.0), &m, 4.0, beside), None, "one flank is cut");
        assert_eq!(full_width_at(&line(19.0), &m, 4.0, |_, _| false), None, "one flank is in air");
    }
}
