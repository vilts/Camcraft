//! The operation owns its mark (#1595): the program verdict's credit for
//! the mark an Engrave or a Trace on the finished part was asked to cut.
//!
//! Provenance: our own law over this codebase's column deviation
//! (`deviate`) and its declared-credit walk (`edge_break::credit_floors`,
//! #303); the sweep is the column engine's own atoms. Nothing from
//! vendor binaries or output.
//!
//! The law: the part the program must leave is the model WITH every
//! declared mark cut. A mark is what its operation's own cutter sweeps,
//! no deeper than the operation's budget under the model's top in each
//! column, and never to within [`MARK_FLOOR`] of the underside the
//! material there rests on (the highest underside in the column, read
//! by [`undersides`]): a mark is bounded by the part actually under the
//! cutter, so a plate or a web thinner than the budget is never cut
//! through on credit. A column the verdict convicted that the mark's
//! cutter swept is judged against that floor instead - `max(swept,
//! model top - budget, underside + MARK_FLOOR)`: cut down to it (within
//! `tol`) is the finished part; cut below it - by the mark itself past
//! its budget or through the part, or by another operation under the
//! mark - is a gouge charged at its depth under the floor. A column no
//! mark swept keeps the model's verdict. Rest is untouched: a mark is
//! permission to remove, never a demand.

use super::cut::{simulate, SimOp};
use super::deviate::Deviation;
use super::dexel::Dexel;
use super::edge_break::credit_floors;
use super::kernel::part_surface;

use crate::geometry::Triangle;
use crate::verify::{undersides, MARK_FLOOR};

/// One operation's declared mark: its own moves and cutter, how far
/// under the part's surface it may cut (mm), and which part(s) it marks
/// (`None` for every part in `facets`, `Some(parts)` for only the
/// source part(s) the operation marks, #1627).
#[derive(Clone, Copy, Debug)]
pub struct Mark<'a> {
    pub op: SimOp<'a>,
    pub budget: f64,
    pub target: Option<&'a [Triangle]>,
}

/// One validated mark sweep, ready for the floor lookup.
struct Sweep {
    dex: Dexel,
    budget: f64,
    /// `(target_top, target_seat)` when the mark targets specific part(s);
    /// `None` when it applies to every part in `facets`.
    part: Option<(Vec<f64>, Vec<f64>)>,
}

/// What `mark`'s cutter sweeps on `dex`'s lattice: `+inf` where it never
/// reaches. `None` when the sweep cannot be read (the engine's own
/// refusals) - no credit then, the sharp verdict stands.
fn swept(dex: &Dexel, mark: &Mark) -> Option<Dexel> {
    let mut s = Dexel {
        x0: dex.x0,
        y0: dex.y0,
        cell: dex.cell,
        nx: dex.nx,
        ny: dex.ny,
        stock_z: f64::INFINITY,
        tiles: vec![None; dex.tiles.len()],
        history: None,
        laid: None,
    };
    simulate(&mut s, &[mark.op]).ok()?;
    Some(s)
}

fn prepare_sweep(dex: &Dexel, mark: &Mark) -> Option<Sweep> {
    if !(mark.budget.is_finite() && mark.budget > 0.0) {
        return None;
    }
    let part = match mark.target {
        None => None,
        Some([]) => return None,
        Some(tf) => {
            let under = undersides(tf)?;
            let top = part_surface(tf, dex.x0, dex.y0, dex.cell, dex.nx, dex.ny);
            let seat = part_surface(&under, dex.x0, dex.y0, dex.cell, dex.nx, dex.ny);
            Some((top, seat))
        }
    };
    let s = swept(dex, mark)?;
    Some(Sweep { dex: s, budget: mark.budget, part })
}

/// Re-judge `dev`'s gouge columns against the declared `marks`; returns
/// how many columns the marks cleared or re-charged. `dex` is the
/// verdict's own lattice (the tops `dev` was read from) and `facets`
/// the part it judged. A mark with no positive finite budget declares
/// nothing; a mark with `target: Some(parts)` credits only columns
/// whose top belongs to `parts` (never a neighbouring part), bounded by
/// `parts`' own undersides. A lattice that does not match `dev` credits
/// nothing - the verdict then stands as it was, never looser.
pub fn credit_marks(dev: &mut Deviation, dex: &Dexel, facets: &[Triangle], marks: &[Mark]) -> usize {
    if dev.gouge_cells == 0 {
        return 0;
    }
    let sweeps: Vec<Sweep> = marks.iter().filter_map(|m| prepare_sweep(dex, m)).collect();
    if sweeps.is_empty() {
        return 0;
    }
    // The surface the material under each column's top rests on for
    // whole-program marks (`target: None`); a part whose undersides
    // cannot be read is credited nothing.
    let all_seat = if sweeps.iter().any(|s| s.part.is_none()) {
        undersides(facets).map(|u| part_surface(&u, dex.x0, dex.y0, dex.cell, dex.nx, dex.ny))
    } else {
        None
    };
    credit_floors(dev, dex, facets, |ix, iy, p, _tol| {
        let k = ix + iy * dex.nx;
        sweeps
            .iter()
            .filter_map(|sw| {
                let z = sw.dex.top_at(k);
                if !z.is_finite() {
                    return None;
                }
                let seat = match &sw.part {
                    Some((top, seat)) => {
                        let t = top.get(k).copied().unwrap_or(f64::NEG_INFINITY);
                        if !t.is_finite() || (p - t).abs() > 1e-9 {
                            return None;
                        }
                        seat.as_slice()
                    }
                    None => all_seat.as_deref()?,
                };
                // A column with a top and no underside under it is not a
                // closed part there: nothing to measure a floor from, no credit.
                let kept = seat.get(k).filter(|u| u.is_finite()).map(|u| u + MARK_FLOOR)?;
                Some(z.max(p - sw.budget).max(kept))
            })
            .fold(f64::INFINITY, f64::min)
    })
}
