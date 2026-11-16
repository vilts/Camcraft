//! The Sinker Burn's rough and finish passes (#543): a rough or finish
//! burn's condition undersize, read from the setup machine's condition
//! table (#531), and - for a finish - the undersize of the rough burn
//! it follows, so the order law can warn on an inverted pair.
//!
//! Provenance: our own glue over the motion crate's undersize law
//! (`sinker_undersize`, whose module head cites its public sources:
//! general die-sink EDM practice); the condition table is #531's user
//! data (#290 D4).

use flecs_ecs::prelude::World;

use crate::machine::ConditionField;
use crate::model;
use crate::sinker_undersize::{SIZE_SLACK, SinkerPass};

use super::types::{OpParams, SinkerBurnOp};

/// What a rough or finish burn reads from the world.
#[derive(Clone, Debug, PartialEq)]
pub struct PassRead {
    /// The burn's own condition's electrode undersize per side, mm.
    pub undersize: f64,
    /// For a finish: the rough burn it follows - that op's name and its
    /// condition's undersize. `None` when no rough of the same cavity
    /// precedes it in the setup (a finish may stand alone).
    pub rough: Option<(String, f64)>,
}

/// The undersize of the burn's condition, by the same door as its gap
/// (`sinker_op::condition_gap`): a sinker EDM, a named condition that
/// states an undersize.
fn condition_undersize(world: &World, setup: Option<&str>, op: &SinkerBurnOp) -> Result<f64, String> {
    let c = super::sinker_op::condition(world, setup, op, &[ConditionField::Gap, ConditionField::Undersize])?;
    let id = op.condition.trim();
    c.undersize
        .ok_or_else(|| format!("EDM condition {id:?} states no undersize - a {} burn reads it", op.pass.label()))
}

/// Does `other` burn the same cavity at the same point as `op`?
fn same_cavity(op: &SinkerBurnOp, other: &SinkerBurnOp) -> bool {
    let near = |a: f64, b: f64| (a - b).abs() <= SIZE_SLACK;
    near(op.x, other.x) && near(op.y, other.y) && near(op.cavity_x, other.cavity_x) && near(op.cavity_y, other.cavity_y)
}

/// The rough burn a finish follows: the nearest Sinker Burn before it
/// in program order, in the same setup, not suppressed, marked rough and
/// burning the same cavity at the same point - with its condition's
/// undersize. A rough whose own condition does not read is skipped: its
/// own generation refuses it by name.
fn rough_before(world: &World, op_name: &str, setup: Option<&str>, op: &SinkerBurnOp) -> Option<(String, f64)> {
    let mine = crate::setup::effective(world, setup);
    let order = crate::setup::operation_order(world);
    let at = order.iter().position(|(n, _)| n == op_name)?;
    order[..at].iter().rev().find_map(|(name, e)| {
        let o = model::operation_of(world, e)?;
        if o.suppressed || crate::setup::effective(world, o.setup.as_deref()) != mine {
            return None;
        }
        let OpParams::SinkerBurn(r) = &o.params else { return None };
        if r.pass != SinkerPass::Rough || !same_cavity(op, r) {
            return None;
        }
        let u = condition_undersize(world, o.setup.as_deref(), r).ok()?;
        Some((name.clone(), u))
    })
}

/// A rough or finish burn's reading; `None` for a single burn (#541's
/// law reads only the gap).
///
/// # Errors
/// The condition refusals of `condition_undersize`, by name.
pub fn read_pass(world: &World, op_name: &str, setup: Option<&str>, op: &SinkerBurnOp) -> Option<Result<PassRead, String>> {
    if op.pass == SinkerPass::Single {
        return None;
    }
    Some(condition_undersize(world, setup, op).map(|undersize| PassRead {
        undersize,
        rough: (op.pass == SinkerPass::Finish).then(|| rough_before(world, op_name, setup, op)).flatten(),
    }))
}
