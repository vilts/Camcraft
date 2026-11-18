//! Provenance: crossing search over closed sketch loops. Candidate pairs come from a sweep over
//! the bounding boxes sorted on one axis ("sweep and prune": Cohen, Lin, Manocha and Ponamgi,
//! "I-COLLIDE", Symposium on Interactive 3D Graphics, 1995; the sweep itself is Shamos and
//! Hoey, "Geometric intersection problems", FOCS 1976); each pair is then tested exactly by
//! `intersect`. The pair count is budgeted, so a hostile drawing is refused, not hung on.
//! Original implementation.

use super::curve::{Piece, dist, same};
use super::intersect::contacts;
use super::{Budget, ChainError};
use crate::geo2d::Point2;

const NO_JOINT: usize = usize::MAX;

/// A closed loop made watertight: its pieces in order, with a straight bridge over every
/// joint gap. Each element knows the joints it ends at, so contact there is not a crossing.
#[derive(Clone, Debug)]
pub(super) struct Sealed {
    pub pieces: Vec<Piece>,
    /// Per element of `pieces`: the joint at its start and at its end.
    joints: Vec<(usize, usize)>,
    /// Per joint: the end of the piece before it and the start of the piece after it.
    joint_ends: Vec<(Point2, Point2)>,
    pub lo: Point2,
    pub hi: Point2,
}

impl Sealed {
    pub fn new(walk: &[Piece]) -> Sealed {
        let n = walk.len();
        let mut sealed = Sealed {
            pieces: Vec::with_capacity(n),
            joints: Vec::with_capacity(n),
            joint_ends: vec![(Point2::default(), Point2::default()); n],
            lo: Point2::new(f64::INFINITY, f64::INFINITY),
            hi: Point2::new(f64::NEG_INFINITY, f64::NEG_INFINITY),
        };
        for (i, piece) in walk.iter().enumerate() {
            let (lo, hi) = piece.bounds();
            sealed.lo = Point2::new(sealed.lo.x.min(lo.x), sealed.lo.y.min(lo.y));
            sealed.hi = Point2::new(sealed.hi.x.max(hi.x), sealed.hi.y.max(hi.y));
            let after = (i + 1) % n;
            let Some(next) = walk.get(after) else {
                continue;
            };
            if same(piece.a, piece.b) && n == 1 {
                // A full circle has no joint.
                sealed.pieces.push(*piece);
                sealed.joints.push((NO_JOINT, NO_JOINT));
                continue;
            }
            sealed.pieces.push(*piece);
            sealed.joints.push((i, after));
            if let Some(slot) = sealed.joint_ends.get_mut(after) {
                *slot = (piece.b, next.a);
            }
            if !same(piece.b, next.a) {
                sealed.pieces.push(Piece::line(piece.b, next.a));
                sealed.joints.push((after, after));
            }
        }
        sealed
    }

    /// Whether `at` is contact that the shared joints of elements `i` and `j` explain: it
    /// lies within `reach` of an end that meets there.
    fn at_shared_joint(&self, i: usize, j: usize, at: Point2, reach: f64) -> bool {
        let (Some(&(a0, a1)), Some(&(b0, b1))) = (self.joints.get(i), self.joints.get(j)) else {
            return false;
        };
        [a0, a1]
            .into_iter()
            .filter(|&joint| joint != NO_JOINT && (joint == b0 || joint == b1))
            .any(|joint| {
                self.joint_ends
                    .get(joint)
                    .is_some_and(|&(p, q)| dist(at, p) <= reach || dist(at, q) <= reach)
            })
    }
}

/// One element's bounds as (along the sweep axis, across it).
struct Entry {
    chain: usize,
    element: usize,
    piece: Piece,
    lo: (f64, f64),
    hi: (f64, f64),
}

/// Whether to sweep along y: the axis on which the box centres spread widest keeps the fewest
/// boxes open at once (Ericson, "Real-Time Collision Detection", 2005, sec. 7.5.1).
pub(super) fn sweep_on_y(boxes: impl Iterator<Item = (Point2, Point2)> + Clone) -> bool {
    let n = boxes.clone().count().max(1) as f64;
    let centre = |(lo, hi): (Point2, Point2)| (0.5 * (lo.x + hi.x), 0.5 * (lo.y + hi.y));
    let (sx, sy) = boxes
        .clone()
        .map(centre)
        .fold((0.0, 0.0), |(x, y), c| (x + c.0, y + c.1));
    let (mx, my) = (sx / n, sy / n);
    let spread =
        |(x, y): (f64, f64), c: (f64, f64)| (x + (c.0 - mx).powi(2), y + (c.1 - my).powi(2));
    let (vx, vy) = boxes.map(centre).fold((0.0, 0.0), spread);
    vy > vx
}

#[derive(Clone, Copy, PartialEq)]
pub(super) enum Search {
    /// Contact of a loop with itself, away from its joints.
    Within,
    /// Contact between two different loops.
    Between,
}

/// The first contact found per loop: where, and (for `Between`) with which other loop.
/// Loops flagged in `skip` take no part.
pub(super) fn search(
    loops: &[Sealed],
    skip: &[bool],
    mode: Search,
    tol: f64,
    eps: f64,
    budget: &mut Budget,
) -> Result<Vec<Option<(usize, Point2)>>, ChainError> {
    let skipped = |chain: usize| skip.get(chain).copied().unwrap_or(false);
    let live = || {
        loops
            .iter()
            .enumerate()
            .filter(|(chain, _)| !skipped(*chain))
    };
    let on_y = sweep_on_y(live().flat_map(|(_, sealed)| sealed.pieces.iter().map(Piece::bounds)));
    let axes = |p: Point2| if on_y { (p.y, p.x) } else { (p.x, p.y) };
    let mut entries = Vec::new();
    for (chain, sealed) in live() {
        for (element, piece) in sealed.pieces.iter().enumerate() {
            let (lo, hi) = piece.bounds();
            entries.push(Entry {
                chain,
                element,
                piece: *piece,
                lo: axes(lo),
                hi: axes(hi),
            });
        }
    }
    entries.sort_by(|p, q| {
        p.lo.0
            .total_cmp(&q.lo.0)
            .then(p.chain.cmp(&q.chain))
            .then(p.element.cmp(&q.element))
    });
    let mut hits: Vec<Option<(usize, Point2)>> = vec![None; loops.len()];
    let mut active: Vec<usize> = Vec::new();
    let mut found = Vec::new();
    for (current, entry) in entries.iter().enumerate() {
        active.retain(|&k| {
            entries
                .get(k)
                .is_some_and(|other| other.hi.0 + eps >= entry.lo.0)
        });
        budget.spend(active.len())?;
        for &k in &active {
            let Some(other) = entries.get(k) else {
                continue;
            };
            if (other.chain == entry.chain) != (mode == Search::Within) {
                continue;
            }
            if other.hi.1 + eps < entry.lo.1 || entry.hi.1 + eps < other.lo.1 {
                continue;
            }
            found.clear();
            contacts(&other.piece, &entry.piece, eps, &mut found);
            let real = found.iter().copied().find(|&at| {
                mode == Search::Between
                    || !loops.get(entry.chain).is_some_and(|sealed| {
                        sealed.at_shared_joint(other.element, entry.element, at, tol + eps)
                    })
            });
            if let Some(at) = real {
                for (chain, with) in [(entry.chain, other.chain), (other.chain, entry.chain)] {
                    if let Some(slot @ None) = hits.get_mut(chain) {
                        *slot = Some((with, at));
                    }
                }
            }
        }
        active.push(current);
    }
    Ok(hits)
}
