//! The undercut law of a constant-taper wire cut (#3210): a move that
//! cuts a drafted wall at one judged height must cut it at every
//! height. The wire is a straight line, so a taper that leans it past
//! or short of the wall's own draft stands it off the wall at the
//! heights away from where it was laid - the part is left oversize
//! there (a punch) or the opening undersize (a die). The law judges
//! only the last pass (it sets the size), and only a move whose wall
//! drafts: a vertical wall cut at a taper is a stated relief, not an
//! undercut.
//!
//! Provenance: plain 2D distance geometry (the kerf law's clearance and
//! reach) over the public wire EDM practice the umbrella #290 cites -
//! a tapered cut follows the wall's draft angle. Our own code; no
//! vendor material.

use super::kerf::{reach, Wire};
use super::passes::Pass;
use super::{dist, Product, WireFinding, WireSection, KERF_TOL};
use crate::geo2d::Point2;

/// How one move reads at one height: the kerf's stand-off from the
/// product past its reach (negative when it enters), where, and the
/// clearance of the upright wire at the program point.
struct Read {
    off: f64,
    z: f64,
    at: Point2,
    upright: f64,
}

/// The first move of the last pass whose kerf stands off a drafted wall
/// by more than `tol` (plus [`KERF_TOL`]) at some height, while it cuts
/// that wall within `tol` at another; with the count of such moves.
/// `product` reads each section as the kerf law does.
pub(super) fn judge<'a>(
    sections: &'a [WireSection],
    product: impl Fn(&'a WireSection) -> Product<'a>,
    wire: &Wire,
    passes: &[Pass],
    tol: f64,
) -> Option<WireFinding> {
    let last = passes.last()?;
    let band = tol + KERF_TOL;
    let planes: Vec<(f64, Product<'a>, Vec<Point2>)> = sections.iter().map(|s| (s.z, product(s), wire.at(s.z))).collect();
    let mut first: Option<(usize, Read)> = None;
    let mut moves = 0;
    for i in last.first..last.last {
        let (Some(&a0), Some(&b0)) = (wire.pts.get(i), wire.pts.get(i + 1)) else { break };
        let (ta, tb) = (wire.lean.get(i).copied(), wire.lean.get(i + 1).copied());
        let reads: Vec<Read> = planes
            .iter()
            .filter_map(|(z, p, pts)| {
                let (&a, &b) = (pts.get(i)?, pts.get(i + 1)?);
                let (d, at) = dist::clearance(p, a, b);
                let (upright, _) = dist::clearance(p, a0, b0);
                Some(Read { off: d - reach(last.k, a, b, ta, tb, at), z: *z, at, upright })
            })
            .collect();
        let finite = |f: fn(&Read) -> f64| reads.iter().map(f).filter(|v| v.is_finite());
        let hugs = finite(|r| r.off).fold(f64::INFINITY, f64::min) <= band;
        let (lo, hi) = (finite(|r| r.upright).fold(f64::INFINITY, f64::min), finite(|r| r.upright).fold(f64::NEG_INFINITY, f64::max));
        let drafted = hi - lo > band;
        let worst = reads.into_iter().filter(|r| r.off.is_finite()).max_by(|a, b| a.off.total_cmp(&b.off));
        let Some(worst) = worst.filter(|w| hugs && drafted && w.off > band) else { continue };
        moves += 1;
        if first.is_none() {
            first = Some((i + 1, worst));
        }
    }
    first.map(|(move_no, r)| WireFinding::Undercut { move_no, z: r.z, at: r.at, off: r.off, moves, tol })
}
