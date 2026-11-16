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

use super::infeed::{DepthSchedule, InfeedMethod, Pass, ThreadInfeed};
use super::thread::ThreadParams;

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
///   `repeat_each_depth` - no single cycle states it.
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
    let rec = ThreadCycle {
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
        first,
        last,
        hand: p.hand,
    };
    if m0.kind == MoveKind::Rapid && rec.is_sane(&path.moves) {
        path.threads.push(rec);
    } else {
        path.notes.push(NONE.into());
    }
}
