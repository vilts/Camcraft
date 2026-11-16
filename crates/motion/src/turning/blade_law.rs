//! The blade judgments over the meridian scan (M5.2 C, M5.3 D; split from `revolve.rs` by #904, structure only): one generated blade path, and a program's blade legs composed for the final-part scan - each read into [`revolve`]'s band moves and judged by its shared close. The `_excess` twins are #904's: the gouge clause tightened for a model imported coarser than promised. Provenance: `revolve`'s (machine-tool handbook grooving and cutoff, our own boxcar-min scan); no vendor strategy consulted.

use crate::cl::{MoveKind, Toolpath};
use crate::geometry::Triangle;
use crate::verify::Warning;

use super::profile;
use super::revolve::{judge, BandMove, Floor};

/// The full blade judgment over a generated path: the warnings this
/// path deserves, in the vocabulary the verifier already speaks.
/// A model this cannot even frame reports as a gouge at the first
/// cut - a judge that cannot measure never reports silence (the
/// law (b) precedent the envelope scan keeps too).
pub fn blade_warnings(facets: &[Triangle], path: &Toolpath, width: f64, tol: f64, leave: f64) -> Vec<Warning> {
    blade_warnings_excess(facets, path, width, tol, leave, 0.0)
}

/// [`blade_warnings`] on a model imported coarser than promised
/// (#904): the gouge clause gives the model's `excess` chord error
/// back out of `tol` while a positive tolerance is left
/// ([`super::revolve::gouge_tol_on`]); the rest clause and the silhouette's chord
/// budget keep `tol`. An `excess` of 0 is [`blade_warnings`] exactly.
pub fn blade_warnings_excess(
    facets: &[Triangle],
    path: &Toolpath,
    width: f64,
    tol: f64,
    leave: f64,
    excess: f64,
) -> Vec<Warning> {
    let first_cut = || Warning::Gouge {
        at: path
            .moves
            .iter()
            .position(|m| m.kind != MoveKind::Rapid)
            .unwrap_or(0),
    };
    if width <= 0.0 || !width.is_finite() || facets.is_empty() {
        return vec![first_cut()];
    }
    let f = match profile::Frame::about(facets) {
        Ok(f) => f,
        Err(_) => return vec![first_cut()],
    };
    let pts = match profile::silhouette(facets, &f, tol * 0.25) {
        Ok(p) => p,
        Err(_) => return vec![first_cut()],
    };
    let moves: Vec<BandMove> = path
        .moves
        .iter()
        .enumerate()
        .filter(|(_, m)| m.kind == MoveKind::Feed)
        .map(|(i, m)| {
            let (sa, ra) = f.sr(m.from);
            let (sb, rb) = f.sr(m.to);
            BandMove {
                a: sa,
                b: sb,
                tip: ra.min(rb),
                at: i,
                floor: Floor::Flat,
                half: width * 0.5,
            }
        })
        .collect();
    judge(&pts, moves, (tol, excess), leave, false, None)
}

/// One leg of a program composed for the final-part scan: the
/// passes an operation cached, and the width of the blade that
/// would run them.
pub struct BladeLeg<'a> {
    pub path: &'a Toolpath,
    pub width: f64,
}

/// The final-part scan (M5.3 D): what an in-process probe touches
/// is everything the program's blades cut TOGETHER, so a probe
/// budget is judged on the composed cut, not on any one op. The
/// legs' moves union into one lattice - the same boxcar-min law as
/// the single-tool scan, so a finish that comes later in the
/// program answers for a rough's left floor, and the kerf law
/// stands unchanged: inside a cutoff, what stands there was meant
/// to go. `tol` is the strictest authored ask (it chords the
/// silhouette at its quarter, the scan's own resolution law), and
/// leave is zero BY CONSTRUCTION - a roughing promise is what the
/// PROMISE forgives, never the part. `at` indexes the composed
/// feed order (program order, the caller's op names), which is how
/// the verdict says who left the steel. A model this cannot frame,
/// or a blade without a width, reports as a gouge at move 0 - a
/// judge that cannot measure never reports silence.
pub fn blade_program_warnings(facets: &[Triangle], legs: &[BladeLeg<'_>], tol: f64) -> Vec<Warning> {
    blade_program_warnings_excess(facets, legs, tol, 0.0)
}

/// [`blade_program_warnings`] on a model imported coarser than
/// promised (#904): the composed scan's gouge clause tightened by the
/// model's `excess`, exactly as [`blade_warnings_excess`] is.
pub fn blade_program_warnings_excess(
    facets: &[Triangle],
    legs: &[BladeLeg<'_>],
    tol: f64,
    excess: f64,
) -> Vec<Warning> {
    if legs.is_empty()
        || facets.is_empty()
        || legs.iter().any(|l| l.width <= 0.0 || !l.width.is_finite())
    {
        return vec![Warning::Gouge { at: 0 }];
    }
    let f = match profile::Frame::about(facets) {
        Ok(f) => f,
        Err(_) => return vec![Warning::Gouge { at: 0 }],
    };
    let pts = match profile::silhouette(facets, &f, tol * 0.25) {
        Ok(p) => p,
        Err(_) => return vec![Warning::Gouge { at: 0 }],
    };
    let mut moves: Vec<BandMove> = Vec::new();
    let mut start = 0usize;
    for l in legs {
        for (i, m) in l.path.moves.iter().enumerate() {
            if m.kind != MoveKind::Feed {
                continue;
            }
            let (sa, ra) = f.sr(m.from);
            let (sb, rb) = f.sr(m.to);
            moves.push(BandMove {
                a: sa,
                b: sb,
                tip: ra.min(rb),
                at: start + i,
                floor: Floor::Flat,
                half: l.width * 0.5,
            });
        }
        start += l.path.moves.len();
    }
    judge(&pts, moves, (tol, excess), 0.0, false, None)
}
