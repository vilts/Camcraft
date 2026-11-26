//! #2561: a tab against every cut of its level. `tab_bridge` fits each tab
//! between the two cuts beside it; the tab is a bridge only while no other
//! cut of the level (a later run of the same ring, or another ring) reaches
//! it. Plane geometry, segment-to-segment distance: a cut whose centre line
//! passes nearer than `D` to the tab's line on the ring cuts into the tab,
//! and one nearer than `W + D` leaves less than the tab width between the
//! two kerfs.

use std::collections::HashMap;

use crate::geo2d::{Loop, Point2};

use super::contour_params::ContourParams;
use super::contour_start::{seat, start_at};
use super::fillet::Legal;
use super::fillet_tally::FilletTally;
use super::ring::{Ring, dist, tab_window, walked};
use super::tab_bridge::{Cut, SLACK, TabFit, fit_tabs, held, keep_cut, pt_seg, seg_seg};
use super::tab_kerf::kerf_gap;

/// The tab fit of every loop of a band (`loops` index to fit), each ring
/// seated as its tabbed levels walk it. `levels[i]` holds the tabbed
/// levels (`f64::to_bits`) that loop `i` is cut at: a tab is held only
/// against the cuts of a level that it stands at. Empty when no loop
/// walks a tabbed level.
pub(super) fn fit_loops(
    loops: &[Loop],
    levels: &[Vec<u64>],
    pick: Option<(usize, Point2)>,
    p: &ContourParams,
    legal: &Legal,
) -> HashMap<usize, TabFit> {
    if p.tab_count == 0 || p.tab_height.is_nan() || p.tab_height <= 0.0 || levels.iter().all(Vec::is_empty) {
        return HashMap::new();
    }
    let mut scratch = Vec::new();
    let rings: Vec<Ring> = loops
        .iter()
        .enumerate()
        .map(|(li, l)| {
            let pts = walked(l.points(), p.climb == legal.inside, legal.inside);
            let picked = pick.filter(|&(pi, _)| pi == li).map(|(_, q)| q);
            Ring::new(seat(pts, picked, true, false, p, legal, &mut scratch))
        })
        .collect();
    fit_band(&rings, levels, pick.map(|(pi, _)| pi), p, legal).into_iter().enumerate().collect()
}

/// #3453: an untabbed level of a tabbed loop starts where the first cut of
/// its tabbed levels begins (`raw`, the ring as the tabbed levels seat it,
/// and its fit), so that the tabbed level enters where each level above
/// cut its lead-in - not one stepdown below the stock top in sheet that no
/// level cut. `seated`, the untabbed start, when the loop has no tab fit.
pub(super) fn level_start(seated: Vec<Point2>, raw: Vec<Point2>, fit: Option<&TabFit>) -> Vec<Point2> {
    let Some(&(k0, _)) = fit.and_then(|f| f.keeps.first()) else {
        return seated;
    };
    let ring = Ring::new(raw);
    if !(ring.len > 0.0 && ring.len.is_finite() && k0.is_finite()) {
        return seated;
    }
    start_at(&seated, ring.at(k0.rem_euclid(ring.len)))
}

/// Each ring's fit (`tab_bridge::fit_tabs`), then each tab against all the
/// other cuts of the level, as first fitted. A tab that another cut cuts
/// into holds the whole loop (its tabbed levels cut nothing), by name. A
/// tab that keeps its width but has less than that width of material
/// beside it gets a note. Only the rings that share a tabbed level with
/// ring `i` (`levels`) are read against its tabs: a ring that stops on a
/// shelf above the tabs cuts nothing beside them.
fn fit_band(rings: &[Ring], levels: &[Vec<u64>], picked: Option<usize>, p: &ContourParams, legal: &Legal) -> Vec<TabFit> {
    let mut fits: Vec<TabFit> = rings.iter().enumerate().map(|(i, r)| fit_tabs(r, p, picked == Some(i), legal)).collect();
    let need = tab_window(p.tab_width, p.tool_diameter);
    let d = need - p.tab_width;
    let cuts: Vec<Vec<((f64, f64), Cut)>> = rings.iter().zip(&fits).map(|(r, f)| ring_cuts(r, &f.keeps, p, legal)).collect();
    let lv = |k: usize| levels.get(k).map_or(&[][..], Vec::as_slice);
    for (i, ring) in rings.iter().enumerate() {
        let mine = lv(i);
        if mine.is_empty() {
            continue;
        }
        let shares: Vec<bool> = (0..rings.len()).map(|k| lv(k).iter().any(|z| mine.contains(z))).collect();
        let m = cuts[i].len();
        // The nearest other cut to any tab of this ring: (distance, tab).
        let mut worst: Option<(f64, Point2)> = None;
        for j in 0..m {
            let (from, to) = (cuts[i][j].0.1, cuts[i][(j + 1) % m].0.0);
            let line = ring.arc_between(from, from + (to - from).rem_euclid(ring.len));
            if line.len() < 2 {
                continue;
            }
            let gap = nearest_other(&cuts, &shares, i, j, &line, need, d);
            if worst.is_none_or(|w| gap < w.0) {
                worst = Some((gap, line[line.len() / 2]));
            }
        }
        let Some((gap, at)) = worst else { continue };
        if gap < d - SLACK {
            fits[i] = held(
                p,
                format!(
                    "another cut of this level passes {gap:.3} mm from the tab near ({:.3}, {:.3}) and cuts into it \
                     (a turn of the same path, or another loop)",
                    at.x, at.y
                ),
            );
        } else if gap < need - SLACK {
            fits[i].notes.push(format!(
                "tabs: the tab near ({:.3}, {:.3}) keeps its {:.3} mm, but another cut of this level leaves only \
                 {:.3} mm of material beside it",
                at.x,
                at.y,
                p.tab_width,
                gap - d
            ));
        }
    }
    fits
}

/// The flattened cuts of a ring's keeps, in ring order.
fn ring_cuts(ring: &Ring, keeps: &[(f64, f64)], p: &ContourParams, legal: &Legal) -> Vec<((f64, f64), Cut)> {
    let mut scratch = Vec::new();
    let mut tally = FilletTally::default();
    let mut cuts: Vec<((f64, f64), Cut)> =
        keeps.iter().filter_map(|&k| keep_cut(ring, k, p, legal, &mut scratch, &mut tally).map(|c| (k, Cut::of(&c)))).collect();
    cuts.sort_by(|x, y| x.0.0.rem_euclid(ring.len).total_cmp(&y.0.0.rem_euclid(ring.len)));
    cuts
}

/// The smallest distance from the tab line `line` (the tab after cut `j`
/// of ring `i`) to every other cut of the level. The two cuts beside the
/// tab count from where they have left it: `tab_bridge` measured their
/// first `need` of walk and their leads on the tab's side, and the rest
/// stays out until it is `need` away from the line for the first time, or
/// until it turns back toward the line. `shares[ri]`: ring `ri` is cut at
/// a tabbed level of ring `i`. The two cuts beside the tab are also read
/// against the tab's core (`tab_kerf`, second review of #2705): a run
/// beside the tab that does not come nearer is not a departure. That read
/// counts only when it cuts into the tab (under `d`, the tool diameter).
fn nearest_other(cuts: &[Vec<((f64, f64), Cut)>], shares: &[bool], i: usize, j: usize, line: &[Point2], need: f64, d: f64) -> f64 {
    let m = cuts[i].len();
    let mut best = f64::INFINITY;
    for (ri, ring) in cuts.iter().enumerate() {
        if ri != i && !shares.get(ri).copied().unwrap_or(false) {
            continue;
        }
        for (ci, (_, c)) in ring.iter().enumerate() {
            let before = ri == i && ci == j;
            let after = ri == i && ci == (j + 1) % m;
            if !(before || after) {
                best = best.min(near(line, &c.lead_in, need)).min(near(line, &c.walk, need)).min(near(line, &c.lead_out, need));
                continue;
            }
            let beside: Vec<Point2> = match (before, after) {
                (true, true) => c.walk.clone(),
                (false, true) => c.walk.iter().chain(&c.lead_out).copied().collect(),
                _ => c.lead_in.iter().chain(&c.walk).copied().collect(),
            };
            let k = kerf_gap(&beside, line, after, before, d);
            // NaN on either side reads as a touch (the loop is held).
            let lim = d - SLACK;
            if k < lim || k.is_nan() || lim.is_nan() {
                best = best.min(if k.is_nan() { 0.0 } else { k });
            }
            // One keep on both sides of the tab: `tab_bridge`'s reach.
            let reach = if m == 1 { need.min(c.len / 3.0) } else { need };
            let mut path = c.walk.clone();
            if after {
                if !before {
                    path.extend(c.lead_out.iter().copied());
                }
                path = departed(&path, line, reach, need);
            }
            if before {
                path.reverse();
                if !after {
                    path.extend(c.lead_in.iter().rev().copied());
                }
                path = departed(&path, line, reach, need);
            }
            best = best.min(near(line, &path, need));
            if before && after {
                // One keep (review of #2705): both of its leads join this
                // tab. They count where they turn back toward it.
                let lead_in: Vec<Point2> = c.lead_in.iter().rev().copied().collect();
                best = best.min(near(line, &departed(&lead_in, line, 0.0, need), need));
                best = best.min(near(line, &departed(&c.lead_out, line, 0.0, need), need));
            }
        }
    }
    best
}

/// `walk` (it starts at the tab) from where it has left the tab: past
/// `reach` of its length, then on to its first point `need` from `line`.
/// A walk that turns back toward `line` before that (a U-turn in a shallow
/// notch beside the tab, review of #2705) counts from the piece where it
/// turns: only the exit whose distance from the line keeps growing is
/// dropped.
fn departed(walk: &[Point2], line: &[Point2], reach: f64, need: f64) -> Vec<Point2> {
    let off = |q: Point2| line.windows(2).map(|w| pt_seg(q, w[0], w[1])).fold(f64::INFINITY, f64::min);
    // The nearest approach of a piece to the line; NaN reads as a touch.
    let low = |a: Point2, b: Point2| {
        line.windows(2).map(|w| seg_seg(w[0], w[1], a, b)).map(|s| if s.is_nan() { 0.0 } else { s }).fold(f64::INFINITY, f64::min)
    };
    let mut run = 0.0;
    let mut peak = walk.first().map_or(0.0, |&q| off(q));
    for (k, w) in walk.windows(2).enumerate() {
        let len = dist(w[0], w[1]);
        if low(w[0], w[1]) < peak - SLACK {
            // The walk comes back toward the tab on this piece.
            return walk[k..].to_vec();
        }
        if run + len > reach && off(w[1]) >= need {
            // The walk leaves on this piece: bisect for the point.
            let t0 = if len > 0.0 { ((reach - run) / len).clamp(0.0, 1.0) } else { 1.0 };
            let at = |t: f64| Point2::new(w[0].x + (w[1].x - w[0].x) * t, w[0].y + (w[1].y - w[0].y) * t);
            let (mut lo, mut hi) = (t0, 1.0);
            if off(at(lo)) >= need {
                hi = lo;
            }
            for _ in 0..32 {
                let mid = 0.5 * (lo + hi);
                if off(at(mid)) >= need { hi = mid } else { lo = mid }
            }
            let mut out = vec![at(hi)];
            out.extend_from_slice(&walk[k + 1..]);
            return out;
        }
        peak = peak.max(off(w[1]));
        run += len;
    }
    Vec::new()
}

/// The smallest distance from `line` to the polyline `pts`, read only
/// where it can be under `need` (a box test first: a long ring stays cheap).
fn near(line: &[Point2], pts: &[Point2], need: f64) -> f64 {
    let (mut x0, mut y0, mut x1, mut y1) = (f64::INFINITY, f64::INFINITY, f64::NEG_INFINITY, f64::NEG_INFINITY);
    for q in line {
        (x0, y0, x1, y1) = (x0.min(q.x - need), y0.min(q.y - need), x1.max(q.x + need), y1.max(q.y + need));
    }
    let mut best = f64::INFINITY;
    let mut piece = |a: Point2, b: Point2| {
        if a.x.max(b.x) < x0 || a.x.min(b.x) > x1 || a.y.max(b.y) < y0 || a.y.min(b.y) > y1 {
            return;
        }
        for w in line.windows(2) {
            // A NaN distance is not clear: it reads as a touch.
            let s = seg_seg(w[0], w[1], a, b);
            best = best.min(if s.is_nan() { 0.0 } else { s });
        }
    };
    match pts {
        [q] => piece(*q, *q),
        _ => pts.windows(2).for_each(|w| piece(w[0], w[1])),
    }
    best
}
