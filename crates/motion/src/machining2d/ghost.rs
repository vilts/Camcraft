//! The diagnostic ghost (#261 part 3): the moves a generator had built
//! when it refused, handed back beside the refusal so the viewport can
//! show what the operation was doing when it stopped. Display-only: a
//! ghost is never a toolpath - it rides the error, never the operation,
//! so nothing refused can be cached, simulated, verified or posted.
//! Provenance: our own error model over our own CL moves; no vendor
//! material consulted.

use crate::cl::{Move, Toolpath};

/// The moves a refused generation had built. Bounded and finite by
/// construction ([`Ghost::of`]); its `Debug` names only its size, so a
/// refusal logged or asserted on never dumps a path.
#[derive(Clone, PartialEq)]
pub struct Ghost {
    moves: Box<[Move]>,
    truncated: bool,
}

impl Ghost {
    /// The most moves a ghost keeps: past it the first `MAX_MOVES` stand
    /// in for the rest (the overlay's own vertex budget is smaller still),
    /// so a refusal can never hold an unbounded path in memory.
    pub const MAX_MOVES: usize = 100_000;

    /// The ghost of `path`: `None` when it holds no move, or any move
    /// that is not finite (nothing to show, and no NaN may reach the
    /// camera). Only the moves are kept - never cycles or notes, which
    /// are the post's and the status line's business.
    #[must_use]
    pub fn of(path: &Toolpath) -> Option<Ghost> {
        Self::of_moves(&path.moves)
    }

    /// [`Ghost::of`] over a bare move list.
    #[must_use]
    pub fn of_moves(moves: &[Move]) -> Option<Ghost> {
        let kept = moves.get(..Self::MAX_MOVES).unwrap_or(moves);
        if kept.is_empty() || !kept.iter().all(|m| m.is_finite()) {
            return None;
        }
        Some(Ghost { moves: kept.into(), truncated: kept.len() < moves.len() })
    }

    /// The moves, in the order the generator built them.
    #[must_use]
    pub fn moves(&self) -> &[Move] {
        &self.moves
    }

    /// How many moves the ghost holds.
    #[must_use]
    pub fn len(&self) -> usize {
        self.moves.len()
    }

    /// Never true: an empty ghost is no ghost ([`Ghost::of`]).
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.moves.is_empty()
    }

    /// Whether the generator had built more than [`Ghost::MAX_MOVES`].
    #[must_use]
    pub fn truncated(&self) -> bool {
        self.truncated
    }

    /// The ghost carried through `f` (a frame's local-to-world map of a
    /// whole path); `None` when the mapped path has nothing finite left
    /// to show - never a ghost drawn in the wrong frame.
    #[must_use]
    pub fn map_path(&self, f: impl FnOnce(&Toolpath) -> Toolpath) -> Option<Ghost> {
        let local = Toolpath { moves: self.moves.to_vec(), ..Toolpath::default() };
        let mut g = Ghost::of(&f(&local))?;
        g.truncated |= self.truncated;
        Some(g)
    }
}

impl Ghost {
    /// #1015: this ghost with `outer` - the moves an enclosing generator
    /// had built before the inner one refused - in front of it, so a
    /// nested refusal shows the whole program up to where it stopped.
    /// `None` when the joined path is not finite (the caller keeps the
    /// inner ghost alone); the cap and its flag hold over the join.
    #[must_use]
    pub fn after(&self, outer: &[Move]) -> Option<Ghost> {
        let all: Vec<Move> = outer.iter().chain(self.moves.iter()).take(Self::MAX_MOVES + 1).copied().collect();
        let mut g = Ghost::of_moves(&all)?;
        g.truncated |= self.truncated;
        Some(g)
    }
}

impl std::fmt::Debug for Ghost {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Ghost({} moves{})", self.moves.len(), if self.truncated { ", truncated" } else { "" })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cl::MoveKind;
    use crate::geometry::Point3;

    fn feed(x: f64) -> Move {
        Move::new(MoveKind::Feed, Point3::new(x, 0.0, -1.0), Point3::new(x + 1.0, 0.0, -1.0), Some(500.0))
    }

    #[test]
    fn an_empty_or_non_finite_path_has_no_ghost() {
        assert!(Ghost::of(&Toolpath::default()).is_none());
        let bad = Move::new(MoveKind::Feed, Point3::new(f64::NAN, 0.0, 0.0), Point3::new(1.0, 0.0, 0.0), Some(1.0));
        assert!(Ghost::of_moves(&[feed(0.0), bad]).is_none(), "no NaN may reach the camera");
    }

    #[test]
    fn a_huge_path_is_capped_and_says_so() {
        let moves: Vec<Move> = (0..Ghost::MAX_MOVES + 5).map(|i| feed(i as f64)).collect();
        let g = Ghost::of_moves(&moves).expect("ghost");
        assert_eq!(g.len(), Ghost::MAX_MOVES);
        assert!(g.truncated());
        assert_eq!(format!("{g:?}"), format!("Ghost({} moves, truncated)", Ghost::MAX_MOVES));
        let small = Ghost::of_moves(&moves[..3]).expect("ghost");
        assert!(!small.truncated() && small.len() == 3 && !small.is_empty());
    }

    #[test]
    fn the_frame_map_carries_every_move_and_drops_a_non_finite_result() {
        let g = Ghost::of_moves(&[feed(0.0), feed(1.0)]).expect("ghost");
        let shifted = g
            .map_path(|t| {
                let mut t = t.clone();
                for m in &mut t.moves {
                    m.from.x += 10.0;
                    m.to.x += 10.0;
                }
                t
            })
            .expect("still finite");
        assert_eq!(shifted.moves()[1].from.x, 11.0);
        assert!(g.map_path(|_| Toolpath::default()).is_none());
    }

    #[test]
    fn the_outer_moves_stand_in_front_and_a_bad_join_is_refused() {
        // #1015: the nested refusal's merge - outer program first.
        let inner = Ghost::of_moves(&[feed(10.0)]).expect("ghost");
        let g = inner.after(&[feed(0.0), feed(1.0)]).expect("finite join");
        let xs: Vec<f64> = g.moves().iter().map(|m| m.from.x).collect();
        assert_eq!(xs, vec![0.0, 1.0, 10.0]);
        assert!(!g.truncated());
        let bad = Move::new(MoveKind::Feed, Point3::new(f64::NAN, 0.0, 0.0), Point3::new(1.0, 0.0, 0.0), Some(1.0));
        assert!(inner.after(&[bad]).is_none(), "no NaN may reach the camera");
        // The cap holds over the join, and says so.
        let outer: Vec<Move> = (0..Ghost::MAX_MOVES).map(|i| feed(i as f64)).collect();
        let capped = inner.after(&outer).expect("finite");
        assert_eq!(capped.len(), Ghost::MAX_MOVES);
        assert!(capped.truncated());
    }
}
