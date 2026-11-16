//! The lifted pieces' chords (M11.4 C round 4, `lift`; issue #982). The
//! spiral's own sag keeps its rings microns apart at the walls; a biting
//! piece is where its neighbours stand a step apart, so it can be posted
//! on far longer chords (fewer moves, no joint past the in-cut break
//! law). But a chord cuts inside the arc it replaces: by up to its
//! sagitta (elementary circle geometry: a chord `c` of a circle `R`
//! stands `R - sqrt(R^2 - c^2 / 4)` inside it) it leaves stock the
//! spiral's own chords clear, and the next piece meets that sliver as
//! engagement the spiral never planned - a rim sample or two over the
//! bound, appearing and vanishing with nanometres of ring geometry (a
//! planner cell is about two samples wide at the rim; which cell centres
//! a 0.01 mm sliver covers is chance). So the longer chords are taken
//! only where the planner's stock (`raster`) shows they change nothing:
//! they read within the bound AND leave standing no cell the piece's
//! own chords clear. The lifted plan's stock is then never more than
//! its pieces' own chords leave, and its reading is compared with the
//! spiral's on the same stock. The test is ours.

use crate::geo2d::Point2;

use super::Worst;
use super::guard::crossing_pairs;
use super::lift::LiftSpec;
use super::lift_split::{Piece, thin_turn};
use super::raster::Raster;

/// A front's piece is re-thinned to this sag (mm, the verifier's own
/// chord tolerance) ...
const COARSE_SAG: f64 = 0.01;

/// ... and no joint of it turns more than this (degrees): under the
/// 5-degree in-cut break law with a margin.
const MAX_TURN_DEG: f64 = 4.0;

/// The fronts' pieces re-thinned to longer chords (the spiral's own sag
/// keeps rings microns apart at the walls; a biting piece is where its
/// neighbours stand a step apart) under the break law, each keeping its
/// fine chords to fall back on - except a piece whose coarser chords
/// would cross another piece (either version) or the laps after them
/// (`rest`): it keeps only its fine chords. No two posted pieces cross.
pub(super) fn coarsen(pieces: &mut [Piece], rest: &[Point2]) {
    let n = pieces.len();
    let coarse: Vec<Vec<Point2>> = pieces.iter().map(|p| thin_turn(&p.pts, MAX_TURN_DEG.to_radians(), COARSE_SAG)).collect();
    let mut ok: Vec<bool> = coarse.iter().zip(pieces.iter()).map(|(c, p)| c.len() < p.pts.len()).collect();
    for _ in 0..8 {
        // Walks: the coarse candidates still standing, every fine piece,
        // the laps; a candidate may only touch its own fine version.
        let live: Vec<usize> = (0..n).filter(|&k| ok[k]).collect();
        let mut walks: Vec<Vec<Point2>> = live.iter().map(|&k| coarse[k].clone()).collect();
        walks.extend(pieces.iter().map(|p| p.pts.clone()));
        walks.push(rest.to_vec());
        let m = live.len();
        let mut bad = false;
        for (a, b) in crossing_pairs(&walks) {
            for (x, y) in [(a, b), (b, a)] {
                if x < m && y != m + live[x] {
                    ok[live[x]] = false;
                    bad = true;
                }
            }
        }
        if !bad {
            for k in live {
                pieces[k].fine = Some(std::mem::replace(&mut pieces[k].pts, coarse[k].clone()));
            }
            return;
        }
    }
}

/// Cut `p` on `ras` and return its worst reading: on its longer chords
/// where they read within the bound and leave no cell standing that its
/// own (fine) chords clear, else on its own chords - `p.pts` is what was
/// cut. A sliver the longer chords would leave is stock the next piece
/// was never planned on (issue #982).
pub(super) fn cut(ras: &mut Raster, p: &mut Piece, s: &LiftSpec) -> Worst {
    let read = (s.r, s.rim, s.step);
    let Some(fine) = p.fine.take() else { return ras.walk(&p.pts, s.r, s.rim, s.step) };
    let mut coarse = ras.clone();
    let w = coarse.walk_until(&p.pts, read, s.bound);
    // The piece's own chords, cut for real: what the spiral leaves here.
    let own = ras.walk(&fine, s.r, s.rim, s.step);
    if w.0 <= s.bound && !coarse.stands_over(ras) {
        *ras = coarse;
        return w;
    }
    p.pts = fine;
    own
}
