//! Laws (f) and (f2), the measured finish-envelope verdict and the posed-contact law of ball-rolling paths - our own measurement on the swept-ball envelope (`stock_sim::envelope_scan`), stated in `verify`'s module law (split from `engine.rs` by M13.7; structure only).

use super::laws::dist2_seg;
use super::types::{Mode, VerifyContext, Warning};

use crate::cl::{MoveKind, Toolpath};

/// (f) Finish envelope - the measured verdict that replaces law (b)
/// for ball-rolling paths (`stock_sim::envelope_scan`): `gouge` is
/// facet truth (M5.2 E), model surface standing inside the swept
/// envelope - the exact distance of each triangle to each ball
/// center, honest on tilts, walls and under-passes where a column
/// floor was none of those; `rest` is distance truth, model
/// standing outside every ball at ridge points - surface points
/// the center path passed twice inside the promise band, where
/// the stepdown law promised tolerance. The band is `tol` wide,
/// so the tolerance is handed to the scan as well as to the gate.
/// Both are
/// judged against `finish_tol` plus the scan's own quantization
/// floor, `cell^2 / (2r)` - the ball's sagitta across one cell,
/// the smallest deviation this measurement can honestly resolve.
/// A verdict tighter than the measurement is a lie in the other
/// direction; the rest this law does NOT report - untouched
/// columns, shelves and crowns and missing-pass gaps - is another
/// op's jurisdiction (rest machining, M2.3), law documented in
/// finishing3d.rs. An Envelope context that cannot measure - no
/// model, no tolerance, an unscanable lattice - has proven
/// nothing, and proves-not reports as a gouge at the first cut,
/// never as silence (the law (b) slice-proof precedent).
pub(super) fn finish_envelope(
    path: &Toolpath,
    cx: &VerifyContext,
    chord_tol: f64,
    warns: &mut Vec<Warning>,
) {
    if matches!(cx.mode, Mode::Envelope | Mode::Rotary) {
        // The envelope ball is the CUT CIRCLE the path rolls: a
        // ground ball's great circle - the tool radius - or a turning
        // insert's smaller nose circle when the tool states one
        // (M5.2 A; `ToolGeom::nose_radius` is 0.0 for every mill
        // kind, so this reads unchanged for all pre-turning paths).
        // A stated nose radius marks the turning convention too: the
        // generator walked the nose CENTER, so the moves are centers
        // already and the scan must not lift them (M5.2 E; a `+r`
        // tilt-lift on an outward-facing cone reads a gouge of
        // r - r/v2 that no ball ever cut).
        let nose_centers = cx.tool.nose_radius.is_finite() && cx.tool.nose_radius > 0.0;
        let r = if nose_centers {
            cx.tool.nose_radius
        } else {
            cx.tool.radius
        };
        let first_cut = path
            .moves
            .iter()
            .position(|m| m.kind != MoveKind::Rapid)
            .unwrap_or(0);
        let verdict = match cx.model.zip(cx.finish_tol) {
            Some((facets, tol)) => match if nose_centers {
                crate::stock_sim::envelope_scan_centers(facets, &path.moves, r, tol)
            } else {
                crate::stock_sim::envelope_scan(facets, &path.moves, r, tol)
            } {
                Ok(rep) => {
                    // The hidden half unjudged (M13.1): said by name,
                    // beside whatever the judged half found - a rest
                    // number that covers only the ridge law must never
                    // pass as the whole verdict.
                    warns.extend(Warning::hidden_unjudged(&rep, first_cut));
                    let gate = tol + rep.cell * rep.cell / (2.0 * r) + 1e-9;
                    // The cutting move nearest a deviation's XY: where
                    // a verdict is reported.
                    let nearest = |q: crate::geo2d::Point2| {
                        let mut at = 0usize;
                        let mut best = f64::INFINITY;
                        for (i, m) in path.moves.iter().enumerate() {
                            if m.kind == MoveKind::Rapid {
                                continue;
                            }
                            for (a, b) in m.chords(chord_tol) {
                                let d = dist2_seg(q, a, b);
                                if d < best {
                                    best = d;
                                    at = i;
                                }
                            }
                        }
                        at
                    };
                    // The spacing's scallop (M13.9): its own non-blocking
                    // kind, beside the envelope verdict, never in it.
                    if rep.scallop_mm > gate {
                        warns.push(Warning::Scallop {
                            at: nearest(rep.scallop_at),
                            mm: rep.scallop_mm.clamp(0.0, 1e9),
                            gate_mm: gate,
                        });
                    }
                    // #864: the gouge half answers to the tolerance
                    // less a coarse model's excess chord error (its true
                    // convex surface stands that far outside the facets
                    // the scan measured); the rest half and the scan's
                    // own band keep the op's tolerance.
                    let gouge_gate = gate - (tol - cx.tightened(tol));
                    if rep.gouge_mm > gouge_gate || rep.rest_mm > gate {
                        Some(Warning::Envelope {
                            at: nearest(rep.at),
                            gouge_mm: rep.gouge_mm.clamp(0.0, 1e9),
                            rest_mm: rep.rest_mm.clamp(0.0, 1e9),
                        })
                    } else {
                        None
                    }
                }
                Err(_) => Some(Warning::Gouge { at: first_cut }),
            },
            None => Some(Warning::Gouge { at: first_cut }),
        };
        warns.extend(verdict);
        // (f2) The posed-contact law: a LEANING cut claims its ball
        // rests on the part (the lean laws are functions of the
        // contact normal), so a leaning ball standing off the skin is
        // a pose and a position that disagree - the pivot-law failure
        // the ridge rest law cannot see (it judges only points two
        // passes came near; an all-air pass is near nothing). Mill
        // balls in Envelope only: the rotary kinds pose radially in
        // air by design (their radial field law owns them), and a
        // nose-centre turning path never leans.
        if cx.mode == Mode::Envelope
            && !nose_centers
            && let Some((facets, tol)) = cx.model.zip(cx.finish_tol)
        {
            warns.extend(super::contact::posed_contact(path, facets, cx.tool, r, tol, cx.stock_to_leave));
        }
    }
}
