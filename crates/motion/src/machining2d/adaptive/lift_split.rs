//! The microlift plan's pieces (M11.4 C round 4, `lift`): a walk cut at
//! its air stretches into the runs that bite, each with the air walk
//! ahead of it (its lead, where a return may land further back), and
//! the order law between them - a piece waits for every earlier piece
//! within reach of its reading. Elementary polyline work; ours.

use crate::geo2d::Point2;

use super::front::{densify, dist, len};
use super::near::{SegIndex, foot};

/// Each piece keeps this much of the air either side (mm): its lead in
/// and out, where the return lands and leaves.
const PAD: f64 = 0.5;

/// Air shorter than this many tool radii is ridden at the floor (a lift
/// costs more than it saves).
const MIN_GAP_PER_R: f64 = 1.0;

/// The lead is sampled this finely (mm) for a landing further back.
const LEAD_STEP: f64 = 0.25;

/// A biting run of the walk and the air walk ahead of it (forward
/// order, ending at the run's first point; empty for the walk's first).
#[derive(Clone, Debug)]
pub(super) struct Piece {
    pub pts: Vec<Point2>,
    pub lead: Vec<Point2>,
    /// The air walk after it (forward order, from its last point; empty
    /// where the walk ends): where a return may climb along.
    pub tail: Vec<Point2>,
    /// The spiral's own chords when `pts` holds coarser ones (`lift`'s
    /// re-thinning), walked instead where those read worse or leave a
    /// sliver (`lift_coarse::cut`).
    pub fine: Option<Vec<Point2>>,
}

/// `walk` cut into its biting pieces by its per-segment `marks`: air
/// stretches (`marks <= bite`) at least [`MIN_GAP_PER_R`] radii long are
/// dropped, less [`PAD`] each side. `anchored`: the first piece starts
/// where the walk does (the entry ends there).
pub(super) fn split(walk: &[Point2], marks: &[f64], (r, bite): (f64, f64), anchored: bool) -> Vec<Piece> {
    let n = walk.len();
    let air = |i: usize| marks.get(i).is_some_and(|&m| m <= bite);
    let mut gaps: Vec<(usize, usize)> = Vec::new();
    let mut i = 0;
    while i + 1 < n {
        if !air(i) {
            i += 1;
            continue;
        }
        let i0 = i;
        while i + 1 < n && air(i) {
            i += 1;
        }
        // A leading gap of an unanchored walk is dropped whole.
        let lead_gap = i0 == 0 && !anchored;
        if (len(&walk[i0..=i]) >= MIN_GAP_PER_R * r && i0 > 0) || lead_gap {
            gaps.push((i0, i));
        }
    }
    let mut pieces = Vec::new();
    let mut from: (usize, Option<Point2>) = (0, None);
    let mut lead: Vec<Point2> = Vec::new();
    for &(g0, g1) in &gaps {
        // The piece runs on PAD into the gap; the next leaves PAD early.
        let (end_seg, end_pt) = if g0 == 0 { (0, walk[0]) } else { along(walk, g0, PAD, true) };
        let (beg_seg, beg_pt) = if g1 + 1 >= n { (g1, walk[g1]) } else { along(walk, g1, PAD, false) };
        // The gap's air from this piece's end on: its tail, the next one's
        // lead.
        let mut gap: Vec<Point2> = vec![if g0 == 0 { walk[0] } else { end_pt }];
        gap.extend_from_slice(&walk[(end_seg + 1).min(beg_seg + 1)..=beg_seg]);
        gap.push(beg_pt);
        let gap = densify(&dedup(gap), LEAD_STEP, false);
        if g0 > 0 {
            let mut pts: Vec<Point2> = from.1.into_iter().collect();
            pts.extend_from_slice(&walk[from.0..=end_seg]);
            pts.push(end_pt);
            push_piece(&mut pieces, pts, std::mem::take(&mut lead), gap.clone());
        }
        lead = gap;
        from = (beg_seg + 1, Some(beg_pt));
    }
    // A trailing gap is not walked at all.
    if from.0 < n {
        let mut pts: Vec<Point2> = from.1.into_iter().collect();
        pts.extend_from_slice(&walk[from.0..]);
        push_piece(&mut pieces, pts, lead, Vec::new());
    }
    pieces
}

fn dedup(mut pts: Vec<Point2>) -> Vec<Point2> {
    pts.dedup_by(|a, b| dist(*a, *b) <= 1e-9);
    pts
}

fn push_piece(pieces: &mut Vec<Piece>, pts: Vec<Point2>, lead: Vec<Point2>, tail: Vec<Point2>) {
    let pts = dedup(pts);
    if pts.len() >= 2 {
        pieces.push(Piece { pts, lead, tail, fine: None });
    }
}

/// The point `d` along the walk from vertex `v` (forward) or back from
/// it (`!fwd`), and the index of the segment it lies on.
fn along(walk: &[Point2], v: usize, d: f64, fwd: bool) -> (usize, Point2) {
    let mut left = d;
    let mut i = v;
    if fwd {
        while i + 1 < walk.len() {
            let l = dist(walk[i], walk[i + 1]);
            if l >= left {
                return (i, lerp(walk[i], walk[i + 1], left / l.max(1e-12)));
            }
            left -= l;
            i += 1;
        }
        (walk.len().saturating_sub(1), walk[walk.len().saturating_sub(1)])
    } else {
        while i > 0 {
            let l = dist(walk[i - 1], walk[i]);
            if l >= left {
                return (i - 1, lerp(walk[i], walk[i - 1], left / l.max(1e-12)));
            }
            left -= l;
            i -= 1;
        }
        (0, walk[0])
    }
}

fn lerp(a: Point2, b: Point2, t: f64) -> Point2 {
    Point2::new(a.x + (b.x - a.x) * t, a.y + (b.y - a.y) * t)
}

/// The longest chord a re-thinned piece takes (mm).
const TURN_CHORD_MAX: f64 = 5.0;

/// `fine` re-thinned to chords as long as the polyline lets them be: no
/// chord leaves it by more than `sag`, and no joint turns by more than
/// `max_turn` (radians) - the in-cut break law, held at every joint the
/// chords leave (a chord is taken only if the next fine segment can
/// still follow it). Where the fine polyline itself turns harder, its
/// own chord is kept. Ours.
pub(super) fn thin_turn(fine: &[Point2], max_turn: f64, sag: f64) -> Vec<Point2> {
    let fine = dedup(fine.to_vec());
    let n = fine.len();
    if n <= 2 {
        return fine;
    }
    let dir = |a: Point2, b: Point2| {
        let l = dist(a, b).max(1e-12);
        Point2::new((b.x - a.x) / l, (b.y - a.y) / l)
    };
    let turn = |u: Point2, v: Point2| (u.x * v.y - u.y * v.x).atan2(u.x * v.x + u.y * v.y).abs();
    let mut out = vec![fine[0]];
    let (mut k, mut prev): (usize, Option<Point2>) = (0, None);
    while k + 1 < n {
        let mut best = k + 1;
        for i in k + 2..n {
            let (a, b) = (fine[k], fine[i]);
            if dist(a, b) > TURN_CHORD_MAX {
                break;
            }
            let d = dir(a, b);
            if !((k + 1..i).all(|m| seg_dist(fine[m], a, b) <= sag) && prev.is_none_or(|p| turn(p, d) <= max_turn)) {
                break;
            }
            if i + 1 >= n || turn(d, dir(fine[i], fine[i + 1])) <= max_turn {
                best = i;
            }
        }
        out.push(fine[best]);
        prev = Some(dir(fine[k], fine[best]));
        k = best;
    }
    out
}

/// Distance from `p` to the segment `a b`.
pub(super) fn seg_dist(p: Point2, a: Point2, b: Point2) -> f64 {
    dist(p, foot(a, b, p))
}

/// The air a return may ride next to a piece (mm): the longest down
/// ramp `lift_link` tries, and its climb.
const LEAD_READ: f64 = 3.0;

/// The last `d` (mm) of `p`.
fn end_of(p: &[Point2], d: f64) -> Vec<Point2> {
    let mut k = p.len().saturating_sub(1);
    while k > 0 && len(&p[k..]) < d {
        k -= 1;
    }
    p[k.min(p.len())..].to_vec()
}

/// The first `d` (mm) of `p`.
fn start_of(p: &[Point2], d: f64) -> Vec<Point2> {
    let mut k = 0;
    while k + 1 < p.len() && len(&p[..=k]) < d {
        k += 1;
    }
    p[..(k + 1).min(p.len())].to_vec()
}

/// For every piece, the earlier pieces within `reach` of it (their cut
/// changes the stock it reads, or its cut the stock they read).
pub(super) fn depends(pieces: &[Piece], reach: f64) -> Vec<Vec<usize>> {
    // A piece reads the stock along itself and along the air a return
    // descends or climbs on next to it (its lead's end, its tail's start).
    let read = |p: &Piece| -> Vec<Point2> {
        let mut q = end_of(&p.lead, LEAD_READ);
        q.extend_from_slice(&p.pts);
        q.extend(start_of(&p.tail, LEAD_READ));
        densify(&q, 0.5, false)
    };
    let samples: Vec<Vec<Point2>> = pieces.iter().map(read).collect();
    let boxes: Vec<(Point2, Point2)> = samples.iter().map(|q| bounds(q)).collect();
    let index: Vec<SegIndex<'_>> = pieces.iter().map(|p| SegIndex::new(&p.pts, 1.0)).collect();
    let mut deps = vec![Vec::new(); pieces.len()];
    for j in 0..pieces.len() {
        for i in 0..j {
            let (a, b) = (boxes[i], boxes[j]);
            let gap = (a.0.x - b.1.x).max(b.0.x - a.1.x).max(a.0.y - b.1.y).max(b.0.y - a.1.y);
            if gap > reach {
                continue;
            }
            // Samples half a millimetre apart: a quarter of slack.
            if samples[j].iter().any(|&q| dist(index[i].nearest(q), q) < reach + 0.25) {
                deps[j].push(i);
            }
        }
    }
    deps
}

fn bounds(p: &[Point2]) -> (Point2, Point2) {
    let (mut lo, mut hi) = (Point2::new(f64::INFINITY, f64::INFINITY), Point2::new(f64::NEG_INFINITY, f64::NEG_INFINITY));
    for q in p {
        lo = Point2::new(lo.x.min(q.x), lo.y.min(q.y));
        hi = Point2::new(hi.x.max(q.x), hi.y.max(q.y));
    }
    (lo, hi)
}
