//! The marks the program declares (#1595): the program sim credits an
//! Engrave's or a Trace's cut into the finished part only as the mark
//! its OWN operation carries a law for - its cutter's sweep, no deeper
//! than the operation's mark budget (`operation::mark_law`), and only on
//! the part(s) the operation marks (#1627: its source entities for a
//! part source; every setup part for a stock source, #1607). The budget
//! and target are computed here from the operation as it stands, never
//! read from a cached record, so file data cannot state a deeper mark
//! than the operation does (the #529 bound, by construction). An
//! operation on any other kind declares no mark and is judged against
//! the sharp model - stricter, never looser.
//!
//! Provenance: our own wiring of this repo's mark law onto the sim
//! crate's `credit_marks`. No vendor material consulted.

use super::Planned;
use crate::stock_sim::{Mark, SimOp};

/// The marks the program declares, in the frame the verdict is read in:
/// every op's when one frame holds the whole program, only the final
/// frame's ops' when the program spans setups that hold the part
/// differently (the rule `program_breaks` keeps for the edge breaks).
pub(super) fn program_marks<'a>(
    world: &flecs_ecs::prelude::World,
    planned: &'a [Planned],
    spanned: bool,
    frame: &Option<String>,
) -> Vec<Mark<'a>> {
    planned
        .iter()
        .filter(|p| !spanned || crate::setup::effective(world, p.setup.as_deref()) == *frame)
        .filter_map(|p| {
            let mark = p.mark.as_ref()?;
            Some(Mark {
                op: SimOp { moves: &p.moves, radius: p.radius, ball: p.ball, cone: p.cone, corner: p.corner },
                budget: mark.budget,
                target: mark.target.as_deref(),
            })
        })
        .collect()
}
