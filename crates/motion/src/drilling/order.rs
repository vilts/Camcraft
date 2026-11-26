//! The hole order of a drilling operation under the user's start and exit
//! picks (#3328, design #3324): the hole under the start pick is cut
//! first, the hole under the exit pick is cut last, and the holes between
//! keep the list order, rotated to the start hole. A pick that is not on
//! a hole, and two picks on one hole of a longer list, are refused by
//! name - a pick is never moved to a different hole.
//!
//! Provenance: our own ordering rule over the general machining practice
//! of a chosen first and last hole (machine-tool handbooks); no vendor
//! material.

use crate::geo2d::Point2;
use crate::path_picks::{PathPick, PathPicks, PickRole};

/// How far outside a hole's circle a pick still counts as on the hole,
/// mm: the rounding of a typed coordinate, not a search distance.
pub const HOLE_PICK_TOL: f64 = 1.0e-6;

/// The index of the hole that `pick` is on: the nearest centre whose
/// circle holds the pick (the earlier hole wins a tie). `None` when the
/// pick is on no hole.
fn hole_under(holes: &[(Point2, f64)], pick: PathPick) -> Option<usize> {
    let mut best: Option<(usize, f64)> = None;
    for (i, (c, r)) in holes.iter().enumerate() {
        let d = (pick.xy.x - c.x).hypot(pick.xy.y - c.y);
        if !(d.is_finite() && r.is_finite()) || d > r + HOLE_PICK_TOL {
            continue;
        }
        if best.is_none_or(|(_, bd)| d < bd) {
            best = Some((i, d));
        }
    }
    best.map(|(i, _)| i)
}

/// The cut order of `holes` (each a centre and a radius) under `picks`,
/// as indices into the list. No pick is the list order itself.
///
/// The rule: the list order, rotated so that the start hole is first;
/// then the exit hole moves to the end. With an exit pick only, the list
/// order keeps its first hole and the exit hole moves to the end.
///
/// # Errors
/// The refusal, by name: a pick that is on no hole, or a start pick and
/// an exit pick on the same hole of a list with more than one hole.
pub fn hole_order(holes: &[(Point2, f64)], picks: &PathPicks) -> Result<Vec<usize>, String> {
    let n = holes.len();
    let mut order: Vec<usize> = (0..n).collect();
    if picks.is_empty() || n == 0 {
        return Ok(order);
    }
    let find = |role: PickRole| -> Result<Option<usize>, String> {
        let Some(pick) = picks.first(role) else {
            return Ok(None);
        };
        hole_under(holes, pick).map(Some).ok_or_else(|| {
            format!(
                "the {what} point ({x:.3}, {y:.3}) is not on a hole of this operation - set it on one of the listed holes, or clear the {what} point",
                what = role.word(),
                x = pick.xy.x,
                y = pick.xy.y,
            )
        })
    };
    let start = find(PickRole::Start)?;
    let exit = find(PickRole::Exit)?;
    if n > 1 && start.is_some() && start == exit {
        return Err(
            "the start point and the exit point are on the same hole - an operation with more than one hole cannot start and end on one hole; move or clear one of the two points"
                .into(),
        );
    }
    if let Some(s) = start {
        order.rotate_left(s.min(n));
    }
    if let Some(e) = exit
        && let Some(at) = order.iter().position(|&i| i == e)
    {
        let hole = order.remove(at);
        order.push(hole);
    }
    Ok(order)
}

/// `items` in `order` (indices from [`hole_order`]); an index outside the
/// list is skipped, never a panic.
#[must_use]
pub fn in_order<T: Clone>(items: &[T], order: &[usize]) -> Vec<T> {
    order.iter().filter_map(|&i| items.get(i).cloned()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row() -> Vec<(Point2, f64)> {
        (0..4).map(|k| (Point2::new(10.0 * f64::from(k), 0.0), 2.0)).collect()
    }

    fn picks(start: Option<(f64, f64)>, exit: Option<(f64, f64)>) -> PathPicks {
        let one = |p: Option<(f64, f64)>| p.map(|(x, y)| PathPick::xy(Point2::new(x, y))).into_iter().collect();
        PathPicks { start: one(start), exit: one(exit) }
    }

    #[test]
    fn no_pick_is_the_list_order() {
        assert_eq!(hole_order(&row(), &PathPicks::default()), Ok(vec![0, 1, 2, 3]));
        assert_eq!(hole_order(&[], &picks(Some((1.0, 1.0)), None)), Ok(vec![]));
    }

    #[test]
    fn the_start_rotates_the_list_and_the_exit_goes_last() {
        assert_eq!(hole_order(&row(), &picks(Some((20.0, 0.0)), None)), Ok(vec![2, 3, 0, 1]));
        assert_eq!(hole_order(&row(), &picks(None, Some((10.0, 0.0)))), Ok(vec![0, 2, 3, 1]));
        assert_eq!(hole_order(&row(), &picks(Some((20.0, 0.0)), Some((30.0, 0.0)))), Ok(vec![2, 0, 1, 3]));
        // A pick inside the circle, off the centre, is on that hole.
        assert_eq!(hole_order(&row(), &picks(Some((11.5, 1.0)), None)), Ok(vec![1, 2, 3, 0]));
    }

    #[test]
    fn a_pick_off_every_hole_and_two_picks_on_one_hole_are_refused_by_name() {
        let off = hole_order(&row(), &picks(None, Some((5.0, 0.0)))).expect_err("between two holes");
        assert!(off.contains("exit point (5.000, 0.000) is not on a hole"), "{off}");
        let nan = hole_order(&row(), &picks(Some((f64::NAN, 0.0)), None)).expect_err("not finite");
        assert!(nan.contains("start point"), "{nan}");
        let same = hole_order(&row(), &picks(Some((10.0, 0.0)), Some((10.5, 0.0)))).expect_err("one hole");
        assert!(same.contains("same hole"), "{same}");
        // One hole: the first hole is also the last one.
        assert_eq!(hole_order(&row()[..1], &picks(Some((0.0, 0.0)), Some((0.5, 0.0)))), Ok(vec![0]));
    }

    #[test]
    fn in_order_skips_an_index_outside_the_list() {
        assert_eq!(in_order(&['a', 'b', 'c'], &[2, 9, 0]), vec!['c', 'a']);
    }
}
