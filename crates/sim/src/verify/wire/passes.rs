//! The wire laws' pass split (#538 over #534): a skimmed cut is the
//! rough pass and its skims, each walked from the threading point and
//! back to it, each burning the kerf of its own condition (wire radius
//! plus that condition's spark gap). Each move is judged at its own
//! pass's kerf offset - the real half-width that pass burns, never a
//! narrower one - so a skim's nearer offset is not misread as the rough
//! kerf entering the part, and a skim that does come too near still is.
//! A pass is read on the wire as the kerf law reads it (#536): its
//! section at each judged height and its kerf widened by its lean.
//! Provenance: public wire EDM practice (a rough cut, then skims at
//! decreasing offsets, each offset the wire radius plus that pass's
//! gap); our own code.

use super::kerf::{reach, Wire};
use super::{dist, Product, WireFinding, KERF_TOL};
use crate::geo2d::Point2;

/// One pass of the wire path: the wire's vertices `first..=last`
/// (0-based; its moves are `first + 1..=last`, 1-based), and the kerf
/// offset it burns.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Pass {
    pub first: usize,
    pub last: usize,
    pub k: f64,
}

impl Pass {
    /// This pass's polyline within `pts` (the wire at one height, or a
    /// prefix of it - a pass past its end is clipped, one wholly past
    /// it is `None`).
    pub(super) fn of<'a>(&self, pts: &'a [Point2]) -> Option<&'a [Point2]> {
        let last = self.last.min(pts.len().checked_sub(1)?);
        (self.first < last).then(|| pts.get(self.first..=last)).flatten()
    }
}

/// How near (mm) a move's end must come to the threading point to
/// close a pass: the path ends each pass exactly on it.
const RETURN_TOL: f64 = 1e-6;

/// Split `pts` (the wire centre's polyline) into its passes. `k` is
/// the rough pass's kerf offset, `skims` each later pass's, in cut
/// order. With no skims the whole path is one pass at `k`, as before.
///
/// # Errors
/// A skim offset that is not a positive number, or a path that does
/// not return to its threading point once per pass - the moves of a
/// pass cannot then be told apart, so nothing is judged at a guess.
pub(super) fn split(pts: &[Point2], k: f64, skims: &[f64]) -> Result<Vec<Pass>, String> {
    if skims.is_empty() {
        return Ok(vec![Pass { first: 0, last: pts.len().saturating_sub(1), k }]);
    }
    if let Some(bad) = skims.iter().find(|s| !(s.is_finite() && **s > 0.0)) {
        return Err(format!("a skim's kerf offset {bad} is not a positive number"));
    }
    let Some(&thread) = pts.first() else {
        return Err("the path has no moves".into());
    };
    let offsets: Vec<f64> = std::iter::once(k).chain(skims.iter().copied()).collect();
    let mut out: Vec<Pass> = Vec::with_capacity(offsets.len());
    let mut first = 0;
    for (i, &q) in pts.iter().enumerate().skip(1) {
        if dist::pp(q, thread) <= RETURN_TOL {
            let Some(&pk) = offsets.get(out.len()) else {
                return Err(format!("the path returns to its threading point more than once per pass ({} passes)", offsets.len()));
            };
            out.push(Pass { first, last: i, k: pk });
            first = i;
        }
    }
    if first + 1 < pts.len() || out.len() != offsets.len() {
        return Err(format!(
            "the path returns to its threading point {} time(s), not once per pass ({} passes) - regenerate it",
            out.len(),
            offsets.len()
        ));
    }
    Ok(out)
}

/// The largest kerf offset any pass burns - the clearance the
/// threading point needs, whichever pass is threaded.
pub(super) fn widest(passes: &[Pass]) -> f64 {
    passes.iter().map(|p| p.k).fold(0.0, f64::max)
}

/// The first move whose kerf enters the product at height `z`, with
/// the count of moves that do: each move at its own pass's kerf,
/// widened by the wire's lean toward the product (`kerf::gouge_at` per
/// pass).
pub(super) fn gouge_at(p: &Product<'_>, wire: &Wire, passes: &[Pass], z: f64) -> Option<WireFinding> {
    let pts = wire.at(z);
    let mut first: Option<(usize, Point2, f64)> = None;
    let mut moves = 0;
    for pass in passes {
        for i in pass.first..pass.last {
            let (Some(&a), Some(&b)) = (pts.get(i), pts.get(i + 1)) else { break };
            let (d, at) = dist::clearance(p, a, b);
            let need = reach(pass.k, a, b, wire.lean.get(i).copied(), wire.lean.get(i + 1).copied(), at);
            if d < need - KERF_TOL {
                moves += 1;
                if first.is_none() {
                    first = Some((i + 1, at, need - d));
                }
            }
        }
    }
    first.map(|(move_no, at, depth)| WireFinding::Gouge { move_no, z, at, depth, moves })
}
