//! The rotary clause of the document stock (M13.1): which rotary
//! kinds the field eye cuts through the chord pre-pass
//! (`Stock::simulate_rotary`), at what pose tolerance, and why the
//! column VERDICT still refuses them by name. Provenance: our own
//! engines and the M13 radial law; nothing from vendor binaries.
//!
//! ## The law here
//!
//! - **The eye chords the turn; the verdict refuses it.** A rotary
//!   program's moves turn the part under a posed tool. The field eye
//!   (`stock_view`, `stock_play`) cuts them through `simulate_rotary`:
//!   every turning move chorded at the eye's own chord class, never
//!   the un-chorded station-to-station chord (the silent approximation
//!   the old `plan_program` refusal existed to stop). The column
//!   verdict (`simulate_document`) reads heights marched from above:
//!   under a laid bar's equator, and on every ring a standing bar's
//!   lower flank hides, a gouge sits beneath an untouched top and
//!   reads clean - so it does not judge, by name
//!   ([`ROTARY_VERDICT_REFUSAL`]); the radial verdict at generation
//!   governs, and the post gate shows "not judged".
//! - **The eye's pose tolerance is its chord law.** The field chords
//!   every arc at a quarter cell (`backend.rs`' `chord_tol`); the turn
//!   is chorded at the same sag ([`rotary_pose_tol`]), so the picture
//!   is one chord class throughout. Parity at the VERDICT class
//!   (1e-4 sag, sphere-cut columns within `FIELD_VERDICT_EPS` plus the
//!   centre sag) is proved per kind by `crates/app/tests/
//!   rotary_field_parity.rs`; the flute above the ball stands on each
//!   sub-chord's mid pose - a chord-class flank error (`L·α` up the
//!   axis), stated there and in the refusal, never hidden.
//! - **A slice turns from where the head stood.** `rotary_chords`
//!   reads a program's first move as standing on its own pose (no
//!   approach is invented); a playback slice starting mid-program
//!   would then skip its first move's turn. [`simulate_rotary_slice`]
//!   prepends a standing rapid at the pose the program held before the
//!   slice, so the turn is chorded, and the rapid cuts nothing.

use crate::cl::{Move, MoveKind};
use crate::operation::OpParams;
use crate::stock_sim::{RotaryChordStats, SimOp, Stock, StockError};
use crate::verify::RotaryAxis;

/// The column verdict's refusal of a rotary program, said once.
pub const ROTARY_VERDICT_REFUSAL: &str = "a rotary program turns the part under a posed tool: the column verdict's height field is blind under the equator (a gouge there sits beneath an untouched top and would read clean), so it is not judged here - the radial verdict at generation stands; the field eye chords the turn at its chord class (the flute above the ball rides each sub-chord's mid pose, a chord-class flank error, not the verdict's 1e-5 class)";

/// The chord sag the eye allows at the tool's farthest point from the
/// rotary axis: a quarter cell, the same sagitta law the field chords
/// every arc at - the picture is one chord class throughout.
#[must_use]
pub fn rotary_pose_tol(cell: f64) -> f64 {
    (cell * 0.25).max(1e-9)
}

/// The refusal of a rotary kind whose chord-pass parity and cost are
/// not proved on the field (the heavy lane of
/// `rotary_field_parity.rs`), said once. No kind carries it today -
/// a new rotary kind lands here until its lane is green, never on the
/// eye unmeasured.
pub const ROTARY_KIND_UNMEASURED: &str = "a rotary cut turns the part under a posed tool, and this kind's chord-pass parity and cost on the field are not yet proved on its own program (the heavy lane of rotary_field_parity.rs) - not judged here; the radial verdict at generation stands";

/// Which rotary kinds ride the field eye: `Ok(())` for a kind whose
/// chord-pass parity against the radial law is proved and whose cost
/// is sane, `Err(why)` by name for the rest, `None` for a kind that is
/// not rotary at all. Measured 2026-11-06 (`rotary_field_parity.rs`,
/// release, the verdict class 1e-4 then the eye's class cell/4 on the
/// 0.5 mm lattice): Rotary Parallel - 1,196 sphere-cut tops within
/// 1.57e-5 of the law (bound 2.06e-5), 0 sign misses over 7,023 cell
/// centres, 7,099 sub-chords, the eye 729 sub-chords in 2.5 s; Rotary
/// Contour - 276 standing tops exact, 0 misses over 6,940 centres,
/// 92,727 sub-chords, the eye 3,947 in 8.7 s; Rotary Offset - 440
/// standing tops exact, 0 misses over 7,020 centres, 89,685
/// sub-chords, the eye 3,124 in 6.4 s. Rotary Flow - measured
/// 2026-11-25 (#2828, the laid bar's neck band, its passes picked
/// between the rings at x 2 and x 16): 168 sphere-cut tops within
/// 1.27e-5 of the law (bound 2.01e-5), 1,015 standing tops exact,
/// 0 misses over 7,053 centres, 35,491 sub-chords, the eye 2,131
/// sub-chords in 3.0 s.
#[must_use]
pub fn rotary_kind_lift(params: &OpParams) -> Option<Result<(), &'static str>> {
    match params {
        OpParams::RotaryParallel { .. }
        | OpParams::RotaryContour { .. }
        | OpParams::RotaryOffset { .. }
        // #2828: the flow band's chord-pass parity and cost are
        // measured on its own program (the heavy lane of
        // `rotary_field_parity.rs`, the note above) - the eye cuts
        // it; the column verdict still refuses it by name, as it
        // refuses every rotary kind.
        | OpParams::RotaryFlow { .. } => Some(Ok(())),
        _ => None,
    }
}

/// The axis a rotary op's plan carries into the sim: the fitted axis
/// of its source body (`fixture::rotary_axis_of`), or the refusal by
/// name - a rotary program without a fitted axis has no turn to chord.
pub(crate) fn rotary_plan(
    world: &flecs_ecs::prelude::World,
    name: &str,
    op: &crate::schema::Operation,
) -> Result<Option<RotaryAxis>, String> {
    let Some(lift) = rotary_kind_lift(&op.params) else { return Ok(None) };
    lift.map_err(|why| format!("{name}: {why}"))?;
    crate::fixture::rotary_axis_of(world, op)
        .map(Some)
        .ok_or_else(|| format!("{name}: a rotary cut needs its body's fitted axis to chord the turn, and none fits - not judged"))
}

/// Cut `op`'s moves `[a, a + moves.len())` of a rotary program through
/// the chord pre-pass, turning from the pose held before `a` (see the
/// module law). `all` is the op's whole program, `moves` the slice.
pub fn simulate_rotary_slice(
    stock: &mut Stock,
    op: &SimOp,
    all: &[Move],
    a: usize,
    ax: &RotaryAxis,
    pose_tol: f64,
) -> Result<RotaryChordStats, StockError> {
    let held = all.get(..a).and_then(|before| before.iter().rev().find_map(|m| m.axis));
    let first = op.moves.first();
    match (held, first) {
        (Some(axis), Some(m0)) if m0.axis != Some(axis) => {
            let mut v: Vec<Move> = Vec::with_capacity(op.moves.len() + 1);
            v.push(Move { rotary: None, kind: MoveKind::Rapid, from: m0.from, to: m0.from, feed: None, arc: None, axis: Some(axis) });
            v.extend_from_slice(op.moves);
            stock.simulate_rotary(&[SimOp { moves: &v, ..*op }], ax, pose_tol)
        }
        _ => stock.simulate_rotary(std::slice::from_ref(op), ax, pose_tol),
    }
}
