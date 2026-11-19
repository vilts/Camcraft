//! Cuts in the new frame on a carried field (#637 E2c, route (ii) of the
//! step-E design): the turn's signed permutation is baked into the bricks
//! (`SdfField::turned`, E2a) the first time a carried field is cut, so the
//! view keeps only the translation; the moves are shifted by that
//! translation into the baked field's own frame and subtracted through
//! `Stock::simulate`'s field arm (`simulate_field`): the same tool
//! construction (`spanning_flank`), batched per brick. Every later cut
//! rides the shipped `+Z` code - true XY arcs (#413), the closed-form flat
//! floor - and a flipped bake's floor under an earlier cut is read by the
//! up-march from the records the bake moved (`cut_downs`, E2b).
//!
//! ## The laws here
//!
//! - **A translation moves an arc whole.** `from`, `to` and the centre
//!   shift by the same vector, in the plane's own word order
//!   (`ArcPlane::words`): sense, plane and radius are unchanged, so an XY
//!   arc stays a true arc on the field. The tool axis is a direction and
//!   does not move. Nothing is re-sampled.
//! - **Refusals come first.** `check_ops` runs on the moves as the caller
//!   stated them, before the bake or a single cell is written - a bad
//!   program leaves the carried stock as it was, as `Stock::simulate` does.
//!   A shift that overflows a move to a non-finite value is refused by the
//!   same gate inside `simulate_field`, before any subtraction; the bake
//!   that ran by then changed the representation, never the solid (E2a's
//!   view equivalence).
//! - **The clip stays at read-back.** A cut and the clip block are both
//!   intersections with the carried solid, so they commute; the clip is
//!   applied where it always was (`distance`, `inside`, `column_body`).
//! - **The bake is lazy and once per turn.** A view that is already a pure
//!   translation is cut as it stands; a later `then` adds a new turn to the
//!   view, and the next cut bakes that one.
//!
//! Provenance: rigid-motion invariance of the Euclidean distance; the
//! RS-274 arc's centre words (`cl::plane`); our own design (#637 step-E
//! research comment, section 3).

use super::carry::LatticeTurn;
use super::cut::{check_ops, SimOp};
use super::field_carry::CarriedField;
use super::kernel::StockError;
use crate::cl::Move;
use crate::geo2d::Point2;
use crate::geometry::Point3;

/// `p - s`, the residual view's own arithmetic (`unapply`'s `x - t`).
fn minus(p: Point3, s: Point3) -> Point3 {
    Point3::new(p.x - s.x, p.y - s.y, p.z - s.z)
}

/// One move of the new frame in the baked field's frame: every point less
/// the residual translation, the arc centre in its plane's word order.
fn shifted(m: &Move, s: Point3) -> Move {
    let mut out = *m;
    out.from = minus(m.from, s);
    out.to = minus(m.to, s);
    if let Some(arc) = out.arc.as_mut() {
        let w = arc.plane.words(s);
        arc.center = Point2::new(arc.center.x - w.x, arc.center.y - w.y);
    }
    out
}

impl CarriedField {
    /// Bake the view's signed permutation into the bricks, leaving only its
    /// translation as the view (a no-op for a view that is one already).
    fn bake(&mut self) -> Result<(), StockError> {
        let (m, s) = self.turn.linear();
        if m == [[1, 0], [0, 1]] && s == 1 {
            return Ok(());
        }
        let (g, shift) = self.field.turned(&self.turn);
        let only = LatticeTurn::from_matrix([[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]], [shift.x, shift.y, shift.z])
            .map_err(|e| StockError::Invalid(format!("the carried field's translation: {e}")))?;
        self.field = g;
        self.turn = only;
        Ok(())
    }

    /// Run a program on the carried stock, its moves stated in the NEW
    /// frame (#637 E2c, the module law): the turn is baked into the bricks,
    /// the moves are shifted by the residual translation, and the field arm
    /// of `Stock::simulate` cuts them.
    ///
    /// # Errors
    /// Every refusal `Stock::simulate` makes, by the same words and before
    /// any cell is written.
    pub fn simulate(&mut self, ops: &[SimOp]) -> Result<(), StockError> {
        self.simulate_with(ops, false)
    }

    /// Run a twist-drill program on the carried stock (#2441).
    pub fn simulate_drill(&mut self, ops: &[SimOp]) -> Result<(), StockError> {
        self.simulate_with(ops, true)
    }

    fn simulate_with(&mut self, ops: &[SimOp], drill: bool) -> Result<(), StockError> {
        check_ops(ops)?;
        self.bake()?;
        let shift = self.turn.shift();
        let moved: Vec<Vec<Move>> = ops.iter().map(|op| op.moves.iter().map(|m| shifted(m, shift)).collect()).collect();
        let ops: Vec<SimOp> = ops.iter().zip(&moved).map(|(op, mv)| SimOp { moves: mv, ..*op }).collect();
        super::backend::simulate_field_with(&mut self.field, &ops, drill)
    }
}
