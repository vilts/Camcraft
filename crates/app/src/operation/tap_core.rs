//! Issue #266: a tap's feed moves judged at its thread's core - generate's re-reading of the cutting laws' `Gouge` convictions with `tool_law::cutting_radius` (ISO 724 basic minor diameter); our own reading of tapping practice (a tap rides a hole drilled at tap-drill size and cuts only the thread band), no vendor material consulted.

use crate::drilling::CycleChoice;
use crate::operation::OpParams;
use crate::tools::{Tool, ToolKind};
use crate::verify::Warning;

/// The tool a tap's cutting moves are judged as: `tool` with its
/// diameter cut to its thread's minor (core) diameter, or `None` when
/// the re-reading does not apply - not a Tap-kind tool, not a Z drill
/// cycle (a thread mill or a cross hole keeps its full verdict), or a
/// pitch too coarse to leave a core (`cutting_radius` refuses it, and
/// the full-diameter verdict stands: it can only over-report).
#[must_use]
pub(super) fn core_tool(tool: &Tool, params: &OpParams) -> Option<Tool> {
    if tool.kind != ToolKind::Tap {
        return None;
    }
    // A tap rides a tap-drill hole only in a tapping cycle; a Tap-kind
    // tool forced into a Drill/Peck/Bore cycle keeps its full verdict.
    if !matches!(
        params,
        OpParams::Drill { cross, cycle: CycleChoice::Tap | CycleChoice::FloatingTap | CycleChoice::Auto, .. }
            if cross.is_empty()
    ) {
        return None;
    }
    let core = super::tool_law::cutting_radius(tool).ok()?;
    let diameter = core * 2.0;
    (diameter.is_finite() && diameter > 0.0 && diameter < tool.diameter)
        .then(|| Tool { diameter, ..tool.clone() })
}

/// Drop each `Gouge` in `warns` that the same laws, run with the tap's
/// core (`judge`, handed [`core_tool`]'s tool), do not raise. Only the
/// cutting verdict is re-read: a tap in a hole narrower than its core,
/// or in solid metal, gouges at the core too and stays convicted; every
/// other warning (rapids, shank, holder, fixtures) keeps the full
/// diameter's verdict, because a tap must never rapid into its thread
/// band.
///
/// The solid law reports a move once - `Gouge` when the flutes hit,
/// else the body - so a full-diameter `Gouge` can hide a shank, neck or
/// holder collision on the same move (#1289 review: a Ø6 shank on an M6
/// tap fed past its flute length into a Ø5 hole). For every move whose
/// `Gouge` is dropped, the core run's body verdicts on that move
/// (`ShankHit`, `SweepHit`, `HolderHit`, `FixtureHit`) are carried in,
/// one per kind and move: the acquittal covers the thread band only.
pub(super) fn acquit(
    warns: &mut Vec<Warning>,
    tool: &Tool,
    params: &OpParams,
    judge: impl FnOnce(&Tool) -> Vec<Warning>,
) {
    if !warns.iter().any(|w| matches!(w, Warning::Gouge { .. })) {
        return;
    }
    let Some(core) = core_tool(tool, params) else { return };
    let at_core = judge(&core);
    let dropped: Vec<usize> = warns
        .iter()
        .filter_map(|w| match w {
            Warning::Gouge { at } if !at_core.contains(w) => Some(*at),
            _ => None,
        })
        .collect();
    warns.retain(|w| !matches!(w, Warning::Gouge { .. }) || at_core.contains(w));
    for w in at_core {
        let Some(key) = body_key(&w) else { continue };
        if dropped.contains(&key.1) && !warns.iter().any(|k| body_key(k) == Some(key)) {
            warns.push(w);
        }
    }
}

/// A body verdict's kind and move: the warnings a fluted-edge `Gouge`
/// can mask on its move. `None` for every other warning.
fn body_key(w: &Warning) -> Option<(u8, usize)> {
    match w {
        Warning::ShankHit { at, .. } => Some((0, *at)),
        Warning::SweepHit { at, .. } => Some((1, *at)),
        Warning::HolderHit { at, .. } => Some((2, *at)),
        Warning::FixtureHit { at, .. } => Some((3, *at)),
        _ => None,
    }
}
