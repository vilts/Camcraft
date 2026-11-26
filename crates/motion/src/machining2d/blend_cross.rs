//! The crossing law on walks with corner arcs (#1733 arc turns, #2317).
//! `arc_cross` gives a crossing arc back to its pass's own chords, and
//! can only map a walk whose vertices are the pass's. A rounded walk is
//! not such a walk, so the law here keeps the walks as fitted (`base`)
//! and rounds them again each round:
//!
//! - a corner arc that crosses another feed is refused alone: its corner
//!   stays sharp (counted `crossed` in the fillet tally), every other
//!   corner of the pass keeps its arc;
//! - a fitted arc that crosses (trimmed by a corner arc or not) is given
//!   back to the plan's chords in `base`, by `arc_cross::refuse`;
//! - the walks are rounded and posted again, [`CROSS_ROUNDS`] times at
//!   most; past that no corner is rounded and the plain law
//!   (`arc_cross::post_crossing_free`) holds on the fitted walks.
//!
//! The path returned is one the law read with no crossing, or the plain
//! law's own answer. A corner arc that only touches the tangent at its
//! end is no crossing (`arc_touch`, with its bound).
//!
//! Provenance: ours, the refusal loop of `arc_cross` (orientation
//! predicate, O'Rourke; uniform grid, Franklin 1989) over the tangent
//! circle of `fillet.rs`; clean-room, no vendor material consulted.

use std::collections::HashSet;

use crate::cl::{PathBuilder, Toolpath};
use crate::geo2d::Point2;

use super::arc_cross::{CROSS_ROUNDS, Key, crossing_arcs, key, own_keys, post_crossing_free, refuse};
use super::arc_proof::ProofTally;
use super::arcs::{ARC_FIT_TOL, Seg};
use super::fillet::{Legal, Source, Traced, fillet_corners_except};
use super::fillet_tally::{FilletTally, fillet_floor};

/// The corner arcs asked for: the radius and where an arc may stand.
pub(super) struct Blend<'a> {
    pub(super) rho: f64,
    pub(super) legal: Legal<'a>,
}

/// A corner of a pass: the pass and the corner point, as bits.
type Corner = (usize, u64, u64);

/// The corners the law keeps sharp.
#[derive(Clone, Default)]
pub(super) struct Bans {
    corners: HashSet<Corner>,
    all: bool,
}

impl Bans {
    pub(super) fn ban(&mut self, pass: usize, at: Point2) {
        self.corners.insert((pass, at.x.to_bits(), at.y.to_bits()));
    }
}

/// The walks rounded: the walks, the fillet books, and each added or
/// trimmed arc with its pass.
pub(super) struct Rounded {
    pub(super) runs: Vec<Vec<Seg>>,
    pub(super) tally: FilletTally,
    trace: Vec<(usize, Traced)>,
}

impl Rounded {
    /// The fillet notes, as the pocket family words them.
    pub(super) fn notes(&self) -> Vec<String> {
        self.tally.notes("pass", fillet_floor(ARC_FIT_TOL))
    }
}

/// Round every pass's walk (`base[i]` walks `passes[i]` past its first
/// point), the banned corners kept sharp.
pub(super) fn round_walks(passes: &[Vec<Point2>], base: &[Vec<Seg>], blend: &Blend, bans: &Bans) -> Rounded {
    let mut out = Rounded { runs: Vec::with_capacity(base.len()), tally: FilletTally::default(), trace: Vec::new() };
    for (i, (pass, walk)) in passes.iter().zip(base).enumerate() {
        let Some(first) = pass.first() else {
            out.runs.push(Vec::new());
            continue;
        };
        let banned = |v: Point2| bans.all || bans.corners.contains(&(i, v.x.to_bits(), v.y.to_bits()));
        let (walk, t, trace) = fillet_corners_except(walk, *first, blend.rho, ARC_FIT_TOL, &blend.legal, &banned);
        out.tally.add(t);
        out.trace.extend(trace.into_iter().map(|t| (i, t)));
        out.runs.push(walk);
    }
    out
}

/// Round `base`, post it (`post` lays the walks onto a builder standing
/// where `b` does, the fillet notes on it) and hold the crossing law on
/// the result, one corner arc at a time (module doc).
pub(super) fn post_blended_crossing_free(
    b: PathBuilder,
    passes: &[Vec<Point2>],
    mut base: Vec<Vec<Seg>>,
    blend: &Blend,
    seed_bans: &Bans,
    tally: &mut ProofTally,
    post: impl Fn(&mut PathBuilder, Option<&[Vec<Seg>]>),
) -> Toolpath {
    let noted = |r: &Rounded| {
        let mut bb = b.clone();
        for n in r.notes() {
            bb.note(n);
        }
        bb
    };
    let mut bans = seed_bans.clone();
    for round in 0..=CROSS_ROUNDS {
        let rounded = round_walks(passes, &base, blend, &bans);
        let mut bb = noted(&rounded);
        post(&mut bb, Some(rounded.runs.as_slice()));
        let path = bb.finish();
        let keys = own_keys(&rounded.runs);
        let traced: Vec<(Key, usize, &Source)> =
            rounded.trace.iter().map(|(i, t)| (key(t.to, t.center, t.ccw), *i, &t.source)).collect();
        let blends: HashSet<Key> =
            traced.iter().filter(|(.., s)| matches!(s, Source::Corner(_))).map(|(k, ..)| *k).collect();
        let crossing = if keys.is_empty() { HashSet::new() } else { crossing_arcs(&path, &keys, &blends) };
        if crossing.is_empty() {
            return path;
        }
        if round == CROSS_ROUNDS {
            break;
        }
        // A corner arc: its corner stays sharp. A fitted arc - by its
        // own key, or by the key it had before a corner arc trimmed it -
        // goes back to the plan's chords.
        let mut fitted: HashSet<Key> = crossing.clone();
        for (k, i, source) in &traced {
            if !crossing.contains(k) {
                continue;
            }
            fitted.remove(k);
            match source {
                Source::Corner(v) => {
                    bans.corners.insert((*i, v.x.to_bits(), v.y.to_bits()));
                }
                Source::Trimmed(to) => {
                    fitted.insert((to.x.to_bits(), to.y.to_bits(), k.2, k.3, k.4));
                }
            }
        }
        if !fitted.is_empty() {
            refuse(passes, &mut base, &fitted, tally);
        }
    }
    // The rounds are spent: no corner arc, and the plain law on the
    // fitted walks. Every corner that would round is counted `crossed`.
    bans.all = true;
    let rounded = round_walks(passes, &base, blend, &bans);
    post_crossing_free(noted(&rounded), passes, rounded.runs, tally, post)
}
