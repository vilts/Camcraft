//! The threading verdicts (M5.2 D, issue #629; split from `thread.rs` by #904, structure only): the wave scan of a generated threading path and its hand check. The `_excess` twin is #904's: the gouge clause tightened for a model imported coarser than promised. Provenance: `thread`'s (machine-tool handbook threading, our own wave floor on `revolve`'s scan); no vendor strategy consulted.

use crate::cl::{MoveKind, ThreadHand, Toolpath};
use crate::geometry::Triangle;
use crate::verify::Warning;

use super::profile;
use super::revolve;

/// The threading verdict over a generated path: the path's own
/// helix states the lattice - its radial plunges stand at the run's
/// first center, and every SECOND arc endpoint returns the tool to
/// the frame's meridian, where the wave's V belongs. Rapids cut
/// nothing and reset the count; the judge is [`revolve`]'s scan,
/// with the crest law (floors above the blank measure nothing, the
/// shell owns the crest) and no leave. A model this cannot even
/// frame reports as a gouge at the first cut - a judge that cannot
/// measure never reports silence.
pub fn thread_warnings(facets: &[Triangle], path: &Toolpath, insert_width: f64, form_angle: f64, tol: f64) -> Vec<Warning> {
    thread_warnings_excess(facets, path, insert_width, form_angle, tol, 0.0)
}

/// [`thread_warnings`] on a model imported coarser than promised
/// (#904): the wave scan's gouge clause gives the model's `excess`
/// chord error back out of `tol` while a positive tolerance is left
/// ([`revolve::gouge_tol_on`]). An `excess` of 0 is
/// [`thread_warnings`] exactly.
pub fn thread_warnings_excess(
    facets: &[Triangle],
    path: &Toolpath,
    insert_width: f64,
    form_angle: f64,
    tol: f64,
    excess: f64,
) -> Vec<Warning> {
    let first_cut = || Warning::Gouge {
        at: path
            .moves
            .iter()
            .position(|m| m.kind != MoveKind::Rapid)
            .unwrap_or(0),
    };
    if facets.is_empty()
        || !(insert_width > 0.0 && insert_width.is_finite())
        || !(tol > 0.0 && tol.is_finite())
        || !(0.0 < form_angle && form_angle < 180.0)
    {
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
    let cot = 1.0 / (form_angle * 0.5).to_radians().tan();
    let mut moves: Vec<revolve::BandMove> = Vec::new();
    let mut parity = 0u32;
    for (i, m) in path.moves.iter().enumerate() {
        if m.kind == MoveKind::Rapid {
            continue;
        }
        let (sa, ra) = f.sr(m.from);
        let (sb, rb) = f.sr(m.to);
        let tip = ra.min(rb);
        if m.arc.is_some() {
            parity = parity.wrapping_add(1);
            if parity.is_multiple_of(2) {
                // Every arc END returns one station further along;
                // even counts are the meridian crossings, and both
                // endpoints of a constant-radius half-turn read the
                // same depth, so the end's station IS the center.
                moves.push(revolve::BandMove {
                    a: sb,
                    b: sb,
                    tip,
                    at: i,
                    floor: revolve::Floor::Wave(cot),
                    half: insert_width * 0.5,
                });
            }
        } else {
            parity = 0;
            let c = (sa + sb) * 0.5;
            moves.push(revolve::BandMove {
                a: c,
                b: c,
                tip,
                at: i,
                floor: revolve::Floor::Wave(cot),
                half: insert_width * 0.5,
            });
        }
    }
    revolve::judge(&pts, moves, (tol, excess), 0.0, true, None)
}

/// The hand verdict (issue #629): a half-turn that does not wind the
/// asked hand cuts a groove the drawing never had - a gouge at that
/// move, so the post gate refuses the program by name.
pub fn thread_hand_warnings(path: &Toolpath, hand: ThreadHand) -> Vec<Warning> {
    path.moves
        .iter()
        .enumerate()
        .filter(|(_, m)| m.arc.is_some())
        .filter(|(_, m)| ThreadHand::of_move(m) != Some(hand))
        .map(|(at, _)| Warning::Gouge { at })
        .take(1)
        .collect()
}
