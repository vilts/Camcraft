//! How a rest pass is reached (issue 2031): a stay-down feed from where
//! the last pass ended, or a retract and a plunge at a proved-clear
//! position (`entry`) with a feed lead onto the run.
//!
//! A run that starts on a wall is led in ALONG that wall, on the wall's
//! own vertices (`canon`), and a run of an area cut before is stretched
//! along the walls to where that area's earlier passes started and
//! ended: the way in, the way out and the link between them are then
//! the very chords walked before, and a retrace is no crossing where a
//! fresh chord a few microns off an earlier one would be. A closed run
//! is reached on the chord most nearly tangent to it.
//!
//! Provenance: tangential lead-in of peripheral milling (machining
//! handbooks; the roll-in of M. Otkur and I. Lazoglu, "Trochoidal
//! milling", Int. J. Machine Tools and Manufacture 47, 2007, read as
//! arriving along the cut, not square onto it). The candidates and
//! their order are ours.

use crate::geo2d::Point2;
use crate::machining2d::chord_clears;

use super::super::front::{dist, unit};
use super::canon::{Vtx, WallAt};
use super::peel::Peel;
use super::runs::Run;

/// A stay-down link is at most this many tool diameters long; past it
/// the pass is reached by a retract and a rapid.
const STAY_PER_D: f64 = 4.0;

/// A wall lead looks for a proved position at most this many tool
/// diameters back along the wall.
const WALK_PER_D: f64 = 8.0;

/// A run is stretched to an earlier pass's start or end at most this
/// many tool diameters along the wall (the same corner, the same mouth
/// of a neck - not the next one).
const ANCHOR_PER_D: f64 = 3.0;

/// Proved positions tried for one run.
const SPOTS: usize = 6;

/// One way to reach and cut a run.
pub(super) struct Lead {
    /// The feed, from where the tool stands or plunges.
    pub pts: Vec<Point2>,
    /// Reached by a retract and a plunge at `pts[0]`.
    pub hop: bool,
    /// How many points lead up to the run itself (every way of reaching
    /// a run ends in the same run).
    pub own: usize,
    /// The wall vertices the feed lands on and leaves from, if any.
    pub start: Option<WallAt>,
    pub end: Option<WallAt>,
}

/// Where an earlier pass of the depth landed on a wall, and the proved
/// position it plunged at (when it did).
#[derive(Clone, Copy, Debug)]
pub(super) struct Anchor {
    pub start: WallAt,
    pub entry: Option<Point2>,
}

impl Peel<'_> {
    /// The ways to reach and cut `run`, best first.
    pub(super) fn leads(&self, run: &Run) -> Vec<Lead> {
        let (r, center) = (self.lv.r, self.lv.center);
        let Some(&first) = run.v.first() else { return Vec::new() };
        let mut out: Vec<Lead> = Vec::new();
        let stay = STAY_PER_D * 2.0 * r;
        if run.closed {
            let body: Vec<Point2> = run.v.iter().map(|v| v.p).collect();
            let ring = |from: Point2| seat(&body, from, |q| chord_clears(center, from, q));
            let lead = |from: Point2, hop: bool| Lead { pts: std::iter::once(from).chain(ring(from)).collect(), hop, own: 1, start: None, end: None };
            if let Some(cur) = self.cur
                && ring(cur).first().is_some_and(|&s| dist(cur, s) <= stay && chord_clears(center, cur, s))
            {
                out.push(lead(cur, false));
            }
            let samples: Vec<Point2> = body.iter().step_by((body.len() / 16).max(1)).copied().collect();
            for e in self.entries.near(&samples, SPOTS) {
                if ring(e).first().is_some_and(|&s| chord_clears(center, e, s)) {
                    out.push(lead(e, true));
                }
            }
            return out;
        }
        // Stretched along the walls to where the area was entered and
        // left before.
        let (pre, anchor) = self.back_to_anchor(first);
        let mut v: Vec<Vtx> = pre;
        v.extend(run.v.iter().copied());
        if let Some(&last) = run.v.last() {
            v.extend(self.on_to_end(last));
        }
        let body: Vec<Point2> = v.iter().map(|q| q.p).collect();
        let (Some(&head), end) = (v.first(), v.last().and_then(|q| q.at)) else { return out };
        let with = |lead: Vec<Point2>, hop: bool, start: Option<WallAt>| Lead {
            own: lead.len(),
            pts: lead.into_iter().chain(body.iter().skip(1).copied()).collect(),
            hop,
            start,
            end,
        };
        if let Some(cur) = self.cur {
            // Landing further back along the wall where there is no
            // earlier landing to share.
            let backs: &[f64] = if anchor.is_some() || head.at.is_none() { &[0.0] } else { &[0.0, r, 2.0 * r, 4.0 * r] };
            for &back in backs {
                let (walk, at) = self.walk_back(head, back);
                let Some(&t) = walk.first() else { continue };
                if dist(cur, t) <= stay && chord_clears(center, cur, t) {
                    out.push(with(std::iter::once(cur).chain(walk).collect(), false, at));
                }
            }
        }
        // The plunge the area's earlier passes used, onto the same landing.
        if let Some(e) = anchor.and_then(|a| a.entry) {
            out.push(with(vec![e, head.p], true, head.at));
        }
        if let Some((walk, at)) = self.walk_to_proved(head) {
            out.push(with(walk, true, at));
        }
        // From a proved position a little back along the wall, so the
        // feed meets the wall at a shallow angle, never square onto it.
        if head.at.is_some() {
            for back in [2.0 * r, r, 4.0 * r] {
                let Some(&t) = self.walk_back(head, back).0.first() else { continue };
                for e in self.entries.near(&[t], 2) {
                    if chord_clears(center, e, head.p) {
                        out.push(with(vec![e, head.p], true, head.at));
                    }
                }
            }
        }
        for e in self.entries.near(&[head.p], SPOTS) {
            if dist(e, head.p) <= 1e-9 {
                out.push(with(vec![head.p], true, head.at));
            } else if chord_clears(center, e, head.p) {
                out.push(with(vec![e, head.p], true, head.at));
            }
        }
        out
    }

    /// The wall step that runs AGAINST the travel of a run.
    fn back_step(&self) -> isize {
        if self.lv.climb { -1 } else { 1 }
    }

    /// The wall vertices from the nearest earlier landing back along the
    /// wall up to (not including) `first`, in travel order, and that
    /// landing; nothing when `first` is off the wall or none is near.
    fn back_to_anchor(&self, first: Vtx) -> (Vec<Vtx>, Option<Anchor>) {
        let Some(mut at) = first.at else { return (Vec::new(), None) };
        let mut out: Vec<Vtx> = Vec::new();
        let (mut gone, mut p) = (0.0, first.p);
        let max = ANCHOR_PER_D * 2.0 * self.lv.r;
        while gone <= max && out.len() < 4096 {
            if let Some(a) = self.anchors.iter().find(|a| a.start == at) {
                out.reverse();
                return (out, Some(*a));
            }
            at = self.canon.next(at, self.back_step());
            let q = self.canon.point(at);
            gone += dist(p, q);
            p = q;
            out.push(Vtx { p: q, at: Some(at) });
        }
        (Vec::new(), None)
    }

    /// The wall vertices after `last` on to the nearest earlier end,
    /// in travel order; nothing when `last` is off the wall or none is
    /// near.
    fn on_to_end(&self, last: Vtx) -> Vec<Vtx> {
        let Some(mut at) = last.at else { return Vec::new() };
        let mut out: Vec<Vtx> = Vec::new();
        let (mut gone, mut p) = (0.0, last.p);
        let max = ANCHOR_PER_D * 2.0 * self.lv.r;
        while gone <= max && out.len() < 4096 {
            if self.ends.contains(&at) {
                return out;
            }
            at = self.canon.next(at, -self.back_step());
            let q = self.canon.point(at);
            gone += dist(p, q);
            p = q;
            out.push(Vtx { p: q, at: Some(at) });
        }
        Vec::new()
    }

    /// The wall vertices from at least `len` back along the wall up to
    /// `first`, in travel order (`first` alone when it is off the
    /// wall), and the wall vertex the walk starts on.
    fn walk_back(&self, first: Vtx, len: f64) -> (Vec<Point2>, Option<WallAt>) {
        let mut out = vec![first.p];
        let Some(mut at) = first.at else { return (out, None) };
        let mut gone = 0.0;
        while gone < len && out.len() < 4096 {
            at = self.canon.next(at, self.back_step());
            let p = self.canon.point(at);
            gone += dist(p, out[out.len() - 1]);
            out.push(p);
        }
        out.reverse();
        (out, Some(at))
    }

    /// The wall lead from the nearest proved position back along the
    /// wall up to `first`, in travel order; `None` when `first` is off
    /// the wall or no position within reach is proved.
    fn walk_to_proved(&self, first: Vtx) -> Option<(Vec<Point2>, Option<WallAt>)> {
        let mut at = first.at?;
        let mut out = vec![first.p];
        let mut gone = 0.0;
        let max = WALK_PER_D * 2.0 * self.lv.r;
        loop {
            let p = out[out.len() - 1];
            if self.entries.proved(p) {
                out.reverse();
                return Some((out, Some(at)));
            }
            if gone > max || out.len() >= 4096 {
                return None;
            }
            at = self.canon.next(at, self.back_step());
            let q = self.canon.point(at);
            gone += dist(p, q);
            out.push(q);
        }
    }
}

/// A closed ring (first point repeated last) turned to start at the
/// vertex the chord from `from` meets most nearly along the ring's own
/// heading, among the vertices `legal` allows; the nearest vertex when
/// none of the best is legal.
fn seat(ring: &[Point2], from: Point2, legal: impl Fn(Point2) -> bool) -> Vec<Point2> {
    let n = ring.len().saturating_sub(1);
    if n < 2 {
        return ring.to_vec();
    }
    let reach = ring.iter().map(|&q| dist(from, q)).fold(f64::INFINITY, f64::min).max(1e-9);
    // Along the heading is best; a long chord costs a little.
    let score = |k: usize| {
        let (h, c) = (unit(ring[k], ring[k + 1]), unit(from, ring[k]));
        h.x * c.x + h.y * c.y - 0.05 * dist(from, ring[k]) / reach
    };
    let mut order: Vec<usize> = (0..n).collect();
    order.sort_by(|&a, &b| score(b).total_cmp(&score(a)));
    let nearest = (0..n).min_by(|&a, &b| dist(from, ring[a]).total_cmp(&dist(from, ring[b]))).unwrap_or(0);
    let k = order.into_iter().take(8).find(|&k| legal(ring[k])).unwrap_or(nearest);
    (0..=n).map(|j| ring[(k + j) % n]).collect()
}
