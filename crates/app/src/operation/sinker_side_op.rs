//! The Sinker Side Burn operation's seed, creation law and generation
//! (#545): the op's burn resolved as the Sinker Burn's is (#541's
//! `sinker_op` doors - heights, electrode, condition gap), then
//! `sinker_side::side_burn_worn` run over it with the side's own numbers
//! and its measured wear and layer step (#1340).
//!
//! Provenance: our own glue over the motion crate's side-burn
//! generator, whose module head cites its public sources (lateral EDM
//! burning in public machining texts); the condition table is #531's
//! user data (#290 D4).

use flecs_ecs::prelude::World;

use crate::cl::Toolpath;
use crate::geometry::Triangle;
use crate::machining2d::GenError;
use crate::sinker_side::{allowance, side_burn_worn};
use crate::tools::Tool;
use crate::verify::Warning;

use super::kinds::Carried;
use super::types::{OpParams, SinkerSideBurnOp, SinkerSideDirection};
use super::ResolvedHeights;

/// The kind's name in its refusals.
pub const LABEL: &str = "Sinker Side Burn";

/// The side-burn draft: the burn's own draft (#541's seed - no
/// condition named, the user names one of the machine's), moving the
/// +X wall out by 0.1 mm on an electrode 0.2 mm undersize per side.
/// The numbers are a draft to edit, never a machine's values; the
/// allowance law refuses them against the named condition's gap.
pub(super) fn seed_sinker_side_burn(c: &Carried) -> OpParams {
    OpParams::SinkerSideBurn(SinkerSideBurnOp {
        burn: super::sinker_op::burn_draft(c),
        direction: SinkerSideDirection::PlusX,
        amount: 0.1,
        undersize: 0.2,
    })
}

/// The condition's spark gap (the burn's door, under this kind's name),
/// once the side's allowance holds under it.
///
/// # Errors
/// The condition door's refusals, the side's structural refusal, a gap
/// the undersize cannot honour, an amount past the allowance.
pub fn side_gap(world: &World, setup: Option<&str>, op: &SinkerSideBurnOp) -> Result<f64, String> {
    if let Some(why) = op.refusal() {
        return Err(why);
    }
    let gap = super::sinker_op::condition_gap_as(world, setup, LABEL, &op.burn)?;
    allowance(&op.side(), gap)?;
    Ok(gap)
}

/// The creation door (#545, AGENTS.md "validate early"): what
/// `CreateOperation` / `EditOperationParams` / a paste or a retype
/// refuse before the op enters the document - the side's structural
/// refusal, and once a condition is named: a condition the machine
/// does not state (or states no gap for), a gap the undersize cannot
/// honour, a move past the allowance. An unnamed condition is a draft
/// (the ribbon's seed names none) and is refused at generation; a
/// setup off a sinker EDM is the family door's to refuse.
/// `None` for every other kind.
#[must_use]
pub fn creation_refusal(world: &World, setup: Option<&str>, params: &OpParams) -> Option<String> {
    let OpParams::SinkerSideBurn(op) = params else { return None };
    if let Some(why) = op.refusal() {
        return Some(why);
    }
    // Off a sinker EDM the family door says what to mount instead.
    if op.burn.condition.trim().is_empty() || crate::machine::kind_of_setup(world, setup) != crate::machine::Kind::SinkerEdm {
        return None;
    }
    side_gap(world, setup, op).err()
}

/// Generate the side burn.
pub(super) fn generate_sinker_side(
    op: &SinkerSideBurnOp,
    tool: &Tool,
    hs: &ResolvedHeights,
    gap: f64,
    facets: &[Triangle],
) -> Result<(Toolpath, Vec<Warning>), GenError> {
    // The generator's own door: an unsound wear or layer step (#546), an
    // orbit, a side out of range - refused by name, never ignored.
    if let Some(why) = op.refusal() {
        return Err(GenError::Invalid(why));
    }
    let p = super::sinker_op::sinker_params(&op.burn, tool, hs, gap, facets)?;
    // #1340: a measured wear and a layer step compensate the plunge in Z.
    let w = super::sinker_wear_op::side_wear(&op.burn);
    Ok((side_burn_worn(&p, &op.side(), &w)?, Vec::new()))
}
