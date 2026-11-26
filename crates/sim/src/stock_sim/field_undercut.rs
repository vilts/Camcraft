//! The undercut cutter in the stock simulation (#1444): a head wider than its neck, cut as the union of two convex solids of revolution - the constructive view of a non-convex cutter (Requicha & Voelcker, "Solid modeling: a historical summary and contemporary assessment", IEEE CG&A 1982) on our signed-distance field, whose running max of cut distances is that union (Frisken et al., "Adaptively sampled distance fields", SIGGRAPH 2000, for the CSG-by-min/max law). Our own implementation; no vendor source consulted.
//!
//! ## The laws here
//!
//! - **The true profile, piece by piece.** The head is the cutter's own
//!   shape: a disc (`SdfTool::Flat`, as tall as the head), a frustum
//!   (`SdfTool::Dovetail`) or a whole ball (`SdfTool::Ball`). The neck is
//!   a cylinder of the neck radius from the neck start up, `neck_len`
//!   long - never the spanning flank a mill gets: nothing above the neck
//!   is removed by a cutter that cuts sideways under a roof. Each piece
//!   is convex, so the field's convex sweep law holds for each; the
//!   field keeps the max of both cut layers, which is their union.
//! - **The column arm cuts nothing.** A column holds one top per (x, y):
//!   it has no way to say "air under material". Lowering the column to
//!   the head would remove the roof, a wrong verdict, so the column arm
//!   leaves its stock as it stood and the undercut stays rest there.
//! - **Upright or refused.** A posed cutting move is refused by name
//!   before a cell is written; the neck's start is measured along +Z.

use camcraft_tools::undercut::{Head, Undercut};

use super::backend::Stock;
use super::cut::{first_posed, POSED_REFUSAL};
use super::field_cut::SdfTool;
use super::kernel::StockError;

use crate::cl::{Move, MoveKind};

/// The head and neck solids of `cutter`, each with the height above the
/// tip its moves ride at.
fn pieces(cutter: &Undercut, neck_len: f64) -> [(f64, SdfTool); 2] {
    let head = match cutter.head {
        Head::Disc => SdfTool::Flat { r: cutter.r, flank: cutter.height },
        Head::Dove { cot } => SdfTool::Dovetail { r: cutter.r, cot, flank: cutter.height },
        Head::Ball => SdfTool::Ball { r: cutter.r },
    };
    [(0.0, head), (cutter.height, SdfTool::Flat { r: cutter.neck_r, flank: neck_len })]
}

impl Stock {
    /// Cut `moves` with the undercut cutter `cutter` on a neck
    /// `neck_len` long (the module law). The field arm removes the head
    /// and the neck; the column arm removes nothing. Rapids touch
    /// nothing on either arm. Bad input is refused by name before any
    /// write.
    pub fn simulate_undercut(&mut self, moves: &[Move], cutter: &Undercut, neck_len: f64) -> Result<(), StockError> {
        let nums = [cutter.r, cutter.height, cutter.neck_r, neck_len];
        if nums.iter().any(|v| !(v.is_finite() && *v > 0.0)) || cutter.neck_r >= cutter.r {
            return Err(StockError::Invalid(format!(
                "undercut cutter (head radius {}, head height {}, neck radius {}, neck length {neck_len}) needs positive finite sizes and a neck under its head",
                cutter.r, cutter.height, cutter.neck_r
            )));
        }
        if !moves.iter().all(|m| m.is_finite()) {
            return Err(StockError::Invalid("an undercut move has a non-finite endpoint".into()));
        }
        if let Some(k) = first_posed(moves) {
            return Err(StockError::Invalid(format!("move {k}: {POSED_REFUSAL}")));
        }
        let Stock::Field(f) = self else {
            return Ok(());
        };
        let chord_tol = (f.cell() * 0.25).max(1e-9);
        let cutting: Vec<Move> = moves.iter().filter(|m| m.kind != MoveKind::Rapid).copied().collect();
        for (lift, tool) in pieces(cutter, neck_len) {
            let lifted: Vec<Move> = cutting
                .iter()
                .map(|m| {
                    let mut m = *m;
                    m.from.z += lift;
                    m.to.z += lift;
                    m
                })
                .collect();
            for chunk in lifted.chunks(super::field_batch::BATCH_MOVES) {
                f.subtract_batch(chunk, &tool, chord_tol)?;
            }
        }
        Ok(())
    }
}
