//! The wire EDM contour's stored parameters (#532, part of #290): the
//! two planes of the workpiece, the machine condition the cut burns
//! with (by the control's own id), the stated punch / die side and the
//! start hole, and the skim passes after the rough cut (#538) - resolved into the motion crate's `wire_edm::WireParams`
//! at generation. The wire's diameter and the spark gap are NOT stored
//! here: the machine's condition table is their one source (#531).
//!
//! Provenance: our own parameter set over the umbrella's decisions
//! (#290 D2, D4, D8) and general wire EDM practice (see
//! `motion::wire_edm`); no vendor material consulted.

use super::height::Height;

pub use crate::wire_edm::WireSide;

/// The run-out clearance (mm) a new wire cut is seeded with (#1109): an
/// open run is threaded and run out this far clear of the stock's edge,
/// past the kerf. A closed profile never reads it.
pub const DEFAULT_CLEARANCE: f64 = 1.0;

/// The longest condition id an operation stores (a control's selector
/// is a short word; this only bounds a hand-edited file).
pub const CONDITION_ID_MAX: usize = 64;
/// The most skim passes an operation stores (#538): far past any
/// practice; it only bounds a hand-edited file.
pub const SKIMS_MAX: usize = 16;

/// One skim pass after the rough cut (#538): the machine condition it
/// burns with (its id; the gap and so the offset are that condition's)
/// and whether it walks the profile the other way round.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct WireSkim {
    /// The machine's EDM condition id; the empty word is "none chosen",
    /// refused at the command door by the skim's number.
    pub condition: String,
    /// Walk this pass clockwise (the rough pass runs counter-clockwise).
    pub reverse: bool,
}

fn sane_id(id: &str) -> bool {
    id.len() <= CONDITION_ID_MAX && !id.chars().any(char::is_control)
}

/// The wire contour's parameters.
#[derive(Clone, Debug, PartialEq)]
pub struct WireContourOp {
    /// The workpiece's top: the plane the profile is read on (the upper
    /// programmed plane a taper will pose toward, #536).
    pub top_z: Height,
    /// The workpiece's bottom: the lower programmed plane the moves
    /// ride (#290 D2).
    pub bottom_z: Height,
    /// The machine's EDM condition this cut burns with, by its id; the
    /// empty word is "none chosen", refused at the command door.
    pub condition: String,
    /// Which side of the profile is the product - stated by the user,
    /// never inferred from the geometry (#290 D8): die / punch for a
    /// closed profile, the kerf's left / right of travel for an open
    /// run (#1109).
    pub side: WireSide,
    /// The start hole / threading point, world XY mm; `None` is not
    /// stated, and a closed profile is refused at generation without it.
    pub start: Option<[f64; 2]>,
    /// The tabs (#539); `None` is off, the default (#290 D6).
    pub tabs: Option<WireTabs>,
    /// The operator states the freed piece is caught (a slug catcher, a
    /// magnet, the machine's slug retention), so it may drop when the
    /// cut closes. `false` - the default - refuses a cut whose slug
    /// would drop unheld (#534's slug law, #539).
    pub capture: bool,
    /// The constant taper (#536), degrees from vertical: 0 is a
    /// vertical cut; positive widens the profile downward from the top
    /// plane (a die's relief, a drafted punch's base), negative narrows
    /// it. The machine's `edm.max_taper_deg` bounds it at generation.
    pub taper_deg: f64,
    /// No-core burnout (#540): `Some(clearance)` burns the pocket's
    /// whole interior out by offset rings, the wire centre at least
    /// `clearance` mm from the walls (never under the kerf offset);
    /// `None` cuts the profile alone.
    pub burnout: Option<f64>,
    /// An open run's threading and run-out clearance past the kerf, mm
    /// (#1109); unread for a closed profile.
    pub clearance: f64,
    /// The skim passes after the rough cut, in cut order (#538); empty
    /// is the rough pass alone.
    pub skims: Vec<WireSkim>,
    /// The 4-axis cut (#537): `Some` reads the selection as two closed
    /// profiles - the upper on the top plane, the lower on the bottom
    /// plane - and pairs them into rulings by the stated rule. `None`
    /// is the one-profile cut.
    pub four_axis: Option<WireFourAxis>,
}

/// How the two profiles of a 4-axis cut are paired (#537) - stated,
/// never inferred.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WirePairingRule {
    /// By normalised arc length, each profile walked counter-clockwise
    /// from its point nearest the start hole.
    ArcLength,
    /// By the stated sync pairs, normalised arc length between them.
    SyncPoints,
}

/// One sync pair, world XY mm: a point of the upper profile and the
/// point of the lower profile the same ruling passes through. A side
/// left `None` is an unpaired sync point - storable (a pick in
/// progress), refused by name at generation.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WireSyncPair {
    pub upper: Option<[f64; 2]>,
    pub lower: Option<[f64; 2]>,
}

/// The most sync pairs an operation stores (bounds a hand-edited file).
pub const SYNC_PAIRS_MAX: usize = 4096;
/// The 4-axis tolerance a new cut starts with, mm.
pub const FOUR_AXIS_TOLERANCE: f64 = 0.05;

/// The 4-axis cut's stored parameters (#537).
#[derive(Clone, Debug, PartialEq)]
pub struct WireFourAxis {
    pub pairing: WirePairingRule,
    /// The sync pairs; read only when the pairing is by sync points.
    pub sync: Vec<WireSyncPair>,
    /// The most (mm) the wire may stand off the profiles past the kerf
    /// offset to keep its kerf out of a twisted ruled surface
    /// (`motion::wire_edm::ruled`).
    pub tolerance: f64,
}

impl Default for WireFourAxis {
    fn default() -> Self {
        WireFourAxis { pairing: WirePairingRule::ArcLength, sync: Vec::new(), tolerance: FOUR_AXIS_TOLERANCE }
    }
}

impl WireFourAxis {
    fn is_sane(&self) -> bool {
        let pt = |q: Option<[f64; 2]>| q.is_none_or(|s| s.iter().all(|v| v.is_finite() && v.abs() < 1e9));
        self.tolerance.is_finite()
            && (0.0..=10.0).contains(&self.tolerance)
            && self.sync.len() <= SYNC_PAIRS_MAX
            && self.sync.iter().all(|s| pt(s.upper) && pt(s.lower))
    }
}

/// The tabs as stated (#539). Every field is required when tabs are
/// on; a missing one is refused by name at generation, never defaulted.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct WireTabs {
    pub count: Option<u32>,
    /// The bridge's width along the profile, mm.
    pub width: Option<f64>,
    /// One position per tab, world XY mm, on the profile.
    pub at: Vec<[f64; 2]>,
    /// The machine condition the tab cut-off burns with, by its id; the
    /// empty word is none named.
    pub cutoff_condition: String,
}

impl WireTabs {
    #[must_use]
    pub fn is_sane(&self) -> bool {
        self.width.is_none_or(|w| w.is_finite() && w.abs() < 1e9)
            && self.at.len() <= 1024
            && self.at.iter().all(|p| p.iter().all(|v| v.is_finite() && v.abs() < 1e9))
            && self.cutoff_condition.len() <= CONDITION_ID_MAX
            && !self.cutoff_condition.chars().any(char::is_control)
    }
}

impl WireContourOp {
    /// Structural sanity (no world): the heights, a finite start point
    /// and a condition id that is one short word. Whether the machine
    /// states that condition is the command door's question
    /// (`operation::wire_op::condition_of`).
    #[must_use]
    pub fn is_sane(&self) -> bool {
        self.top_z.is_sane()
            && self.bottom_z.is_sane()
            && self.start.is_none_or(|s| s.iter().all(|v| v.is_finite() && v.abs() < 1e9))
            && self.taper_deg.is_finite()
            && self.taper_deg.abs() < 90.0
            && self.four_axis.as_ref().is_none_or(WireFourAxis::is_sane)
            && self.burnout.is_none_or(|c| c.is_finite() && c > 0.0 && c < 1e6)
            && self.clearance.is_finite()
            && (0.0..=crate::wire_edm::CLEARANCE_MAX).contains(&self.clearance)
            && sane_id(&self.condition)
            && self.skims.len() <= SKIMS_MAX
            && self.skims.iter().all(|s| sane_id(&s.condition))
            && self.tabs.as_ref().is_none_or(WireTabs::is_sane)
    }
}
