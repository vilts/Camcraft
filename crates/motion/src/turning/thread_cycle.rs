//! The thread-cycle record over a thread's laid passes (issue #34,
//! filled from the infeed plan since #416, stating exactly the laid
//! passes in the cycle's terms since #417): the numbers a lathe
//! control's multi-pass threading cycle needs, read off the built path
//! and the plan that laid it.
//!
//! Provenance: the cycle quantities (first-pass and minimum depth,
//! finish allowance, infeed angle, spring passes) as public lathe
//! programming references list them for a multi-pass threading cycle
//! (GSK988TA manual G76, Fanuc-compatible; LinuxCNC G76) - behaviour
//! only; our own mapping from our pass plan.

use crate::cl::{MoveKind, ThreadCycle, Toolpath};
use crate::geometry::Triangle;

use super::infeed::{DepthSchedule, InfeedMethod, Pass, ThreadInfeed};
use super::profile::{silhouette, Frame};
use super::revolve;
use super::stock::StockEnv;
use super::thread::ThreadParams;

/// The axial station span `[lo, hi]` that a G76 rapid infeed sweeps across
/// (issue #2878): every pass rapids from `rec.start` to its depth along the
/// infeed flank line, carrying an insert of width `insert_width` centered on
/// the tip. Returns `None` if any input is non-finite or non-positive.
#[must_use]
pub fn cycle_infeed_band(f: &Frame, rec: &ThreadCycle, insert_width: f64) -> Option<(f64, f64)> {
    if !(insert_width.is_finite() && insert_width > 0.0 && rec.height.is_finite() && rec.height > 0.0) {
        return None;
    }
    let start_s = f.sr(rec.start).0;
    let end_s = f.sr(rec.end).0;
    if !(start_s.is_finite() && end_s.is_finite() && rec.infeed_angle.is_finite()) {
        return None;
    }
    let dir = if end_s < start_s { -1.0 } else { 1.0 };
    let shift = rec.height * (rec.infeed_angle * 0.5).to_radians().tan();
    if !shift.is_finite() {
        return None;
    }
    let s_deep = start_s + dir * shift;
    let half = insert_width * 0.5;
    Some((start_s.min(s_deep) - half, start_s.max(s_deep) + half))
}

/// Whether the part's silhouette stays at or below `root` across the whole
/// axial band `[band_lo, band_hi]` (issue #2878): true in front of the end
/// face (radius 0) or inside a relief groove wider than the insert whose
/// floor does not stand above `root`; false whenever solid part material
/// stands inside the band above `root`.
#[must_use]
pub fn part_band_clear(facets: &[Triangle], f: &Frame, tol: f64, band_lo: f64, band_hi: f64, root: f64) -> bool {
    if facets.is_empty() || !(tol.is_finite() && tol > 0.0 && root.is_finite()) {
        return false;
    }
    let Ok(pts) = silhouette(facets, f, tol * 0.25) else {
        return false;
    };
    let sg = revolve::segs(&pts);
    revolve::span_max(&sg, band_lo, band_hi) <= root + revolve::HAIR
}

/// Attach the record over `moves[first..=last]`, stating the very
/// passes laid (issue #417) in a lathe cycle's own terms, so a control
/// running the cycle cuts what was verified:
/// - first depth: the first pass's;
/// - minimum depth: the rung for a constant-depth ladder (a minimum
///   equal to the first depth makes the cycle's sqrt(n) steps all the
///   rung), the plan's own for a constant-area one;
/// - finish allowance: the depth of the finishing pass - the first pass
///   at the root - below the pass before it. A cycle roughs to
///   `height - allowance` and then finishes at the root; stating 0 when
///   the last roughing rung IS the root would have the control cut that
///   root twice (one pass the verdict never saw). A one-pass thread
///   states the whole height, which no cycle can rough to.
/// - infeed angle: the INCLUDED angle of the compound infeed (twice the
///   flank angle; 0 radial); alternating infeed sets `alternating`;
/// - a plan that repeats every roughing depth (#418) sets
///   `repeat_each_depth` - no single cycle states it; likewise a plan
///   with chip-break dips (#419) sets `chip_break_dips`.
/// - `start_in_air` (#2878): true only when the rapid-infeed band is
///   clear of both the part silhouette and the raw stock envelope.
///
/// A span the builder did not lay as planned gets no record; the
/// passes still post.
#[allow(clippy::too_many_arguments)]
pub(super) fn attach_cycle(
    path: &mut Toolpath,
    first: usize,
    last: usize,
    ref_top: f64,
    root: f64,
    p: &ThreadParams,
    plan: &ThreadInfeed,
    passes: &[Pass],
    f: &Frame,
    sg: &revolve::Segs,
    stock: Option<StockEnv>,
) {
    const NONE: &str = "threading: the passes did not lay as planned - no thread cycle record";
    let infeed_angle = match plan.method {
        InfeedMethod::Radial => 0.0,
        InfeedMethod::Flank | InfeedMethod::Alternating => 2.0 * plan.flank_angle,
    };
    let (Some(m0), Some(end)) = (path.moves.get(first), last.checked_sub(1).and_then(|i| path.moves.get(i)))
    else {
        path.notes.push(NONE.into());
        return;
    };
    let height = ref_top - root;
    let first_depth = passes.first().map_or(p.depth, |q| ref_top - q.tip);
    // The finishing pass is the first one standing at the root; its
    // depth is measured from the pass before it (the whole height when
    // it is the first pass).
    let at_root = passes.iter().position(|q| q.tip <= root + super::revolve::HAIR);
    let finish_allowance = match at_root {
        Some(0) | None => height,
        Some(i) => passes.get(i - 1).map_or(height, |q| q.tip - root),
    };
    let min_depth = match plan.schedule {
        DepthSchedule::ConstantDepth => p.infeed.min(height),
        DepthSchedule::ConstantArea => plan.min_depth,
    };
    let mut rec = ThreadCycle {
        start: m0.to,
        end: end.to,
        major_diameter: 2.0 * ref_top,
        minor_diameter: 2.0 * root,
        lead: p.pitch,
        height,
        first_depth,
        min_depth,
        finish_allowance,
        infeed_angle,
        spring_passes: plan.spring_passes,
        pull_out: 0.0,
        alternating: plan.method == InfeedMethod::Alternating,
        repeat_each_depth: plan.repeat_each_depth,
        chip_break_dips: plan.dips.is_on(),
        first,
        last,
        hand: p.hand,
        start_in_air: false,
    };
    if let Some((band_lo, band_hi)) = cycle_infeed_band(f, &rec, p.insert_width) {
        let part_clear = revolve::span_max(sg, band_lo, band_hi) <= root + revolve::HAIR;
        let stock_clear = match stock {
            None => true,
            Some(s) => s.s_top.is_finite() && rec.toward_minus_z() && s.s_top <= band_lo + revolve::HAIR,
        };
        rec.start_in_air = part_clear && stock_clear;
    }
    if m0.kind == MoveKind::Rapid && rec.is_sane(&path.moves) {
        path.threads.push(rec);
    } else {
        path.notes.push(NONE.into());
    }
}
