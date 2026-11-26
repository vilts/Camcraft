//! The pilots and turned passes ahead of a turned-family op (#103, #159),
//! read in the meridian rather than through the column grid.
//!
//! Provenance: our own design, split out of `rest.rs` for AGENTS.md's
//! 400-line limit (#318); twist drill point geometry from machine-tool
//! handbooks. No vendor implementation inspected.

use super::ahead::ahead_of;
use crate::cl::Move;
use flecs_ecs::prelude::World;

/// A twist drill ahead in the setup, as the turned family's rapid law
/// credits it (#103): its cut moves, the drill's radius and the cot of
/// its point's half-angle. A drill that states no point (tip angle 0 or
/// 180) is read as the standard 118-degree point: a flat point would
/// credit the full radius down to the tip - MORE removed (review S3).
pub struct DrillAhead {
    pub radius: f64,
    pub cone: Option<f64>,
    pub moves: Vec<Move>,
}

/// A turning op ahead in the setup, as the turned family's rapid law
/// credits it (#159): its moves (nose centres) and its insert's nose
/// radius - a blade or gauge states none and credits its centre line
/// alone (less removed, never more).
pub struct TurnedAhead {
    pub nose: f64,
    /// Half-width of a grooving blade (#2878); 0 for non-blade tools.
    pub band: f64,
    pub moves: Vec<Move>,
}

/// Every turning-family op ahead of `name` in its setup that has cut.
/// Read in the meridian, so a kerf to the axis credits exactly its
/// swept nose - the column grid's undercut objection (#89 review) does
/// not arise.
pub fn turned_ahead(world: &World, name: &str, setup: Option<&str>) -> Vec<TurnedAhead> {
    ahead_of(world, name, setup, true)
        .into_iter()
        .filter(|a| a.cut && a.kind.info().family() == crate::operation::Family::Turn)
        .map(|a| TurnedAhead { nose: a.nose.unwrap_or(0.0), band: a.band, moves: a.moves })
        .collect()
}

/// cot(59 deg): the 118-degree twist drill point (machine-tool handbooks).
const STANDARD_POINT_COT: f64 = 0.600_860_619_027_560_4;

/// Every Drill operation ahead of `name` in its setup that has cut. Read
/// on a lathe too - the axial hole is a cylinder about the spindle, so
/// the meridian law reads it without the column grid (the prior plan's
/// facing-only rule is unchanged).
pub fn drills_ahead(world: &World, name: &str, setup: Option<&str>) -> Vec<DrillAhead> {
    ahead_of(world, name, setup, true)
        .into_iter()
        .filter(|a| a.cut && a.kind == crate::operation::OpType::Drill && a.radius > 0.0)
        .map(|a| DrillAhead { radius: a.radius, cone: a.cone.or(Some(STANDARD_POINT_COT)), moves: a.moves })
        .collect()
}
