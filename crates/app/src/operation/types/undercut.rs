//! The Slot Undercut operation's stored parameters (#1444): our own domain model of a side-entry pass for T-slot, keyseat, dovetail and lollipop cutters; the field meanings follow the generator's (`machining2d::UndercutParams`). No vendor material consulted.

use super::height::Height;
use crate::geo2d::Point2;
use crate::path_picks::PathPicks;

/// One slot's undercut: the centreline `a` -> `b` (straight, or a
/// circular arc by `bulge`), cut at one depth by radial passes, entered
/// and left sideways through the station `run_in` behind `a`.
#[derive(Clone, Debug, PartialEq)]
pub struct SlotUndercutOp {
    pub a: Point2,
    pub b: Point2,
    /// The centreline's bulge, DXF style: the tangent of a quarter of
    /// the swept angle, positive counter-clockwise; 0 is straight.
    pub bulge: f64,
    /// Width of the finished undercut at the cutter's widest point, mm.
    pub width: f64,
    /// Distance from `a` back along the entry tangent to the station
    /// where the cutter goes down and comes up, mm. It must stand
    /// outside the stock (or in a pocket cut earlier): the verifier
    /// refuses a Z move with material over the head.
    pub run_in: f64,
    /// Width of the slot cut earlier for the cutter's neck, mm; 0 states
    /// that nothing stands above the neck's path.
    pub neck_slot: f64,
    pub clearance_z: Height,
    /// Where the rapid down ends and the feed down starts; `None` feeds
    /// down from the clearance plane.
    pub retract_z: Option<Height>,
    /// The top of the material over the slot (the region's plane).
    pub top_z: Height,
    /// The floor of the undercut: the cutter tip's plane.
    pub bottom_z: Height,
    /// How far each pair of passes widens the undercut, mm.
    pub stepover: f64,
    pub feed: f64,
    pub plunge_feed: f64,
    pub climb: bool,
    /// The operation's start and exit picks (#3339).
    pub picks: PathPicks,
}

impl SlotUndercutOp {
    /// The structural refusal by name (no world): the numbers the
    /// generator's own door would refuse without a tool in hand. `None`
    /// when sound.
    #[must_use]
    pub fn refusal(&self) -> Option<String> {
        let nums = [
            self.a.x, self.a.y, self.b.x, self.b.y, self.bulge, self.width, self.run_in, self.neck_slot,
            self.stepover, self.feed, self.plunge_feed,
        ];
        if nums.iter().any(|v| !v.is_finite()) {
            return Some("Slot Undercut parameters must be finite".into());
        }
        if self.a == self.b {
            return Some("Slot Undercut endpoints must be distinct".into());
        }
        if self.width <= 0.0 {
            return Some("Slot Undercut width must be positive".into());
        }
        if self.run_in <= 0.0 {
            return Some("Slot Undercut entry run-in must be positive: the cutter goes down outside the wall".into());
        }
        if self.neck_slot < 0.0 {
            return Some("Slot Undercut entry slot width must not be negative".into());
        }
        if self.bulge.abs() > crate::machining2d::UNDERCUT_MAX_BULGE + 1e-12 {
            return Some(format!("Slot Undercut bulge {} sweeps more than a half circle", self.bulge));
        }
        if !(self.stepover > 0.0 && self.feed > 0.0 && self.plunge_feed > 0.0) {
            return Some("Slot Undercut stepover and feeds must be positive".into());
        }
        None
    }

    /// Structural sanity (no world): sound heights and no [`Self::refusal`].
    #[must_use]
    pub fn is_sane(&self) -> bool {
        self.clearance_z.is_sane()
            && self.retract_z.is_none_or(|h| h.is_sane())
            && self.top_z.is_sane()
            && self.bottom_z.is_sane()
            && self.refusal().is_none()
            && self.picks.refusal().is_none()
    }
}
