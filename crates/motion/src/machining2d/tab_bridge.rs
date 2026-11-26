//! #2480: tab windows grown until the true bridge holds - the material left
//! at a tab is narrowest where the cutter discs on its two sides come
//! closest, `|c1 - c2| - D` for two discs of diameter `D` (plane geometry,
//! segment-to-segment distance), so each tab window grows along the ring
//! until the cut on both sides (path ends and leads) clears `W + D`.

use std::collections::HashMap;

use crate::geo2d::Point2;

use super::arcs::{Seg, fit_arcs_sided};
use super::contour_entry::{flatten, seam_leads};
use super::contour_params::ContourParams;
use super::fillet::{Legal, fillet_corners};
use super::fillet_tally::{FilletTally, corner_arc_radius};
use super::lead_ext::extend_leads;
use super::ring::{Ring, dist, tab_window};

/// One keep's motion: the ring arc, its walk, the lead-in's start, the
/// lead-in and the lead-out.
pub(super) type KeepCut = (Vec<Point2>, Vec<Seg>, Point2, Vec<Seg>, Vec<Seg>);

/// The motion of the keep `(a, e)` of `ring`: its walk (arc-fitted, the
/// corners blended when asked) and both leads, each kept in the legal
/// room (lead_fit.rs) and extended (lead_ext.rs). `None` when the keep is
/// too short to walk.
pub(super) fn keep_cut(
    ring: &Ring,
    (a, e): (f64, f64),
    p: &ContourParams,
    legal: &Legal,
    notes: &mut Vec<String>,
    tally: &mut FilletTally,
) -> Option<KeepCut> {
    let poly = ring.arc_between(a, e);
    if poly.len() < 2 {
        return None;
    }
    let mut walk = fit_arcs_sided(&poly, p.tolerance, legal.air_left);
    if p.blend_turns {
        // #1759: the radius asked for; `validate` held it to the door,
        // and a radius the door refuses rounds nothing here.
        let rho = corner_arc_radius(p.blend_radius, p.tool_diameter).unwrap_or(0.0);
        let (w, t) = fillet_corners(&walk, poly[0], rho, p.tolerance, legal);
        walk = w;
        tally.add(t);
    }
    let (entry, lead, lead_off) = seam_leads(&walk, &poly, p, legal, notes);
    let (entry, lead, lead_off) = extend_leads(entry, lead, lead_off, &walk, &poly, p, legal, notes);
    Some((poly, walk, entry, lead, lead_off))
}

/// The keeps of a tabbed ring, and its notes: why no keep is left, or
/// what a kept tab does not do as asked.
#[derive(Clone)]
pub(super) struct TabFit {
    pub keeps: Vec<(f64, f64)>,
    pub notes: Vec<String>,
}

/// The fitted keeps of ring `li` (computed once per ring: every tabbed
/// level walks the same ring), with its notes said.
pub(super) fn cached(
    fits: &mut HashMap<usize, TabFit>,
    li: usize,
    fit: impl FnOnce() -> TabFit,
    notes: &mut Vec<String>,
) -> Vec<(f64, f64)> {
    let f = fits.entry(li).or_insert_with(fit);
    for n in &f.notes {
        if !notes.contains(n) {
            notes.push(n.clone());
        }
    }
    f.keeps.clone()
}

/// Rounds of growth before a tab is named as not fitting.
const ROUNDS: usize = 64;
/// Numeric slack on the bridge (mm): far under any tab width.
pub(super) const SLACK: f64 = 1e-6;

/// The keeps of a tabbed level. The `tab_count` windows start at
/// `tab_width + D` on the ring (centred at equal spacing, or ending on the
/// spacing marks from a picked start, #1473, which then grow backward only
/// so every level still enters at the pick). Each round builds every
/// keep's motion and measures each bridge: the smallest distance from the
/// end of the cut before a tab (the last `W + D` of its walk, and its
/// lead-out) to the start of the cut after it (its lead-in, and the first
/// `W + D` of its walk). A bridge under `W + D` grows the windows in that
/// gap by the deficit. Windows that fill the ring keep nothing (the part
/// stays held) with a note; a bridge that never fits is named the same way.
pub(super) fn fit_tabs(ring: &Ring, p: &ContourParams, picked: bool, legal: &Legal) -> TabFit {
    let l = ring.len;
    let n = p.tab_count as usize;
    let need = tab_window(p.tab_width, p.tool_diameter);
    if n == 0 || !(l > 0.0 && l.is_finite()) {
        return TabFit { keeps: vec![(0.0, l.max(0.0))], notes: Vec::new() };
    }
    if !(need > 0.0 && need.is_finite()) {
        return held(p, "the tab width is not a length".into());
    }
    // The windows alone fill the loop: name it before one entry per tab is
    // reserved, so a huge count from the app or MCP stays cheap.
    if n as f64 * need >= l {
        return fill(p);
    }
    let s = l / n as f64;
    // (anchor, extent before it, extent after it, share of growth before)
    let mut tabs: Vec<(f64, f64, f64, f64)> = (0..n)
        .map(|i| {
            if picked {
                ((i + 1) as f64 * s, need, 0.0, 1.0)
            } else {
                ((i as f64 + 0.5) * s, need * 0.5, need * 0.5, 0.5)
            }
        })
        .collect();
    let mut worst = None;
    for _ in 0..ROUNDS {
        let windows: Vec<(f64, f64)> = tabs.iter().map(|t| (t.0 - t.1, t.0 + t.2)).collect();
        let keeps = ring.keeps_from_windows(&windows);
        let gaps = short_gaps(ring, &keeps, p, legal, need);
        let Some(gaps) = gaps else {
            return fill(p);
        };
        if gaps.is_empty() {
            let notes = moved_start(ring, &keeps, p, picked).into_iter().collect();
            return TabFit { keeps, notes };
        }
        for (from, to, deficit) in gaps {
            let gap = (to - from).rem_euclid(l);
            let grow = deficit.max(need * 0.01);
            let mut any = false;
            for t in tabs.iter_mut() {
                let mid = t.0 + (t.2 - t.1) * 0.5;
                if (mid - from).rem_euclid(l) <= gap + l * 1e-9 {
                    t.1 += grow * t.3;
                    t.2 += grow * (1.0 - t.3);
                    any = true;
                }
            }
            worst = Some(ring.at(from + gap * 0.5));
            if !any {
                return no_fit(p, worst);
            }
        }
    }
    no_fit(p, worst)
}

/// Each gap between consecutive keeps whose bridge is under `need`:
/// `(end of the keep before, start of the keep after, deficit)`. `None`
/// when no keep cuts anything (the windows fill the ring).
fn short_gaps(ring: &Ring, keeps: &[(f64, f64)], p: &ContourParams, legal: &Legal, need: f64) -> Option<Vec<(f64, f64, f64)>> {
    let l = ring.len;
    let mut scratch = Vec::new();
    let mut tally = FilletTally::default();
    let mut cuts: Vec<((f64, f64), Cut)> = keeps
        .iter()
        .filter_map(|&k| keep_cut(ring, k, p, legal, &mut scratch, &mut tally).map(|c| (k, Cut::of(&c))))
        .collect();
    if cuts.is_empty() {
        return None;
    }
    cuts.sort_by(|x, y| x.0.0.rem_euclid(l).total_cmp(&y.0.0.rem_euclid(l)));
    let m = cuts.len();
    let mut out = Vec::new();
    for j in 0..m {
        let (ka, a) = &cuts[j];
        let (kb, b) = &cuts[(j + 1) % m];
        // One keep on both sides of the gap: its head and tail stay a
        // third of its walk apart, so its own middle never reads as a
        // bridge.
        let reach = if m == 1 { need.min(a.len / 3.0) } else { need };
        let gap = poly_dist(&a.tail(reach), &b.head(reach));
        // A NaN distance is not wide enough: it grows, or ends in no_fit.
        if gap.is_nan() || gap < need - SLACK {
            out.push((ka.1, kb.0, need - gap));
        }
    }
    Some(out)
}

/// One keep's cut flattened: lead-in, walk, lead-out.
pub(super) struct Cut {
    pub lead_in: Vec<Point2>,
    pub walk: Vec<Point2>,
    pub lead_out: Vec<Point2>,
    pub len: f64,
}

impl Cut {
    pub(super) fn of((poly, walk, entry, lead, lead_off): &KeepCut) -> Cut {
        let start = poly.first().copied().unwrap_or(*entry);
        let walk = flatten(start, walk);
        let end = walk.last().copied().unwrap_or(start);
        let len = walk.windows(2).map(|w| dist(w[0], w[1])).sum();
        Cut { lead_in: flatten(*entry, lead), walk, lead_out: flatten(end, lead_off), len }
    }

    /// The lead-in and the first `reach` of the walk.
    fn head(&self, reach: f64) -> Vec<Point2> {
        let mut out = self.lead_in.clone();
        out.extend(prefix(&self.walk, reach));
        out
    }

    /// The last `reach` of the walk and the lead-out.
    fn tail(&self, reach: f64) -> Vec<Point2> {
        let rev: Vec<Point2> = self.walk.iter().rev().copied().collect();
        let mut out: Vec<Point2> = prefix(&rev, reach).into_iter().rev().collect();
        out.extend(self.lead_out.iter().copied());
        out
    }
}

/// The polyline `pts` cut off at arc length `reach`.
fn prefix(pts: &[Point2], reach: f64) -> Vec<Point2> {
    let mut out: Vec<Point2> = pts.first().copied().into_iter().collect();
    let mut run = 0.0;
    for w in pts.windows(2) {
        let d = dist(w[0], w[1]);
        if run + d >= reach {
            let t = if d > 0.0 { ((reach - run) / d).clamp(0.0, 1.0) } else { 0.0 };
            out.push(Point2::new(w[0].x + (w[1].x - w[0].x) * t, w[0].y + (w[1].y - w[0].y) * t));
            return out;
        }
        run += d;
        out.push(w[1]);
    }
    out
}

/// The smallest distance between two polylines (a lone point counts).
pub(super) fn poly_dist(a: &[Point2], b: &[Point2]) -> f64 {
    let segs = |v: &[Point2]| -> Vec<(Point2, Point2)> {
        match v.len() {
            0 => Vec::new(),
            1 => vec![(v[0], v[0])],
            _ => v.windows(2).map(|w| (w[0], w[1])).collect(),
        }
    };
    let (sa, sb) = (segs(a), segs(b));
    let mut best = f64::INFINITY;
    for &(p0, p1) in &sa {
        for &(q0, q1) in &sb {
            best = best.min(seg_seg(p0, p1, q0, q1));
        }
    }
    best
}

fn orient(a: Point2, b: Point2, c: Point2) -> f64 {
    (b.x - a.x) * (c.y - a.y) - (b.y - a.y) * (c.x - a.x)
}

pub(super) fn pt_seg(p: Point2, a: Point2, b: Point2) -> f64 {
    let (dx, dy) = (b.x - a.x, b.y - a.y);
    let ll = dx * dx + dy * dy;
    let t = if ll > 0.0 { (((p.x - a.x) * dx + (p.y - a.y) * dy) / ll).clamp(0.0, 1.0) } else { 0.0 };
    dist(p, Point2::new(a.x + t * dx, a.y + t * dy))
}

pub(super) fn seg_seg(p0: Point2, p1: Point2, q0: Point2, q1: Point2) -> f64 {
    let crossing = orient(p0, p1, q0) * orient(p0, p1, q1) < 0.0 && orient(q0, q1, p0) * orient(q0, q1, p1) < 0.0;
    if crossing {
        return 0.0;
    }
    pt_seg(p0, q0, q1).min(pt_seg(p1, q0, q1)).min(pt_seg(q0, p0, p1)).min(pt_seg(q1, p0, p1))
}

pub(super) fn held(p: &ContourParams, why: String) -> TabFit {
    TabFit {
        keeps: Vec::new(),
        notes: vec![format!("tabs: {} tabs of {:.3} mm - {why}; the tabbed levels cut nothing on this loop", p.tab_count, p.tab_width)],
    }
}

/// #2561: from a picked start the windows grow backward only, so the
/// first cut of each tabbed level stays at the pick. A window that grows
/// over the whole first cut takes the pick into a tab: the level then
/// starts after that tab, and this note says so.
fn moved_start(ring: &Ring, keeps: &[(f64, f64)], p: &ContourParams, picked: bool) -> Option<String> {
    let first = keeps.first()?.0.rem_euclid(ring.len);
    if !picked || first.min(ring.len - first) <= ring.len * 1e-9 {
        return None;
    }
    let (pick, at) = (ring.at(0.0), ring.at(first));
    Some(format!(
        "tabs: the first {:.3} mm tab after the start point ({:.3}, {:.3}) grew back over it to keep its width; \
         the tabbed levels start at ({:.3}, {:.3}) instead",
        p.tab_width, pick.x, pick.y, at.x, at.y
    ))
}

/// The windows (tab width, tool diameter, and the room the leads take)
/// fill the whole loop.
fn fill(p: &ContourParams) -> TabFit {
    held(p, "with the tool diameter and the leads they fill the whole loop".into())
}

fn no_fit(p: &ContourParams, at: Option<Point2>) -> TabFit {
    let at = at.map(|q| format!(" near ({:.3}, {:.3})", q.x, q.y)).unwrap_or_default();
    held(p, format!("no bridge of the tab width fits{at} (the path turns back on itself there, or the leads reach across the tab)"))
}
