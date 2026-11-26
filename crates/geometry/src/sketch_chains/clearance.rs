//! Analytic disk and primitive clearance using the sketch chainer's contact and ray tests.

use super::crossing::Sealed;
use super::curve::Piece;
use super::{Budget, ChainError, ClosedChain, WORK_LIMIT, dist_to_primitive, intersect, nesting};
use crate::dxf::Primitive;
use crate::geo2d::Point2;

/// A simple analytic loop supplied by the chainer. Bound the total work for one drawing.
pub struct ChainClearance {
    ring: Sealed,
    budget: Budget,
}

impl ChainClearance {
    pub fn new(chain: &ClosedChain) -> Result<Self, ChainError> {
        let pieces = chain
            .segments
            .iter()
            .map(|s| Piece::from_primitive(s.primitive, &s.curve))
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Self {
            ring: Sealed::new(&pieces),
            budget: Budget(WORK_LIMIT),
        })
    }

    /// Inside the loop, with at least the stated clearance from its full analytic boundary.
    pub fn contains_disk(&self, p: Point2, radius: f64) -> bool {
        nesting::contains(&self.ring, p)
            && self
                .ring
                .pieces
                .iter()
                .all(|piece| dist_to_primitive(p, &piece.oriented()) > radius)
    }

    /// Is any point of this curve inside, touching, or crossing the selected loop?
    pub fn obstructs(
        &mut self,
        index: usize,
        other: &Primitive,
        eps: f64,
    ) -> Result<bool, ChainError> {
        let q = Piece::from_primitive(index, other)?;
        let (lo, hi) = q.bounds();
        let separate = |a: Point2, b: Point2| {
            hi.x + eps < a.x || b.x + eps < lo.x || hi.y + eps < a.y || b.y + eps < lo.y
        };
        if separate(self.ring.lo, self.ring.hi) {
            return Ok(false);
        }
        self.budget.spend(self.ring.pieces.len())?;
        let mut contacts = Vec::new();
        for p in &self.ring.pieces {
            let (a, b) = p.bounds();
            if !separate(a, b) {
                intersect::contacts(p, &q, eps, &mut contacts);
                if !contacts.is_empty() {
                    return Ok(true);
                }
            }
        }
        Ok(nesting::contains(&self.ring, q.a))
    }
}
