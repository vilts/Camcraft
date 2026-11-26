//! The centerline room of `slot_room` (#1868): one open, straight chain
//! is the slot's centerline. It gives the slot its two ends and no
//! width; a slot whose ends are not the chain's ends is refused by name.
//!
//! Provenance: our own plane geometry (the spread of a polyline across
//! its chord); no vendor material consulted.

use super::{FIT_TOL, GenError, Point2, Room, STRAIGHT_TOL, SlotBound, at, finite, span};

impl SlotBound {
    /// The room of one picked centerline (an XY polyline in walk
    /// order): straight from its first point to its last.
    ///
    /// # Errors
    /// Fewer than two points, no length in XY, or a chain that is not
    /// straight - a bent centerline is never cut along its chord.
    pub fn from_centerline(pts: &[Point2]) -> Result<SlotBound, GenError> {
        if pts.len() < 2 || !finite(pts) {
            return Err(GenError::Invalid("a slot's picked centerline needs two points".into()));
        }
        let (p, q) = (pts[0], pts[pts.len() - 1]);
        let len = (q.x - p.x).hypot(q.y - p.y);
        if len.is_nan() || len <= STRAIGHT_TOL {
            return Err(GenError::Invalid("the picked centerline has no length in XY from its first end to its last".into()));
        }
        let u = Point2::new((q.x - p.x) / len, (q.y - p.y) / len);
        let n = Point2::new(-u.y, u.x);
        let (t0, t1) = span(pts, n);
        if t1 - t0 > STRAIGHT_TOL {
            return Err(GenError::Invalid(format!(
                "the picked centerline is not straight (it bows {:.3} mm off its chord) - a slot follows a straight centerline; pick a straight open chain or a closed slot outline",
                t1 - t0
            )));
        }
        let (s0, s1) = span(&[p, q], u);
        let (lo, hi) = span(pts, u);
        if lo < s0 - STRAIGHT_TOL || hi > s1 + STRAIGHT_TOL {
            return Err(GenError::Invalid(
                "the picked centerline turns back on itself - a slot follows a straight centerline from one end to the other".into(),
            ));
        }
        let t = 0.5 * (t0 + t1);
        Ok(SlotBound { room: Room::Line, u, n, s0, s1, t0: t, t1: t })
    }

    /// The two ends of a picked centerline; `None` for every other room.
    #[must_use]
    pub fn centerline(&self) -> Option<(Point2, Point2)> {
        matches!(self.room, Room::Line).then(|| (at(self.u, self.n, self.s0, self.t0), at(self.u, self.n, self.s1, self.t0)))
    }

    /// The centerline law: the slot's ends are the chain's ends, in
    /// either order. `Err` holds the reason, without its prefix.
    pub(super) fn confine_line(&self, a: Point2, b: Point2) -> Result<(), String> {
        let Some((p, q)) = self.centerline() else {
            return Ok(());
        };
        let near = |m: Point2, e: Point2| (m.x - e.x).hypot(m.y - e.y) <= FIT_TOL;
        if (near(a, p) && near(b, q)) || (near(a, q) && near(b, p)) {
            return Ok(());
        }
        Err(format!(
            "its ends A ({:.3}, {:.3}) and B ({:.3}, {:.3}) are not the ends of the picked centerline, ({:.3}, {:.3}) and ({:.3}, {:.3}) - pick the centerline again",
            a.x, a.y, b.x, b.y, p.x, p.y, q.x, q.y
        ))
    }
}
