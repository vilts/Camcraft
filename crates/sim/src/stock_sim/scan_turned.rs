//! The turned reading of the hidden-rest scan (#1408): a turning nose
//! path's hidden tops judged as the turning work sees them.
//! Provenance: rotation of a point about a line (elementary solid
//! geometry) and the lathe's own working - the work turns under an
//! insert whose nose travels only the axial band its path walks
//! (machine-tool handbooks, turning chapters); nothing from vendor
//! binaries.
//!
//! ## The law here
//!
//! - **The work turns.** A lathe path is written in ONE meridian
//!   plane of the spindle (`turning::Frame`), but the material it cuts
//!   is every point whose orbit about the spindle meets the nose. A
//!   hidden top across the axis from that plane was cut by the same
//!   nose half a turn later, so its distance to the path is read from
//!   its orbit, not from where the facets happen to put it. The orbit
//!   is sampled at the top itself and at its two turns into the path's
//!   plane (both sides of the axis): each sample is a point of the
//!   orbit, so the least distance read can only be at or ABOVE the
//!   orbit's true least - an over-read rest (a refusal), never a
//!   hidden one, even on a path that left its plane.
//! - **A recess the nose never entered is not its rest.** Along an
//!   axial spindle, a hidden top `p` has solid below it, open air up
//!   to `roof` (the part's next surface over it), and solid above
//!   `roof`. Any ball touching `p` from the air side without gouging
//!   `roof` must put its centre strictly inside that open recess: at
//!   or below `p` it stands on the solid side of an upward-facing
//!   surface (`c_z = p_z + r * n_z > p_z`), and at or above `roof` we
//!   have `dist(c, p) > dist(c, roof)`. So a pass whose segments never
//!   enter the open recess `(p, roof)` does not own its floor - a
//!   groove below a facing, a neck past a profile window that ends at
//!   the step (`Z20..Z60` on a `Z60..Z63` neck, #2880), or a neck
//!   below a window starting at its top (`Z63..Z78`). Any pass that
//!   enters the open recess - bridging across it or stopping short
//!   inside it (PR #1429's review) - owns its floor and is judged by
//!   the same distance law as ever. The recess is read along the
//!   scan's Z, so the excuse holds only for a spindle along Z; any
//!   other spindle is judged without it (a refusal, never a pass).
//! - **A crease narrower than the nose is not its rest.** A hidden top
//!   the nose cannot touch at all - the ball tangent there along the
//!   top's normal would cut a neighbouring face, a groove floor's inner
//!   corner under a 0.8 nose - is excused by the ridge law's own
//!   accessibility test (`ridge::Access`), the same excuse the visible
//!   half gives a flanked crease. A top the nose CAN touch is judged.
//! - **The leave is the promise.** A rough told to stand `leave` off
//!   the part rolls a ball `r + leave` wide as far as the hidden half
//!   is concerned: a top standing up to `leave` outside the nose is the
//!   allowance the finish owns (what the visible half's band already
//!   lets stand), and the accessibility test above reads the grown
//!   ball - a groove the rough cannot enter WITH its leave is the
//!   finish's. A top past the leave convicts as ever.
//! - **Mills read as before.** Without a spindle (every mill path,
//!   and a turning path no axis fits) neither clause runs, bit for bit.

use super::field_lathe::LatheAxis;
use super::ridge::Access;

use crate::geometry::{Point3, Triangle};

/// A turning path's spindle frame as the hidden-rest scan reads it:
/// the axis, the stations its nose centres span, the radial
/// direction of the plane they stand in, and the nose-accessibility
/// reader of the part.
pub(super) struct TurnedBand<'a> {
    axis: LatheAxis,
    access: Access<'a>,
    /// The nose radius grown by the leave the op is told to stand,
    /// the leave itself, and the accessibility slack (the op's
    /// tolerance).
    r: f64,
    leave: f64,
    slack: f64,
    /// Each segment's station span, least first - where the nose
    /// stood along the spindle (the recess clause).
    spans: Vec<(f64, f64)>,
    /// Whether the spindle runs along the scan's Z, so that a column's
    /// open span is a span along the spindle (the recess clause).
    axial: bool,
    /// The unit radial of the path's own meridian plane; `None` when
    /// every centre stands on the axis (only the top itself is read).
    radial: Option<[f64; 3]>,
}

impl<'a> TurnedBand<'a> {
    /// The band of `segs` about `axis`, a nose of radius `r` that
    /// stands `leave` off the part, judged at `tol`; `None` when no
    /// segment end is finite (the scan then reads as a mill's).
    pub(super) fn of(
        axis: LatheAxis,
        segs: &[(Point3, Point3)],
        facets: &'a [Triangle],
        (r, leave, tol): (f64, f64, f64),
    ) -> Option<Self> {
        let mut any_finite = false;
        let mut radial = None;
        // A non-finite end stands nowhere known: its span is the whole
        // spindle, so no recess is ever excused by it.
        let spans = segs
            .iter()
            .map(|(a, b)| {
                let (sa, sb) = (axis.split(*a).1, axis.split(*b).1);
                if sa.is_finite() && sb.is_finite() { (sa.min(sb), sa.max(sb)) } else { (f64::NEG_INFINITY, f64::INFINITY) }
            })
            .collect();
        for q in segs.iter().flat_map(|(a, b)| [*a, *b]) {
            if !q.is_finite() {
                continue;
            }
            let (w, s) = axis.split(q);
            if s.is_finite() {
                any_finite = true;
            }
            let n = (w[0] * w[0] + w[1] * w[1] + w[2] * w[2]).sqrt();
            if radial.is_none() && n.is_finite() && n > 1e-9 {
                radial = Some([w[0] / n, w[1] / n, w[2] / n]);
            }
        }
        if !any_finite {
            return None;
        }
        let leave = if leave.is_finite() { leave.max(0.0) } else { 0.0 };
        let axial = (axis.dir()[2].abs() - 1.0).abs() <= 1e-9;
        Some(TurnedBand { axis, access: Access::new(facets, r + leave), r: r + leave, leave, slack: tol, spans, axial, radial })
    }

    /// Whether the nose can touch the hidden top `p` at all: the ball
    /// tangent there along the top's upward normal stands clear of the
    /// part (`ridge::Access`, the ridge law's own accessibility test).
    /// `false` is a crease narrower than the nose - a groove's inner
    /// corner - that no turning pass of this insert reaches (module
    /// law).
    pub(super) fn touchable(&self, p: Point3) -> bool {
        self.access.reachable(p, [0.0, 0.0, 1.0], self.r, self.slack)
    }

    /// The leave the op is told to stand (a rough's finish allowance):
    /// a hidden top that far outside the nose is the promise kept, the
    /// finish pass's to take (module law).
    pub(super) fn leave(&self) -> f64 {
        self.leave
    }

    /// The module law's recess excuse (#1408, #2880): whether the
    /// hidden top `p`, whose open recess runs up the scan's Z to
    /// `roof` (the part's next surface over it), is another
    /// operation's floor - no segment of the path enters the open
    /// recess `(p, roof)`. Any doubt - a spindle off Z, a recess with
    /// no roof, a zero-width or non-finite span - is no excuse: the
    /// top is judged.
    pub(super) fn excuses(&self, p: Point3, roof: f64) -> bool {
        const EPS: f64 = 1e-9;
        if !self.axial || !roof.is_finite() {
            return false;
        }
        let (a, b) = (self.axis.split(p).1, self.axis.split(Point3::new(p.x, p.y, roof)).1);
        if !(a.is_finite() && b.is_finite()) {
            return false;
        }
        let (lo, hi) = (a.min(b), a.max(b));
        if hi <= lo + 2.0 * EPS {
            return false;
        }
        !self.spans.iter().any(|&(s0, s1)| s1 > lo + EPS && s0 < hi - EPS)
    }

    /// `p`'s orbit samples in the path's meridian plane, both sides
    /// of the axis (the module law); empty without a plane.
    pub(super) fn turned(&self, p: Point3) -> Vec<Point3> {
        let Some(u) = self.radial else { return Vec::new() };
        let (w, s) = self.axis.split(p);
        let rho = (w[0] * w[0] + w[1] * w[1] + w[2] * w[2]).sqrt();
        if !(rho.is_finite() && s.is_finite()) {
            return Vec::new();
        }
        let (o, d) = (self.axis.origin(), self.axis.dir());
        let at = |k: f64| {
            Point3::new(
                o.x + d[0] * s + u[0] * k,
                o.y + d[1] * s + u[1] * k,
                o.z + d[2] * s + u[2] * k,
            )
        };
        vec![at(rho), at(-rho)]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_band_spans_the_centres_and_the_orbit_lands_in_their_plane() {
        let segs = [(Point3::new(10.0, 0.0, 48.8), Point3::new(0.0, 0.0, 48.8)), (Point3::new(10.0, 0.0, 50.3), Point3::new(0.0, 0.0, 50.3))];
        let b = TurnedBand::of(LatheAxis::Z, &segs, &[], (0.8, 0.0, 0.05)).expect("a band");
        assert!(b.excuses(Point3::new(-7.0, 0.0, 34.0), 37.0), "a groove floor whose recess the facing never entered");
        assert!(!b.excuses(Point3::new(-7.0, 0.0, 34.0), 49.0), "a recess open across the band is the pass's");
        assert!(!b.excuses(Point3::new(-7.0, 0.0, 34.0), f64::INFINITY), "no roof, no excuse");
        assert!(!b.excuses(Point3::new(-7.0, 0.0, 48.0), 49.0), "a pass standing inside the recess is never excused");
        let t = b.turned(Point3::new(0.0, -7.0, 34.0));
        assert_eq!(t.len(), 2);
        assert!((t[0].x - 7.0).abs() < 1e-12 && t[0].y.abs() < 1e-12 && (t[0].z - 34.0).abs() < 1e-12, "{t:?}");
        assert!((t[1].x + 7.0).abs() < 1e-12, "{t:?}");
    }

    #[test]
    fn a_pass_standing_in_the_recess_owns_its_floor_and_off_z_excuses_nothing() {
        // The review's shortfall: down the OD, into the 34..37 groove
        // at centre station 35.5 - the floor at 34 is still the pass's.
        let segs = [(Point3::new(10.8, 0.0, 48.8), Point3::new(10.8, 0.0, 35.5)), (Point3::new(10.8, 0.0, 35.5), Point3::new(6.3, 0.0, 35.5))];
        let b = TurnedBand::of(LatheAxis::Z, &segs, &[], (0.8, 0.0, 0.05)).expect("a band");
        assert!(!b.excuses(Point3::new(7.0, 0.0, 34.0), 37.0), "it stood in the recess");
        let x = LatheAxis::new(Point3::ZERO, [1.0, 0.0, 0.0]).expect("axis");
        let far = [(Point3::new(40.0, 0.0, 5.0), Point3::new(40.0, 0.0, 0.0))];
        let bx = TurnedBand::of(x, &far, &[], (0.8, 0.0, 0.05)).expect("a band");
        assert!(!bx.excuses(Point3::new(0.0, 0.0, 3.0), 6.0), "off Z the recess is not read: judged");
    }

    #[test]
    fn a_pass_stopping_at_a_neck_step_does_not_own_the_neck_recess() {
        // #2880: a cone pass Z20..Z60 ending right at the step into a
        // Z60..Z63 neck never enters (60, 63), nor does a thread pass
        // Z63..Z78 starting at the neck's top. A pass entering the neck
        // even by 0.1 mm still owns the floor.
        let cone = [(Point3::new(10.8, 0.0, 20.0), Point3::new(5.8, 0.0, 60.0))];
        let bc = TurnedBand::of(LatheAxis::Z, &cone, &[], (0.8, 0.0, 0.05)).expect("a band");
        assert!(bc.excuses(Point3::new(4.8, 0.0, 60.0), 63.0), "ends at step: never entered (60, 63)");
        let thread = [(Point3::new(5.8, 0.0, 63.0), Point3::new(5.8, 0.0, 78.0))];
        let bt = TurnedBand::of(LatheAxis::Z, &thread, &[], (0.8, 0.0, 0.05)).expect("a band");
        assert!(bt.excuses(Point3::new(4.8, 0.0, 60.0), 63.0), "starts at neck top: never entered (60, 63)");
        let into = [(Point3::new(10.8, 0.0, 20.0), Point3::new(5.8, 0.0, 60.1))];
        let bi = TurnedBand::of(LatheAxis::Z, &into, &[], (0.8, 0.0, 0.05)).expect("a band");
        assert!(!bi.excuses(Point3::new(4.8, 0.0, 60.0), 63.0), "entered (60, 63): owns the floor");
    }

    #[test]
    fn a_path_on_the_axis_has_no_plane_and_no_finite_end_no_band() {
        let on = [(Point3::new(0.0, 0.0, 1.0), Point3::new(0.0, 0.0, 2.0))];
        let b = TurnedBand::of(LatheAxis::Z, &on, &[], (0.8, 0.0, 0.05)).expect("a band");
        assert!(b.turned(Point3::new(3.0, 0.0, 1.5)).is_empty());
        let nan = [(Point3::new(f64::NAN, 0.0, 1.0), Point3::new(0.0, f64::NAN, 2.0))];
        assert!(TurnedBand::of(LatheAxis::Z, &nan, &[], (0.8, 0.0, 0.05)).is_none());
    }
}
