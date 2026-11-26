//! The plasma profile operation's stored parameters (#670): what the
//! op keeps, as heights and plain numbers - resolved into the motion
//! crate's `plasma::PlasmaParams` at generation.
//!
//! Provenance: our own parameter set over public plasma practice -
//! Hypertherm's cut-direction guidance, the LinuxCNC Plasma Primer §13
//! small-hole rule, and Tormach's public 1300PL manual / PathPilot post
//! guidelines (feed, pierce height, cut height and delay are optional:
//! blank leaves them to the controller's AutoFS).

use super::height::Height;
use crate::path_picks::PathPicks;

/// Where the kerf offset is made: walked by the computer (the default -
/// the path is the torch centre), or posted as G41/G42 for the control
/// to offset (the CL then carries the part contour beside the centre
/// path, with straight engage / cancel leads longer than the kerf
/// radius - Tormach's PathPilot rule).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PlasmaComp {
    #[default]
    Computer,
    Control,
}

/// The plasma profile's parameters. Every `Option` blank is the
/// controller's decision, never a guessed number.
#[derive(Clone, Debug, PartialEq)]
pub struct PlasmaOp {
    /// The torch-off travel plane.
    pub clearance_z: Height,
    /// The plate's top: the plane the cut rides (plus the cut height,
    /// when stated).
    pub top_z: Height,
    /// The plate's bottom: with the top it states the thickness the
    /// small-hole rule reads. Nothing steps down to it - a plasma cut
    /// goes through in one pass.
    pub bottom_z: Height,
    /// The kerf width override, mm; `None` reads the torch tool's
    /// diameter (its kerf width).
    pub kerf: Option<f64>,
    pub comp: PlasmaComp,
    /// Outer contours counter-clockwise and holes clockwise - the
    /// override of the square-edge-on-the-part default, stated in the
    /// path's notes and its CL header.
    pub reversed: bool,
    /// Lead-in / lead-out length, mm (an arc in computer compensation,
    /// a straight line under G41/G42); 0 = none.
    pub lead_in: f64,
    pub lead_out: f64,
    /// Holes under this diameter (or under 1.5x the plate thickness)
    /// take the small-hole rule, mm.
    pub small_hole_dia: f64,
    /// The small-hole feed factor, (0, 1] - 0.65 by default.
    pub small_hole_feed: f64,
    /// A small hole's overburn past its start, mm; 0 = none.
    pub overburn: f64,
    /// Cut feed, mm/min; `None` = the controller's material table.
    pub feed: Option<f64>,
    /// Pierce height / cut height above the plate, mm, and the pierce
    /// delay, s; `None` = the controller decides.
    pub pierce_height: Option<f64>,
    pub cut_height: Option<f64>,
    pub pierce_delay: Option<f64>,
    /// The operation's start and exit picks (#3339).
    pub picks: PathPicks,
}

impl PlasmaOp {
    /// Structural sanity (no world): every number finite and in range.
    /// The motion crate's door repeats the refusals by name at
    /// generation; this is the command door's yes / no.
    #[must_use]
    pub fn is_sane(&self) -> bool {
        let nneg = |v: f64| v.is_finite() && v >= 0.0;
        let opt = |v: Option<f64>| v.is_none_or(nneg);
        self.clearance_z.is_sane()
            && self.top_z.is_sane()
            && self.bottom_z.is_sane()
            && self.kerf.is_none_or(|k| k.is_finite() && k > 0.0)
            && nneg(self.lead_in)
            && nneg(self.lead_out)
            && nneg(self.small_hole_dia)
            && self.small_hole_feed.is_finite()
            && self.small_hole_feed > 0.0
            && self.small_hole_feed <= 1.0
            && nneg(self.overburn)
            && self.feed.is_none_or(|f| f.is_finite() && f > 0.0)
            && opt(self.pierce_height)
            && opt(self.cut_height)
            && opt(self.pierce_delay)
            && self.picks.refusal().is_none()
    }
}
