//! The travel budget of one rest Adaptive depth (issue 2109): how far
//! the plan may feed out of cut, and how many passes it may reach by a
//! retract, before it is refused by name. Provenance: the plain
//! Adaptive's budget (`pocket_hsm::over_budget`: out-of-cut feed against
//! cut feed, retract hops a level), read here on what the passes
//! remove: a feed is in cut where it clears rest material at least a
//! graze deep on average, the material-removal reading of peripheral milling
//! (Y. Altintas, "Manufacturing Automation", 2nd ed., Cambridge 2012,
//! ch. 2). The limits are this project's, recorded in
//! `docs/decisions/2109-rest-adaptive-air.md`.
//!
//! Why numbers of its own: a rest pass is reached stay-down only along
//! chords it has cut before (the verifier's crossing law), so each pass
//! of a corner goes back along the one before it - about one pass of
//! air per pass of cut, where the plain Adaptive lifts its returns. And
//! each rest area is cut off from the others by material, so it takes
//! an entry of its own on top of the plain plan's hops.

use crate::geo2d::Point2;
use crate::machining2d::GenError;
use crate::machining2d::pocket_hsm::HOP_BUDGET;

use super::super::front::dist;
use super::super::raster::Raster;

/// Most feed out of cut, as a multiple of the feed in cut, one depth.
pub(super) const REST_NONCUT_PER_CUT: f64 = 3.0;

/// The travel of one depth's plan (mm), and its retract hops.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(super) struct Travel {
    pub cut: f64,
    pub air: f64,
    pub hops: usize,
}

/// The travel of `chains` (each entered by a plunge) walked in order
/// on `stock`, the rest material the passes are planned to cut: a feed
/// that clears at least `graze` mm of it on average over its length is
/// in cut, the rest is air.
pub(super) fn travel(mut stock: Raster, chains: &[Vec<Point2>], r: f64, graze: f64) -> Travel {
    let cell = stock.h * stock.h;
    let mut t = Travel { hops: chains.len().saturating_sub(1), ..Travel::default() };
    for chain in chains {
        if let Some(&s) = chain.first() {
            stock.cut_count(s, s, r);
        }
        for w in chain.windows(2) {
            let l = dist(w[0], w[1]);
            let cleared = stock.cut_count(w[0], w[1], r) as f64 * cell;
            if l > 0.0 && cleared >= graze * l {
                t.cut += l;
            } else {
                t.air += l;
            }
        }
    }
    t
}

/// A depth at `z` with travel `t` over `areas` rest areas: `Ok` within
/// the budget, refused by name past it.
pub(super) fn check(t: &Travel, areas: usize, z: f64) -> Result<(), GenError> {
    let why = if t.air > REST_NONCUT_PER_CUT * t.cut {
        format!("would feed {:.0} mm out of cut for {:.0} mm in cut (over {REST_NONCUT_PER_CUT:.1}x)", t.air, t.cut)
    } else if t.hops > hop_budget(areas) {
        format!(
            "would reach {} passes by a retract (over {}: one for each of its {areas} rest areas after the first, and {HOP_BUDGET} more)",
            t.hops,
            hop_budget(areas)
        )
    } else {
        return Ok(());
    };
    Err(GenError::Invalid(format!("rest Adaptive at Z{z:.3} {why} - choose Waterline or Offset rings")))
}

/// Retract hops a depth may take: one to each rest area after the
/// first, and the plain Adaptive's `HOP_BUDGET` more.
pub(super) fn hop_budget(areas: usize) -> usize {
    areas.saturating_sub(1).saturating_add(HOP_BUDGET)
}
