//! The waterjet profile operation's stored parameters (#1883): what the
//! op keeps, as heights and plain numbers - resolved into the motion
//! crate's `waterjet::WaterjetParams` at generation.
//!
//! Provenance: our own parameter set over general abrasive-waterjet
//! practice (kerf side, leads with the pierce in the scrap, tabs as
//! uncut gaps, pierce and abrasive dwell) and RS-274 / ISO 6983 for
//! G41/G42 and linear-only controls. No torch height or THC field.

use super::height::Height;
use crate::path_picks::PathPicks;

/// Which side of an open run's travel the kerf stands on.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum JetSide {
    #[default]
    Center,
    Left,
    Right,
}

/// The shape of the leads.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum JetLead {
    #[default]
    Arc,
    Straight,
}

/// Tabs left as uncut gaps: count, width (mm) and one position per tab
/// (world X, Y), near a closed contour.
#[derive(Clone, Debug, PartialEq)]
pub struct JetTabsOp {
    pub count: u32,
    pub width: f64,
    pub at: Vec<[f64; 2]>,
}

/// The waterjet profile's parameters.
#[derive(Clone, Debug, PartialEq)]
pub struct WaterjetOp {
    /// The jet-off travel plane.
    pub clearance_z: Height,
    /// The sheet's top: the plane the cut rides.
    pub top_z: Height,
    /// The sheet's bottom: nothing steps down to it - a waterjet cut goes
    /// through in one pass.
    pub bottom_z: Height,
    /// The kerf width override, mm; `None` reads the nozzle tool's
    /// diameter (its kerf width).
    pub kerf: Option<f64>,
    /// Post G41/G42 for the control to offset, instead of the computer's
    /// offset path.
    pub control_comp: bool,
    /// Outer contours counter-clockwise and holes clockwise.
    pub reversed: bool,
    pub lead: JetLead,
    /// Lead-in / lead-out length, mm; 0 = none.
    pub lead_in: f64,
    pub lead_out: f64,
    /// The kerf side of open runs.
    pub open_side: JetSide,
    /// `None` = no tabs.
    pub tabs: Option<JetTabsOp>,
    /// Cut feed, mm/min (the stated cut data).
    pub feed: f64,
    /// The pierce dwell and the abrasive's delay before it, s.
    pub pierce_time: f64,
    pub abrasive_delay: f64,
    /// Arcs sent as chords within this tolerance, mm; `None` keeps them.
    pub linearize: Option<f64>,
    /// The operation's start and exit picks (#3339).
    pub picks: PathPicks,
}

impl WaterjetOp {
    /// Structural sanity (no world): every number finite and in range.
    /// The motion crate's door repeats the refusals by name at
    /// generation; this is the command door's yes / no.
    #[must_use]
    pub fn is_sane(&self) -> bool {
        let nneg = |v: f64| v.is_finite() && v >= 0.0;
        let pos = |v: f64| v.is_finite() && v > 0.0;
        self.clearance_z.is_sane()
            && self.top_z.is_sane()
            && self.bottom_z.is_sane()
            && self.kerf.is_none_or(pos)
            && nneg(self.lead_in)
            && nneg(self.lead_out)
            && pos(self.feed)
            && nneg(self.pierce_time)
            && nneg(self.abrasive_delay)
            && self.linearize.is_none_or(pos)
            && self.tabs.as_ref().is_none_or(|t| {
                t.count > 0
                    && t.count as usize == t.at.len()
                    && pos(t.width)
                    && t.at.iter().all(|p| p[0].is_finite() && p[1].is_finite())
            })
            && self.picks.refusal().is_none()
    }
}
