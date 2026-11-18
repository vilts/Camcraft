//! The crossing law on the HSM plan's fitted arcs (M11.4 owed arc
//! output; the fit and its area and kink proofs are `arc_proof`): the
//! verifier's law (a) - Feed-Feed pairs crossing properly in XY at a
//! shared depth - held on the TRUE arc, its bracket's chords against
//! every other feed chord of the posted path. The plan's revolutions
//! part from their rings by microns at the landing seams, where an
//! arc's few-micron bow off its chords is a crossing no region test can
//! see; and the verifier chords an arc at its own tolerance, so only the
//! bracket (outside AND inside the arc) is a proof the verifier cannot
//! later contradict. An arc that crosses is given up back to the plan's
//! chords and the walks are posted again, a few rounds at most.
//!
//! Provenance: the orientation predicate of plane geometry (O'Rourke,
//! "Computational Geometry in C") and the uniform-grid broad phase
//! (Franklin 1989, Auto-Carto 9) - the same two laws `verify::crossing`
//! states; clean-room, no vendor material consulted.

use std::collections::HashSet;

use crate::cl::{Move, MoveKind, PathBuilder, Toolpath, lattice};
use crate::geo2d::Point2;
use crate::geometry::Point3;

use super::arc_proof::{ProofTally, bracket, judge, plain};
use super::arc_touch::Touch;
use super::arcs::{ARC_FIT_TOL, Seg};

/// Rounds the crossing law may refuse arcs in before the walk gives its
/// arcs up wholesale (every round re-posts the walk).
pub(super) const CROSS_ROUNDS: usize = 3;

/// Cells per axis at most in the broad-phase grid.
const MAX_SIDE: f64 = 1024.0;

/// A fitted arc's identity on the posted path: its end, its centre as
/// the move carries it (the 1 um lattice) and its sense, as bits.
pub(super) type Key = (u64, u64, u64, u64, bool);

pub(super) fn key(to: Point2, center: Point2, ccw: bool) -> Key {
    (to.x.to_bits(), to.y.to_bits(), lattice(center.x).to_bits(), lattice(center.y).to_bits(), ccw)
}

/// The key of a posted planar XY feed arc.
fn move_key(m: &Move) -> Option<Key> {
    let a = m.arc?;
    (m.kind == MoveKind::Feed && a.plane.is_xy() && m.from.z == m.to.z).then(|| key(Point2::new(m.to.x, m.to.y), a.center, a.ccw))
}

/// Every fitted arc's key over the walks.
pub(super) fn own_keys(runs: &[Vec<Seg>]) -> HashSet<Key> {
    runs.iter()
        .flatten()
        .filter_map(|s| match s {
            Seg::Arc { center, ccw, to } => Some(key(*to, *center, *ccw)),
            Seg::Line(_) => None,
        })
        .collect()
}

/// A feed chord on the posted path: its move, its ends, its Z ends, and
/// the fitted arc it brackets (`None`: the move's own chord).
struct Chord {
    mv: usize,
    a: Point2,
    b: Point2,
    z0: f64,
    z1: f64,
    own: Option<Key>,
    /// A corner arc's end tangents (`arc_touch`), by index.
    touch: Option<usize>,
}

/// Shared-vertex snap matching `verify::crossing::CROSSING_TOUCH` (mm).
const CROSSING_TOUCH_SQ: f64 = 1e-18;

/// Twice the signed area of `a b c`.
fn orient(a: Point2, b: Point2, c: Point2) -> f64 {
    (b.x - a.x) * (c.y - a.y) - (b.y - a.y) * (c.x - a.x)
}

/// Depth overlap matching `verify::crossing`'s pair predicate: planar
/// pairs share one plane, descending chords share an open depth range.
fn z_depths_overlap(az0: f64, az1: f64, bz0: f64, bz1: f64) -> bool {
    if az0 == az1 && bz0 == bz1 {
        az0 == bz0
    } else {
        az0.max(az1).min(bz0.max(bz1)) - az0.min(az1).max(bz0.min(bz1)) > 0.0
    }
}

/// True proper crossing between XY segments `(a0, a1)` and `(b0, b1)`,
/// treating vertices within `1e-9` as a shared-endpoint touch (#1129).
fn proper_cross_touch(a0: Point2, a1: Point2, b0: Point2, b1: Point2) -> bool {
    if a0.x.max(a1.x) < b0.x.min(b1.x) - 1e-9
        || a0.x.min(a1.x) > b0.x.max(b1.x) + 1e-9
        || a0.y.max(a1.y) < b0.y.min(b1.y) - 1e-9
        || a0.y.min(a1.y) > b0.y.max(b1.y) + 1e-9
    {
        return false;
    }
    let touch = [a0, a1].iter().any(|p| [b0, b1].iter().any(|q| (p.x - q.x).powi(2) + (p.y - q.y).powi(2) <= CROSSING_TOUCH_SQ));
    if touch {
        return false;
    }
    let (oa, ob) = (orient(a0, a1, b0), orient(a0, a1, b1));
    let (oc, od) = (orient(b0, b1, a0), orient(b0, b1, a1));
    oa != 0.0 && ob != 0.0 && oc != 0.0 && od != 0.0 && ((oa > 0.0) != (ob > 0.0)) && ((oc > 0.0) != (od > 0.0))
}

/// Does a candidate straight segment `(a0 -> a1)` over `[az0, az1]`
/// cross any prior `Feed` move in `prior` under the verifier's law?
pub(crate) fn seg_crosses_prior(a0: Point2, a1: Point2, az0: f64, az1: f64, prior: &[Move]) -> bool {
    prior.iter().any(|m| {
        if m.kind != MoveKind::Feed || !z_depths_overlap(az0, az1, m.from.z, m.to.z) {
            return false;
        }
        if m.arc.is_none() {
            return proper_cross_touch(a0, a1, Point2::new(m.from.x, m.from.y), Point2::new(m.to.x, m.to.y));
        }
        m.chords(ARC_FIT_TOL).into_iter().any(|(b0, b1)| proper_cross_touch(a0, a1, b0, b1))
    })
}

/// Does any `Feed` move in `cand` cross any prior `Feed` move in `prior`?
pub(crate) fn moves_cross_prior(cand: &[Move], prior: &[Move]) -> bool {
    cand.iter().filter(|m| m.kind == MoveKind::Feed).any(|m| {
        if m.arc.is_none() {
            return seg_crosses_prior(Point2::new(m.from.x, m.from.y), Point2::new(m.to.x, m.to.y), m.from.z, m.to.z, prior);
        }
        m.chords(ARC_FIT_TOL).into_iter().any(|(a0, a1)| seg_crosses_prior(a0, a1, m.from.z, m.to.z, prior))
    })
}

/// The verifier's pair predicate: two chords of different moves cross
/// properly in XY at a shared depth (a planar pair at one plane; a
/// descending chord only over a shared depth RANGE - a touch at a
/// single endpoint is the graze every entry makes by construction).
fn crosses(p: &Chord, q: &Chord) -> bool {
    if p.mv == q.mv || !z_depths_overlap(p.z0, p.z1, q.z0, q.z1) {
        return false;
    }
    let (oa, ob) = (orient(p.a, p.b, q.a), orient(p.a, p.b, q.b));
    let (oc, od) = (orient(q.a, q.b, p.a), orient(q.a, q.b, p.b));
    oa != 0.0 && ob != 0.0 && oc != 0.0 && od != 0.0 && ((oa > 0.0) != (ob > 0.0)) && ((oc > 0.0) != (od > 0.0))
}

/// The feed chords of `path`: a fitted arc (by key) as its bracket's
/// chords, every other feed as its own chords at [`ARC_FIT_TOL`]; and
/// the end tangents of the corner arcs (`blends`, by key).
fn chords(path: &Toolpath, keys: &HashSet<Key>, blends: &HashSet<Key>) -> (Vec<Chord>, Vec<Touch>) {
    let mut out = Vec::new();
    let mut touches = Vec::new();
    for (i, m) in path.moves.iter().enumerate() {
        if m.kind != MoveKind::Feed {
            continue;
        }
        let (z0, z1) = (m.from.z, m.to.z);
        let own = move_key(m).filter(|k| keys.contains(k));
        let bracketed = own.and_then(|_| {
            let a = m.arc?;
            bracket(Point2::new(m.from.x, m.from.y), a.center, a.ccw, Point2::new(m.to.x, m.to.y))
        });
        match bracketed {
            Some((fan, hull)) => {
                let touch = own
                    .filter(|k| blends.contains(k))
                    .zip(m.arc)
                    .and_then(|(_, a)| Touch::of(a.center, &fan, &hull))
                    .map(|t| {
                        touches.push(t);
                        touches.len() - 1
                    });
                for poly in [fan, hull] {
                    out.extend(poly.windows(2).map(|w| Chord { mv: i, a: w[0], b: w[1], z0, z1, own, touch }));
                }
            }
            None => out.extend(m.chords(ARC_FIT_TOL).into_iter().map(|(a, b)| Chord { mv: i, a, b, z0, z1, own, touch: None })),
        }
    }
    (out, touches)
}

/// The fitted arcs of `path` (by key) whose true arc crosses another
/// feed: the uniform-grid broad phase over every feed chord's box, the
/// predicate above on the pairs that share a cell. A corner arc
/// (`blends`, by key) does not cross a chord that only touches the
/// tangent at one of its ends (`arc_touch`, #2317); no other arc has
/// that allowance.
pub(super) fn crossing_arcs(path: &Toolpath, keys: &HashSet<Key>, blends: &HashSet<Key>) -> HashSet<Key> {
    let (cs, touches) = chords(path, keys, blends);
    let grazes = |c: &Chord, q: &Chord| c.touch.and_then(|t| touches.get(t)).is_some_and(|t| t.grazes(q.a, q.b));
    let finite = |c: &Chord| c.a.x.is_finite() && c.a.y.is_finite() && c.b.x.is_finite() && c.b.y.is_finite();
    let (mut lo, mut hi) = (Point2::new(f64::INFINITY, f64::INFINITY), Point2::new(f64::NEG_INFINITY, f64::NEG_INFINITY));
    for c in cs.iter().filter(|c| finite(c)) {
        lo = Point2::new(lo.x.min(c.a.x).min(c.b.x), lo.y.min(c.a.y).min(c.b.y));
        hi = Point2::new(hi.x.max(c.a.x).max(c.b.x), hi.y.max(c.a.y).max(c.b.y));
    }
    let mut hit = HashSet::new();
    if cs.is_empty() || !(lo.x <= hi.x && lo.y <= hi.y) {
        return hit;
    }
    let (w, h) = ((hi.x - lo.x).max(1e-6), (hi.y - lo.y).max(1e-6));
    let cell = ((w * h / cs.len() as f64).sqrt() * 2.0).max(w / MAX_SIDE).max(h / MAX_SIDE).max(1e-6);
    let (nx, ny) = ((w / cell).floor() as usize + 1, (h / cell).floor() as usize + 1);
    let span = |c: &Chord| -> [usize; 4] {
        let ix = |x: f64| (((x - lo.x) / cell).floor().max(0.0) as usize).min(nx - 1);
        let iy = |y: f64| (((y - lo.y) / cell).floor().max(0.0) as usize).min(ny - 1);
        [ix(c.a.x.min(c.b.x)), ix(c.a.x.max(c.b.x)), iy(c.a.y.min(c.b.y)), iy(c.a.y.max(c.b.y))]
    };
    let mut cells: Vec<Vec<usize>> = vec![Vec::new(); nx.saturating_mul(ny)];
    for (k, c) in cs.iter().enumerate().filter(|(_, c)| finite(c)) {
        let [x0, x1, y0, y1] = span(c);
        for y in y0..=y1 {
            for x in x0..=x1 {
                if let Some(cell) = cells.get_mut(y * nx + x) {
                    cell.push(k);
                }
            }
        }
    }
    for (k, c) in cs.iter().enumerate() {
        let Some(own) = c.own else { continue };
        if !finite(c) {
            hit.insert(own);
            continue;
        }
        let [x0, x1, y0, y1] = span(c);
        'cells: for y in y0..=y1 {
            for x in x0..=x1 {
                for &o in cells.get(y * nx + x).map(Vec::as_slice).unwrap_or(&[]) {
                    if o != k && cs.get(o).is_some_and(|q| crosses(c, q) && !grazes(c, q) && !grazes(q, c)) {
                        hit.insert(own);
                        break 'cells;
                    }
                }
            }
        }
    }
    hit
}

/// Hand every arc of `runs` keyed in `keys` back to its chords.
pub(super) fn refuse(passes: &[Vec<Point2>], runs: &mut [Vec<Seg>], keys: &HashSet<Key>, tally: &mut ProofTally) {
    for (pass, segs) in passes.iter().zip(runs.iter_mut()) {
        let judged = judge(pass, std::mem::take(segs), |_, group| {
            let cross = group.iter().any(|s| matches!(s, Seg::Arc { center, ccw, to } if keys.contains(&key(*to, *center, *ccw))));
            if cross {
                tally.arcs = tally.arcs.saturating_sub(group.len());
                tally.crossed += 1;
                None
            } else {
                Some(group.to_vec())
            }
        });
        *segs = judged.unwrap_or_else(|| plain(pass, tally));
    }
}

/// Post the walks (`post` lays them onto a builder standing where `b`
/// does, with its notes) and hold the crossing law on the result: an
/// arc whose true arc crosses another feed is refused back to its chords
/// and the walks are posted again, [`CROSS_ROUNDS`] times at most; past
/// that every arc still standing is given up. The path returned crosses
/// nowhere a plain walk of the plan would not.
pub(super) fn post_crossing_free(
    b: PathBuilder,
    passes: &[Vec<Point2>],
    runs: Vec<Vec<Seg>>,
    tally: &mut ProofTally,
    post: impl Fn(&mut PathBuilder, Option<&[Vec<Seg>]>),
) -> Toolpath {
    // Each retry starts with the same builder, including its pending
    // opening rapid. A bare cursor loses that clearance-position command.
    crossing_free_from(|| b.clone(), passes, runs, tally, |bb, runs| {
        post(bb, Some(runs));
    })
    .0
}

/// [`post_crossing_free`] from a bare start, handing back the walks
/// that survived too: the 3-D ladder (`machining3d::hsm`) posts a level's
/// walks itself and only asks which arcs the law lets stand.
pub(crate) fn crossing_free(
    start: Point3,
    passes: &[Vec<Point2>],
    runs: Vec<Vec<Seg>>,
    tally: &mut ProofTally,
    post: impl Fn(&mut PathBuilder, &[Vec<Seg>]),
) -> (Toolpath, Vec<Vec<Seg>>) {
    crossing_free_from(|| PathBuilder::new(start), passes, runs, tally, post)
}

fn crossing_free_from(
    builder: impl Fn() -> PathBuilder,
    passes: &[Vec<Point2>],
    mut runs: Vec<Vec<Seg>>,
    tally: &mut ProofTally,
    post: impl Fn(&mut PathBuilder, &[Vec<Seg>]),
) -> (Toolpath, Vec<Vec<Seg>>) {
    let posted = |runs: &[Vec<Seg>]| {
        let mut bb = builder();
        post(&mut bb, runs);
        bb.finish()
    };
    let mut path = posted(&runs);
    for round in 0..=CROSS_ROUNDS {
        let keys = own_keys(&runs);
        if keys.is_empty() {
            break;
        }
        let mut crossing = crossing_arcs(&path, &keys, &HashSet::new());
        if crossing.is_empty() {
            break;
        }
        if round == CROSS_ROUNDS {
            crossing = keys;
        }
        refuse(passes, &mut runs, &crossing, tally);
        path = posted(&runs);
    }
    (path, runs)
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::arc_proof::tests::{disc, ring};
    use super::super::arc_proof::fit_proved;

    /// The crossing law: a ring fitted as arcs, with a straight feed
    /// posted across it at the same plane, gives those arcs up; the same
    /// feed a plane higher is no crossing, and the arcs stay.
    #[test]
    fn an_arc_crossed_by_a_feed_at_its_plane_is_given_up() {
        let legal = disc(0.0, 0.0, 10.0, 720);
        let pass = ring(6.0);
        let mut t = ProofTally::default();
        let runs = vec![fit_proved(&pass, ARC_FIT_TOL * 0.25, &legal, &mut t)];
        let kept = t.arcs;
        assert!(kept >= 2);
        let post_at = |dz: f64| {
            move |b: &mut PathBuilder, runs: Option<&[Vec<Seg>]>| {
                let Some(segs) = runs.and_then(|r| r.first()) else { return };
                super::super::arcs::push_segs(b, segs, -1.0, 500.0);
                b.push(MoveKind::Rapid, Point3::new(-8.0, 0.0, 5.0), None);
                b.push(MoveKind::Feed, Point3::new(-8.0, 0.0, -1.0 + dz), Some(100.0));
                b.push(MoveKind::Feed, Point3::new(8.0, 0.0, -1.0 + dz), Some(500.0));
            }
        };
        let b = PathBuilder::approached(Point3::new(6.0, 0.0, -1.0));
        let mut t2 = t;
        let path = post_crossing_free(b, std::slice::from_ref(&pass), runs.clone(), &mut t2, post_at(0.0));
        assert_eq!(path.moves[0].kind, MoveKind::Rapid, "a crossing retry keeps the opening rapid");
        let arcs = path.moves.iter().filter(|m| m.arc.is_some()).count();
        assert!(arcs < kept && t2.crossed >= 1, "arcs {arcs} of {kept}, {t2:?}");
        assert!(crate::verify::crossed_pairs(&path, 0.01).iter().all(|p| p.iter().all(|&i| path.moves[i].arc.is_none())));
        let b = PathBuilder::new(Point3::new(6.0, 0.0, -1.0));
        let mut t3 = t;
        let path = post_crossing_free(b, &[pass], runs, &mut t3, post_at(1.0));
        assert_eq!(path.moves.iter().filter(|m| m.arc.is_some()).count(), kept, "{t3:?}");
        assert_eq!(t3.crossed, 0);
    }
}
