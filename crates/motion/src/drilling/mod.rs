//! Drilling: hole recognition from the model's facets, and the cycle
//! generator that turns a hole list into a toolpath carrying both the
//! fully expanded move chain and a [`Cycle`] record per hole.
//!
//! Provenance: cycle semantics (drill, chip-break peck, deep-hole with
//! full retract and dwell, rigid tap, bore and back-bore, spot dimple)
//! follow the public RS-274 / ISO 6983 canned-cycle vocabulary; what a
//! hole IS - a closed cylindrical wall in the facet mesh - is general
//! machining knowledge applied to our own mesh model. No vendor data,
//! no post processors, no HSMWorks material consulted.
//!
//! Law of two truths: `moves` is the physical truth (verify, display,
//! simulation, the .cl dump all read it); `cycles` is the post's truth
//! (one canned block per hole, not a re-derivation from the chain).
//! They are produced together, from the same lattice-snapped numbers,
//! so they agree - and [`tests::agreement_moves_and_cycles_describe_the_same_holes`]
//! pins that: per cycle, starting at `first_move`, the moves that stay
//! on the hole's XY are its group, its first is a cut (positioning
//! rapids precede the group), and the group's deepest point is
//! `bottom_z`.
//!
//! Z-axis holes are the mill's whole story (2.5-D); M5.2 F2 added
//! the lathe's other half: recognition tries the wall's AXIS as a
//! finding (`recognize_cross` reads the same mesh for radial walls)
//! and `cross_drill` plunges them as pose-tagged chains from the
//! turret's X or Y station. The Z code is untouched by the
//! generalization - the candidate passes are independent readings
//! of one weld law. M13.5 A added the lathe's own Z hole: `axial`
//! wraps the same Z kernel in the doors and the meridian verdict an
//! on-axis drill from the turret answers to.

mod axial;
// #985: the bosses an external thread is milled on.
mod boss;
mod cylinder;
mod cross;
mod cycle;
mod fine_bore;
mod hole;
mod merge;
mod mouth;
// #3328: the hole order under the start and exit picks.
mod order;
mod recognize;

#[cfg(test)]
#[allow(unused_imports)]
use {cross::*, cycle::*, hole::*, recognize::*};
#[cfg(test)]
mod tests;
#[cfg(test)]
mod tests_cross;
#[cfg(test)]
mod tests_backbore;
#[cfg(test)]
mod tests_fine_bore;
#[cfg(test)]
mod tests_ream;

pub use axial::{axial_doors, axial_warnings, AXIAL_TOL};
pub use boss::recognize_bosses;
pub use cross::{cross_drill, CrossParams};
pub use cycle::drill;
pub use fine_bore::FINE_BORE_MAX_SHIFT_UM;
pub use hole::{
    auto_kind, drill_point_length, through_tip_run, BreakThroughFrom, CrossHole, CycleChoice,
    CycleParams, Hole, CHIP_BREAK, SPOT_DEPTH, THROUGH_CLEAR,
};
pub use order::{hole_order, in_order, HOLE_PICK_TOL};
pub use recognize::{recognize, recognize_cross};

#[cfg(test)]
mod tests_opposite;
#[cfg(test)]
mod tests_punched_bar;
#[cfg(test)]
mod tests_tilted;
