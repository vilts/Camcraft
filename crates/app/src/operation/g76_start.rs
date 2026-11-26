//! Issue #2878: Fanuc G76 free-air start check in the meridian plane against the revolved part, remaining stock and prior turning ops - the rapid-infeed behaviour from public Fanuc lathe programming manuals read onto our own meridian outline; no vendor material consulted.

use crate::cl::{MoveKind, ThreadCycle, Toolpath};
use crate::turning::g76_clearance::{rectangle_clear, AheadCut, PilotCut};
use crate::turning::profile::Frame;
use crate::turning::{cycle_infeed_band, part_band_clear};

use super::drill_prior::PriorCell;
use super::turning_credit::pilots;
use super::turning_outline::Revolved;
use super::turning_rapids::{faced_end, kerf_of, Meridian, SignedMeridian};
use super::OpParams;

/// Refine every [`ThreadCycle::start_in_air`] flag on a generated Threading
/// path against the revolved part, the setup's stock and every prior op's
/// cut (Facing, Grooving, Turning, Drilling). Non-threading operations and
/// paths without thread records are left untouched.
pub(super) fn judge(params: &OpParams, path: &mut Toolpath, cx: &Meridian, prior: &mut PriorCell) {
    let OpParams::Threading { tolerance, .. } = params else {
        return;
    };
    if path.threads.is_empty() {
        return;
    }
    let insert_width = cx.band * 2.0;
    let Ok(frame) = Frame::about(cx.part) else {
        for rec in &mut path.threads {
            rec.start_in_air = false;
        }
        return;
    };
    let sm = SignedMeridian::of(&frame, path);
    let stock = match cx.stock {
        Some(facets) => match Revolved::of(facets, &frame) {
            Some(rev) => Some(rev),
            // Unreadable stock is treated as solid (Prime Directive).
            None => {
                for rec in &mut path.threads {
                    rec.start_in_air = false;
                }
                return;
            }
        },
        None => None,
    };
    let mut cap_cell: Option<Option<f64>> = None;
    let pilot_cuts: Vec<PilotCut> = pilots(cx.drills, &sm)
        .into_iter()
        .map(|p| PilotCut {
            lo: p.lo,
            hi: p.hi,
            r: p.r,
            cot: p.cot,
        })
        .collect();
    for rec in &mut path.threads {
        rec.start_in_air = record_in_air(
            rec,
            insert_width,
            *tolerance,
            &frame,
            &sm,
            stock.as_ref(),
            &pilot_cuts,
            &mut cap_cell,
            prior,
            cx,
        );
    }
}

#[allow(clippy::too_many_arguments)]
fn record_in_air(
    rec: &ThreadCycle,
    insert_width: f64,
    tol: f64,
    frame: &Frame,
    sm: &SignedMeridian,
    stock: Option<&Revolved>,
    pilot_cuts: &[PilotCut],
    cap_cell: &mut Option<Option<f64>>,
    prior: &mut PriorCell,
    cx: &Meridian,
) -> bool {
    let Some((band_lo, band_hi)) = cycle_infeed_band(frame, rec, insert_width) else {
        return false;
    };
    let root = rec.minor_diameter * 0.5;
    if !(root.is_finite() && root >= 0.0) {
        return false;
    }
    // 1. The finished part itself must have no material above `root`
    //    anywhere in the rapid-infeed band.
    if !part_band_clear(cx.part, frame, tol, band_lo, band_hi, root) {
        return false;
    }
    // 2. When stock is present, every point of the stock standing above
    //    `root` inside `[band_lo, band_hi]` must have been cleared by a
    //    prior Facing, Grooving, Turning or Drilling operation.
    let Some(stock) = stock else {
        return true;
    };
    let ahead_cuts: Vec<AheadCut> = cx
        .ahead
        .iter()
        .flat_map(|a| {
            a.moves
                .iter()
                .filter(|m| m.kind != MoveKind::Rapid)
                .filter_map(move |m| {
                    let (p, q) = kerf_of(m, sm, cx.z_tol)?;
                    let reach = a.band.max(a.nose) + cx.margin;
                    let (lo, hi) = (p.0.min(q.0) - reach, p.0.max(q.0) + reach);
                    (hi >= band_lo && lo <= band_hi).then_some(AheadCut {
                        p,
                        q,
                        nose: a.nose,
                        band: a.band,
                    })
                })
        })
        .collect();
    // A polyline's extrema lie at its breakpoints. Bound all remaining
    // stock in each interval by a solid rectangle; do not infer a hole
    // from isolated inner-radius rays. This can refuse uncertain hollow
    // stock, but cannot hide material between axial or radial stations.
    let mut stations = stock.knots();
    stations.retain(|s| *s > band_lo && *s < band_hi);
    stations.extend([band_lo, band_hi]);
    stations.sort_unstable_by(f64::total_cmp);
    stations.dedup();
    for span in stations.windows(2) {
        let (lo, hi) = (span[0], span[1]);
        let outer = stock.annulus(lo).1.max(stock.annulus(hi).1);
        if outer <= root {
            continue;
        }
        let cap = *cap_cell.get_or_insert_with(|| faced_end(prior, frame, sm, cx));
        if cap.is_some_and(|c| lo >= c + cx.margin) {
            continue;
        }
        if !rectangle_clear((lo, hi, root, outer), &ahead_cuts, pilot_cuts, cx.margin) {
            return false;
        }
    }
    true
}

#[cfg(test)]
#[path = "g76_start_tests.rs"]
mod tests;
