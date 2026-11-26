//! Waterjet profile cutting (#1883): 2D through-cuts of closed and open
//! contours with an abrasive jet - the kerf offset, inner contours
//! before the contour that frees them, straight or arc leads with the
//! pierce in the waste, tabs left as uncut gaps, and the tight-hole
//! rule. No torch height or THC: the jet has none.
//!
//! Provenance (public sources, behaviour only, our own words and code):
//! - General abrasive-waterjet shop practice as the public machine
//!   manuals and the umbrella #1525 state it: the kerf is about the
//!   mixing tube's bore, the cut path stands kerf / 2 off the part on
//!   the waste side, a pierce is made in the scrap and led in, the
//!   holes are cut before the outline that frees the part, and tabs
//!   (micro-joints) are short uncut gaps that hold a part in the sheet.
//! - RS-274 / ISO 6983 for G41/G42 and for linear-only controls (arcs
//!   sent as chords).
//! - The geometry is the shared sheet kernel ([`crate::sheet`]).
//!
//! The path is the jet-centre truth. Nothing here guesses: a contour
//! the kerf cannot enter, a lead with no room, a tab that lies on no
//! contour, two tabs that overlap, a lit move whose kerf would reach
//! the part, an edge the kerf never reaches - each is refused by name.

mod emit;
mod plan;
mod tabs;

use crate::cl::Toolpath;
use crate::geo2d::{Loop, Point2, Region};
use crate::machining2d::GenError;
use crate::sheet::{nest, Voice};

/// How close (mm) a lit move's kerf may come to the part before the
/// law refuses it: the offset's round-join flattening and the arc fit,
/// with slack - the same figure the plasma laws hold.
pub const KERF_TOL: f64 = 0.025;
/// The most tabs one operation states.
pub const MAX_TABS: u32 = 256;

/// Which side of an open run's travel the kerf stands on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OpenSide {
    /// On the run's centre line (a parting cut).
    Center,
    Left,
    Right,
}

/// The shape of a lead.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LeadShape {
    Straight,
    Arc,
}

/// Tabs left as uncut gaps: how many, how wide (mm, along the part
/// edge), and one position per tab near a closed contour (world XY).
#[derive(Clone, Debug, PartialEq)]
pub struct JetTabs {
    pub count: u32,
    pub width: f64,
    pub at: Vec<Point2>,
}

/// Everything the generator reads, resolved to plain numbers (mm,
/// mm/min, seconds).
#[derive(Clone, Debug, PartialEq)]
pub struct WaterjetParams {
    /// The kerf width (the nozzle tool's diameter, or the op's override).
    pub kerf: f64,
    /// Post G41/G42 (the control offsets the part contour) instead of
    /// the offset path the computer walked.
    pub control_comp: bool,
    /// Run outer contours counter-clockwise and holes clockwise.
    pub reversed: bool,
    pub lead_shape: LeadShape,
    /// Lead-in / lead-out length, mm (0 = none).
    pub lead_in: f64,
    pub lead_out: f64,
    /// The kerf side of every open run.
    pub open_side: OpenSide,
    pub tabs: Option<JetTabs>,
    /// Seconds the jet dwells to pierce, and the abrasive's delay
    /// before it; stated in the path's notes (the CL record that
    /// carries them to the post is #1882's).
    pub pierce_time: f64,
    pub abrasive_delay: f64,
    /// Send every arc as chords at this tolerance (mm), for a control
    /// with no G2/G3; `None` keeps the arcs.
    pub linearize: Option<f64>,
    pub feed: f64,
    pub clearance_z: f64,
    /// The plane the jet's moves ride (the sheet top).
    pub cut_z: f64,
    /// How much coarser than the importer's promise the model is, mm
    /// (#1053): the laws give it back out of [`KERF_TOL`].
    pub model_excess: f64,
}

impl WaterjetParams {
    /// The kerf and cover laws' tolerance on this model (#1053).
    #[must_use]
    pub fn kerf_tol(&self) -> f64 {
        crate::verify::excess_floor(KERF_TOL, self.model_excess).unwrap_or(KERF_TOL)
    }

    /// The door: every refusal of the numbers by name, before any
    /// geometry is read.
    pub fn door(&self) -> Result<(), GenError> {
        let bad = |w: String| Err(Voice::WATERJET.invalid(w));
        if !(self.kerf.is_finite() && self.kerf > 0.0) {
            return bad(format!("the kerf width must be a positive number (it is {})", self.kerf));
        }
        if !(self.feed.is_finite() && self.feed > 0.0) {
            return bad(format!("the cut feed must be a positive number (it is {})", self.feed));
        }
        for (name, v) in [
            ("lead-in", self.lead_in),
            ("lead-out", self.lead_out),
            ("pierce time", self.pierce_time),
            ("abrasive delay", self.abrasive_delay),
        ] {
            if !(v.is_finite() && v >= 0.0) {
                return bad(format!("the {name} must be zero or more (it is {v})"));
            }
        }
        if let Some(t) = self.linearize
            && !(t.is_finite() && t > 0.0)
        {
            return bad(format!("the arc linearization tolerance must be a positive number (it is {t})"));
        }
        if !(self.clearance_z.is_finite() && self.cut_z.is_finite() && self.clearance_z > self.cut_z) {
            return bad(format!(
                "the clearance plane ({}) must stand above the cut plane ({})",
                self.clearance_z, self.cut_z
            ));
        }
        if self.control_comp {
            let r = self.kerf * 0.5;
            for (end, len) in [("in", self.lead_in), ("out", self.lead_out)] {
                if len + 1e-9 < r {
                    return bad(format!(
                        "control compensation (G41/G42) needs a straight lead-{end} of at least the kerf radius ({r:.3} mm) - the control builds the offset over it; the lead-{end} is {len:.3} mm"
                    ));
                }
            }
        }
        if let Some(t) = &self.tabs {
            tabs::door(t)?;
        }
        Ok(())
    }
}

/// Cut `loops` (closed contours) and `chains` (open runs) with the jet.
/// `part` is the material the kerf must never reach - the model's
/// through-silhouette; an empty region skips that law and the roles
/// fall back to nesting parity (both said in the notes).
pub fn waterjet_profile(loops: &[Loop], chains: &[Vec<Point2>], part: &Region, p: &WaterjetParams) -> Result<Toolpath, GenError> {
    p.door()?;
    if loops.is_empty() && chains.is_empty() {
        return Err(GenError::Empty("waterjet profile: no contour to cut".into()));
    }
    let contours = nest::contours(loops, chains, part, Voice::WATERJET)?;
    let order = nest::cut_order(&contours);
    emit::cut(&contours, &order, part, p)
}

#[cfg(test)]
mod tests;
