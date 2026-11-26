//! "Pass starts at" for the planar Adaptive pocket (#3355): how the HSM
//! plan honours the shared `PassStart` of #3316. Provenance: our own law
//! from general machining practice (an adaptive cut starts from open
//! air or grows from its own bore, never with a full-width slot along a
//! wall) and the air-side plan of #1950 (Held 1991); no vendor material
//! consulted.

use crate::geo2d::Region;

use super::face::GenError;
use super::adaptive_method::AdaptiveMethod;
use super::level_rings::ring_plan;
use super::pass_start::{PassStart, StartSide};
use super::pocket::{PocketParams, PocketStrategy};
use super::pocket_hsm::{Planned, RINGS_FALLBACK, adaptive_ordered};

/// The Adaptive pocket's passes under `side`. `side.stock` is the stock
/// outline in the area's own frame (`None`: not known here).
///
/// - Auto: [`super::pocket_hsm::adaptive_passes`] with no stock, the plan
///   every earlier file cut, byte for byte.
/// - Outside: each piece is cut from the air past the stock edge inward
///   and ends on its holes' walls (the #1950 air plan). Where a piece
///   has no air side, the Offset rings cut the area from its outer
///   boundary in, and the note says why.
/// - Inside: an area without holes is the growth plan itself - it
///   starts at its bore in the core and walks out to the outer wall. An
///   area with holes cannot start on a hole wall without a full-width
///   slot, so the Offset rings cut it from its holes out, said.
///
/// Every ring fallback is `level_rings::ring_plan`'s (#3316), so it
/// takes that planner's order and its completeness guard. The trochoid
/// fallback is tried only under Auto: its loops follow the medial line
/// and have no ring order.
pub(crate) fn adaptive_passes_from(
    region: &Region,
    center: &Region,
    p: &PocketParams,
    what: &str,
    side: &StartSide,
) -> Result<Planned, GenError> {
    match side.start {
        PassStart::Auto => adaptive_ordered(None, center, p, what, true, None, side.start, side.around_part),
        PassStart::Outside => match side.stock.as_ref().filter(|s| !s.is_empty()) {
            Some(st) => adaptive_ordered(
                Some(region),
                center,
                p,
                what,
                false,
                Some(st),
                side.start,
                side.around_part,
            ),
            None => rings(region, center, p, what, side, "has no stock outline to start from in the air"),
        },
        PassStart::Inside if center.loops().iter().any(|l| l.area() < 0.0) => {
            rings(region, center, p, what, side, "cannot start on a hole wall without a full-width slot")
        }
        PassStart::Inside => {
            adaptive_ordered(Some(region), center, p, what, false, None, side.start, side.around_part)
        }
    }
}

/// Why the Trochoidal method refuses `start` (#3355), or `None`. Its
/// loops follow the area's medial line and have no ring order, so Inside
/// and Outside are refused by name, never ignored.
#[must_use]
pub fn trochoidal_pass_start_refusal(method: AdaptiveMethod, start: PassStart) -> Option<String> {
    (method == AdaptiveMethod::Trochoidal && start != PassStart::Auto).then(|| {
        format!(
            "Pass starts at {} is not available with the Trochoidal method - its loops follow the medial line and have no ring order; use Auto or Spiral",
            start.label()
        )
    })
}

/// The Offset rings in the order `side` asks, with the note that names
/// the switch (an order is never changed in silence).
fn rings(
    region: &Region,
    center: &Region,
    p: &PocketParams,
    what: &str,
    side: &StartSide,
    why: &str,
) -> Result<Planned, GenError> {
    let s = PocketStrategy::OffsetRings;
    let note = format!(
        "adaptive: Pass starts at {} - the HSM plan {why}{RINGS_FALLBACK}{what} in that order instead",
        side.start.label()
    );
    let (passes, ring_notes) = ring_plan(region, center, p, s, side)?;
    let mut notes = vec![note];
    notes.extend(ring_notes);
    Ok((passes, notes, s, None, Vec::new()))
}

#[cfg(test)]
#[path = "pocket_hsm_start_tests.rs"]
mod tests;
