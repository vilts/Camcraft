//! Corner relief spur injection into pocket pass walks (#2400).
//!
//! Provenance: clean-room pass-splicing helper for [`super::corner_relief`]
//! over this repo's `Seg` / `chord_clears` primitives.

use crate::geo2d::{Point2, Region};

use super::arcs::Seg;
use super::corner_relief::{CornerRelief, CornerReliefPlan, ResolvedRelief, plan_corner_reliefs};
use super::face::GenError;
use super::pocket::non_sliver_center;
use super::slots::chord_clears;
use super::waterscan::chord_crosses_path;

#[inline]
fn dist(a: Point2, b: Point2) -> f64 {
    (a.x - b.x).hypot(a.y - b.y)
}

/// Splice corner relief spurs `[start, end, start]` into pocket passes (or runs).
///
/// A relief that gets no spur (no pass vertex at its start and no legal
/// detour) is a miss (#3249): its boundary corner is pushed onto
/// `misses` so the caller can name the corner in a note.
pub(super) fn inject_relief_spurs(
    passes: &mut [Vec<Point2>],
    runs: &mut Option<Vec<Vec<Seg>>>,
    center: &Region,
    reliefs: &[ResolvedRelief],
    misses: &mut Vec<Point2>,
) {
    let find_v = |passes: &[Vec<Point2>], target: Point2| -> Option<(usize, usize)> {
        let mut best: Option<(usize, usize, f64)> = None;
        for (pi, pass) in passes.iter().enumerate() {
            for (vi, &pt) in pass.iter().enumerate() {
                let d = dist(pt, target);
                if d <= 1e-4 && best.is_none_or(|(_, _, bd)| d < bd) {
                    best = Some((pi, vi, d));
                }
            }
        }
        best.map(|(pi, vi, _)| (pi, vi))
    };
    for r in reliefs {
        if let Some((pi, vi)) = find_v(passes, r.start) {
            let insert_vi = if vi == 0
                && passes[pi].len() > 2
                && dist(passes[pi][0], passes[pi][passes[pi].len() - 1]) <= 1e-4
            {
                passes[pi].len() - 1
            } else {
                vi
            };
            let anchor = passes[pi][insert_vi];
            passes[pi].insert(insert_vi + 1, anchor);
            passes[pi].insert(insert_vi + 1, r.end);
            if let Some(all_runs) = runs.as_mut()
                && let Some(segs) = all_runs.get_mut(pi)
            {
                if insert_vi == 0 {
                    segs.insert(0, Seg::Line(vec![r.end, anchor]));
                } else {
                    let mut inserted = false;
                    for si in 0..segs.len() {
                        match &mut segs[si] {
                            Seg::Line(pts) => {
                                if let Some(k) = pts.iter().position(|&pt| dist(pt, r.start) <= 1e-4) {
                                    pts.insert(k + 1, pts[k]);
                                    pts.insert(k + 1, r.end);
                                    inserted = true;
                                    break;
                                }
                            }
                            Seg::Arc { to, .. } => {
                                if dist(*to, r.start) <= 1e-4 {
                                    let anchor_arc = *to;
                                    segs.insert(si + 1, Seg::Line(vec![r.end, anchor_arc]));
                                    inserted = true;
                                    break;
                                }
                            }
                        }
                    }
                    if !inserted {
                        segs.push(Seg::Line(vec![r.end, anchor]));
                    }
                }
            }
            continue;
        }
        let mut cands: Vec<(usize, usize, f64)> = passes
            .iter()
            .enumerate()
            .flat_map(|(pi, p)| p.iter().enumerate().map(move |(vi, &q)| (pi, vi, dist(q, r.start))))
            .collect();
        cands.sort_by(|a, b| a.2.total_cmp(&b.2));
        if let Some((pi, vi, _)) = cands.into_iter().find(|&(pi, vi, _)| {
            let q = passes[pi][vi];
            chord_clears(center, q, r.start) && !passes.iter().any(|p| chord_crosses_path(q, r.start, p, false))
        }) {
            let q = passes[pi][vi];
            let detour = [r.start, r.end, r.start, q];
            for (k, &pt) in detour.iter().enumerate() {
                passes[pi].insert(vi + 1 + k, pt);
            }
            if let Some(all_runs) = runs.as_mut()
                && let Some(segs) = all_runs.get_mut(pi)
            {
                let mut inserted = false;
                for seg in segs.iter_mut() {
                    if let Seg::Line(pts) = seg
                        && let Some(k) = pts.iter().position(|&p| dist(p, q) <= 1e-6)
                    {
                        for (m, &pt) in detour.iter().enumerate() {
                            pts.insert(k + 1 + m, pt);
                        }
                        inserted = true;
                        break;
                    }
                }
                if !inserted {
                    segs.push(Seg::Line(detour.to_vec()));
                }
            }
        } else {
            misses.push(r.corner);
        }
    }
}

/// Plan inside-corner reliefs (#2400) against `region`'s cutter-center area for `tool_diameter`
/// and `stock_to_leave`, returning both resolved spurs and the relieved footprint.
pub fn pocket_relief_plan(
    region: &Region,
    tool_diameter: f64,
    stock_to_leave: f64,
    relief: &CornerRelief,
) -> Result<CornerReliefPlan, GenError> {
    relief.validate()?;
    if region.is_empty() {
        return Err(GenError::Empty("empty pocket region".into()));
    }
    let off = tool_diameter * 0.5 + stock_to_leave;
    let center = non_sliver_center(region, off)?;
    if center.is_empty() {
        return Err(GenError::Invalid("pocket is narrower than the tool".into()));
    }
    plan_corner_reliefs(region, &center, off, relief)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geo2d::Loop;

    /// #3249: a relief with no pass vertex at its start and no legal detour
    /// is reported in `misses`, not dropped silently.
    #[test]
    fn a_relief_no_pass_reaches_is_reported_as_a_miss() {
        let square = Loop::new(vec![
            Point2::new(0.0, 0.0),
            Point2::new(10.0, 0.0),
            Point2::new(10.0, 10.0),
            Point2::new(0.0, 10.0),
        ])
        .expect("square loop");
        let center = Region::new(vec![square]).expect("square region");
        // One pass far from the relief corner: no exact vertex and no chord
        // from it to the corner stays inside the center area.
        let mut passes = vec![vec![Point2::new(50.0, 50.0), Point2::new(50.0, 52.0)]];
        let mut runs: Option<Vec<Vec<Seg>>> = None;
        let reliefs = [ResolvedRelief {
            corner: Point2::new(0.0, 0.0),
            start: Point2::new(3.0, 3.0),
            end: Point2::new(2.0, 3.0),
            radius: 3.0,
        }];
        let mut misses = Vec::new();
        inject_relief_spurs(&mut passes, &mut runs, &center, &reliefs, &mut misses);
        assert_eq!(misses, vec![Point2::new(0.0, 0.0)], "the skipped relief is named");
        assert_eq!(passes[0].len(), 2, "no spur is spliced in");

        // The same relief with a pass vertex at its start gets its spur, and
        // reports no miss.
        let mut passes = vec![vec![Point2::new(5.0, 3.0), Point2::new(3.0, 3.0), Point2::new(3.0, 5.0)]];
        let mut misses = Vec::new();
        inject_relief_spurs(&mut passes, &mut None, &center, &reliefs, &mut misses);
        assert!(misses.is_empty(), "a reached relief is not a miss: {misses:?}");
        assert!(passes[0].iter().any(|p| *p == Point2::new(2.0, 3.0)), "the spur end is in the pass");
    }
}
