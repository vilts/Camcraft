//! Sub-resolution feed moves of a 3-D ladder (#3852): a straight feed
//! whose every axis moves less than the finest post resolution posts as
//! a block with the same X, Y and Z as the block before it. Merge it into
//! its straight neighbour, so that the chain of moves stays unbroken.
//!
//! Provenance: this crate's own path clean-up; RS-274 / ISO 6983 word
//! resolution (0.001 mm). No vendor material.

use crate::cl::{Move, MoveKind};

/// The finest metric word resolution a post writes (three decimals).
pub(super) const POST_RESOLUTION_MM: f64 = 1e-3;

/// A straight feed that every post writes with the same X, Y and Z words
/// as the block before it.
fn sub_resolution(m: &Move) -> bool {
    m.kind == MoveKind::Feed
        && m.arc.is_none()
        && [m.to.x - m.from.x, m.to.y - m.from.y, m.to.z - m.from.z]
            .iter()
            .all(|d| d.is_finite() && d.abs() < POST_RESOLUTION_MM)
}

/// `b` can take the motion of `a` into itself: both are straight feeds
/// in the same tool pose. The merged line lies within one resolution
/// step of the two moves it replaces.
fn joinable(a: &Move, b: &Move) -> bool {
    a.kind == MoveKind::Feed
        && b.kind == MoveKind::Feed
        && a.arc.is_none()
        && b.arc.is_none()
        && a.axis == b.axis
        && a.rotary == b.rotary
        && a.to == b.from
}

/// Merge each sub-resolution feed into the next straight feed (its start
/// moves back to the short move's start), else into the previous one
/// (its end moves on). A short move with no straight feed beside it
/// stays: it is never dropped, so the chain never breaks.
pub(super) fn merge_short_feeds(moves: &mut Vec<Move>) {
    let src = std::mem::take(moves);
    let mut out: Vec<Move> = Vec::with_capacity(src.len());
    let mut carry: Option<Move> = None;
    for (i, mv) in src.iter().enumerate() {
        let mut m = *mv;
        if let Some(c) = carry.take() {
            m.from = c.from;
        }
        if sub_resolution(&m) {
            if let Some(next) = src.get(i + 1)
                && joinable(&m, next)
            {
                carry = Some(m);
                continue;
            }
            if let Some(prev) = out.last_mut()
                && joinable(prev, &m)
            {
                prev.to = m.to;
                continue;
            }
        }
        out.push(m);
    }
    *moves = out;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::Point3;

    fn feed(a: (f64, f64, f64), b: (f64, f64, f64)) -> Move {
        Move::new(MoveKind::Feed, Point3::new(a.0, a.1, a.2), Point3::new(b.0, b.1, b.2), Some(1000.0))
    }

    fn chained(moves: &[Move]) -> bool {
        moves.windows(2).all(|w| w[0].to == w[1].from)
    }

    #[test]
    fn a_short_feed_joins_the_next_feed() {
        let mut mv = vec![
            feed((0.0, 0.0, 0.0), (5.0, 0.0, 0.0)),
            feed((5.0, 0.0, 0.0), (5.0004, 0.0003, 0.0)),
            feed((5.0004, 0.0003, 0.0), (10.0, 0.0, 0.0)),
        ];
        merge_short_feeds(&mut mv);
        assert_eq!(mv.len(), 2, "{mv:?}");
        assert_eq!(mv[1].from, Point3::new(5.0, 0.0, 0.0));
        assert_eq!(mv[1].to, Point3::new(10.0, 0.0, 0.0));
        assert!(chained(&mv));
    }

    #[test]
    fn a_short_feed_before_a_rapid_joins_the_previous_feed() {
        let mut mv = vec![
            feed((0.0, 0.0, 0.0), (5.0, 0.0, 0.0)),
            feed((5.0, 0.0, 0.0), (5.0, 0.0009, 0.0)),
            Move::new(MoveKind::Rapid, Point3::new(5.0, 0.0009, 0.0), Point3::new(5.0, 0.0009, 9.0), None),
        ];
        merge_short_feeds(&mut mv);
        assert_eq!(mv.len(), 2, "{mv:?}");
        assert_eq!(mv[0].to, Point3::new(5.0, 0.0009, 0.0), "the rapid starts where the feeds end");
        assert!(chained(&mv));
    }

    #[test]
    fn a_run_of_short_feeds_joins_into_one_move() {
        let mut mv = vec![
            feed((0.0, 0.0, 0.0), (0.0005, 0.0, 0.0)),
            feed((0.0005, 0.0, 0.0), (0.0009, 0.0, 0.0)),
            feed((0.0009, 0.0, 0.0), (3.0, 0.0, 0.0)),
        ];
        merge_short_feeds(&mut mv);
        assert_eq!(mv, vec![feed((0.0, 0.0, 0.0), (3.0, 0.0, 0.0))]);
    }

    #[test]
    fn a_short_feed_without_a_straight_neighbour_stays() {
        let plunge = Move::new(MoveKind::Plunge, Point3::new(0.0, 0.0, 5.0), Point3::new(0.0, 0.0, 0.0), Some(300.0));
        let short = feed((0.0, 0.0, 0.0), (0.0002, 0.0, 0.0));
        let arc = feed((0.0002, 0.0, 0.0), (2.0002, 0.0, 0.0)).with_arc(crate::geo2d::Point2::new(1.0002, 0.0), true);
        let mut mv = vec![plunge, short, arc];
        merge_short_feeds(&mut mv);
        assert_eq!(mv, vec![plunge, short, arc], "a plunge and an arc never take a short move");
    }

    #[test]
    fn a_resolution_step_or_more_stays() {
        let mut mv = vec![feed((0.0, 0.0, 0.0), (0.001, 0.0, 0.0)), feed((0.001, 0.0, 0.0), (4.0, 0.0, 0.0))];
        let before = mv.clone();
        merge_short_feeds(&mut mv);
        assert_eq!(mv, before);
    }
}
