//! Which moves of a cached path its verify warnings name (#261): the
//! set the viewport paints in the gouge colour, and the point a folded
//! warning line frames when clicked.
//! Provenance: our own code over our own `verify::Warning` list; no
//! vendor material consulted.
//!
//! Every located warning carries the index of the move it speaks of
//! (a `Crossing` names two). Those indices come from a cache that may
//! have been read from a project file, so nothing here trusts them:
//! an index past the path's end is ignored, never indexed with.
//! `RestUnjudged` is not a location - it is "reported at the first
//! cutting move" only because a warning must name one - so it marks
//! nothing and frames nothing.

use crate::cl::Move;
use crate::geometry::Point3;
use crate::verify::Warning;

/// The moves `w` names, in its own order (`Crossing`'s pair, or the
/// one `at`); `[None, None]` for a warning that locates nothing.
#[must_use]
pub fn moves_named(w: &Warning) -> [Option<usize>; 2] {
    match w {
        Warning::Crossing { moves } => [Some(moves[0]), Some(moves[1])],
        Warning::Gouge { at }
        | Warning::StockChamferGouge { at }
        | Warning::ShankHit { at, .. }
        | Warning::SweepHit { at, .. }
        | Warning::RapidInStock { at }
        | Warning::FixtureHit { at, .. }
        | Warning::HolderHit { at, .. }
        | Warning::NonFinite { at }
        | Warning::Envelope { at, .. }
        | Warning::Scallop { at, .. }
        | Warning::ProbeSearch { at, .. }
        | Warning::SinkerFinishOrder { at, .. } => [Some(*at), None],
        Warning::RestUnjudged { .. } => [None, None],
    }
}

/// The first move `w` names - the one a click on its line frames.
#[must_use]
pub fn first_move(w: &Warning) -> Option<usize> {
    moves_named(w)[0]
}

/// One flag per move of an `n_moves`-long path: true where some
/// warning names it. Indices at or past `n_moves` are dropped. With no
/// warnings the mask is empty (read it with `.get(i)`), so a clean path
/// costs no allocation per frame.
#[must_use]
pub fn warned_mask(warnings: &[Warning], n_moves: usize) -> Vec<bool> {
    let mut mask = Vec::new();
    for w in warnings {
        for i in moves_named(w).into_iter().flatten() {
            if i < n_moves {
                if mask.is_empty() {
                    mask = vec![false; n_moves];
                }
                if let Some(slot) = mask.get_mut(i) {
                    *slot = true;
                }
            }
        }
    }
    mask
}

/// Where to look at move `at`: the midpoint of its endpoints (an arc's
/// chord midpoint is near enough to frame it). `None` for an index
/// past the path or a non-finite move.
#[must_use]
pub fn move_focus(moves: &[Move], at: usize) -> Option<Point3> {
    let m = moves.get(at)?;
    let mid = Point3::new(
        0.5 * (m.from.x + m.to.x),
        0.5 * (m.from.y + m.to.y),
        0.5 * (m.from.z + m.to.z),
    );
    (mid.x.is_finite() && mid.y.is_finite() && mid.z.is_finite()).then_some(mid)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cl::MoveKind;
    use crate::verify::FixturePart;

    #[test]
    fn every_located_kind_names_its_move_and_the_unjudged_note_names_none() {
        assert_eq!(moves_named(&Warning::Crossing { moves: [3, 9] }), [Some(3), Some(9)]);
        assert_eq!(first_move(&Warning::ShankHit { at: 84, part: true }), Some(84));
        assert_eq!(first_move(&Warning::RapidInStock { at: 2 }), Some(2));
        assert_eq!(first_move(&Warning::FixtureHit { at: 5, fixture_part: FixturePart::Holder }), Some(5));
        // #460: a lathe holder rubbing the stock names its move too.
        assert_eq!(moves_named(&Warning::HolderHit { at: 11, part: true, short_mm: 0.4 }), [Some(11), None]);
        assert_eq!(first_move(&Warning::Envelope { at: 7, gouge_mm: 0.1, rest_mm: 0.0 }), Some(7));
        assert_eq!(moves_named(&Warning::RestUnjudged { at: 0, cells: 1, claims: 1 }), [None, None]);
    }

    #[test]
    fn the_mask_marks_named_moves_and_drops_out_of_range_ones() {
        let w = [
            Warning::Gouge { at: 1 },
            Warning::Crossing { moves: [3, 99] },
            Warning::ShankHit { at: usize::MAX, part: false },
        ];
        assert_eq!(warned_mask(&w, 5), vec![false, true, false, true, false]);
        assert!(warned_mask(&[], 5).is_empty(), "a clean path allocates nothing");
        assert!(warned_mask(&w, 0).is_empty(), "an empty path marks nothing");
        assert!(warned_mask(&[Warning::Gouge { at: 9 }], 3).is_empty(), "all out of range");
    }

    #[test]
    fn the_focus_is_the_move_midpoint_and_refuses_past_the_end() {
        let m = Move::new(MoveKind::Feed, Point3::new(0.0, 0.0, -1.0), Point3::new(10.0, 4.0, -3.0), Some(500.0));
        let f = move_focus(&[m], 0).map(|p| (p.x, p.y, p.z));
        assert_eq!(f, Some((5.0, 2.0, -2.0)));
        assert!(move_focus(&[m], 1).is_none());
        let bad = Move::new(MoveKind::Feed, Point3::new(f64::NAN, 0.0, 0.0), Point3::new(1.0, 0.0, 0.0), None);
        assert!(move_focus(&[bad], 0).is_none());
    }
}
