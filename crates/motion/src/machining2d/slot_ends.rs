//! Slot end extension (#1731): the pass runs past the authored ends by a
//! per-end distance along the slot's own direction, so a channel picked
//! on a face can run out through the part's edge. Elementary vector
//! geometry (a point moved along a unit direction); our own design, no
//! vendor material.

use crate::geo2d::Point2;

use super::face::GenError;

/// The endpoints the pass runs between: `a` moved back by `start` and
/// `b` moved on by `end`, both along `u = (b - a) / |b - a|`. Zero at
/// both ends gives `(a, b)` back exactly (the shape of every earlier
/// slot). A negative or non-finite extension, or coincident endpoints,
/// is refused by name - never a guessed direction.
pub fn extended_ends(a: Point2, b: Point2, start: f64, end: f64) -> Result<(Point2, Point2), GenError> {
    if !start.is_finite() || !end.is_finite() {
        return Err(GenError::Invalid("slot end extensions must be finite".into()));
    }
    if start < 0.0 || end < 0.0 {
        return Err(GenError::Invalid("slot end extensions must not be negative".into()));
    }
    if start == 0.0 && end == 0.0 {
        return Ok((a, b));
    }
    let (dx, dy) = (b.x - a.x, b.y - a.y);
    let len = (dx * dx + dy * dy).sqrt();
    if len == 0.0 || !len.is_finite() {
        return Err(GenError::Invalid("slot endpoints must be distinct".into()));
    }
    let (ux, uy) = (dx / len, dy / len);
    Ok((
        Point2::new(a.x - ux * start, a.y - uy * start),
        Point2::new(b.x + ux * end, b.y + uy * end),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zero_extension_is_the_authored_slot() {
        let (a, b) = (Point2::new(-15.0, 2.0), Point2::new(15.0, 2.0));
        assert_eq!(extended_ends(a, b, 0.0, 0.0).ok(), Some((a, b)));
    }

    #[test]
    fn each_end_moves_along_the_slot_direction() {
        let (a, b) = (Point2::new(0.0, 0.0), Point2::new(3.0, 4.0));
        let Ok((sa, sb)) = extended_ends(a, b, 5.0, 10.0) else {
            panic!("a valid extension")
        };
        assert!((sa.x + 3.0).abs() < 1e-12 && (sa.y + 4.0).abs() < 1e-12, "{sa:?}");
        assert!((sb.x - 9.0).abs() < 1e-12 && (sb.y - 12.0).abs() < 1e-12, "{sb:?}");
    }

    #[test]
    fn bad_extensions_are_refused() {
        let (a, b) = (Point2::new(0.0, 0.0), Point2::new(10.0, 0.0));
        assert!(extended_ends(a, b, -1.0, 0.0).is_err());
        assert!(extended_ends(a, b, 0.0, f64::NAN).is_err());
        assert!(extended_ends(a, b, f64::INFINITY, 0.0).is_err());
        assert!(extended_ends(a, a, 1.0, 0.0).is_err());
    }
}
