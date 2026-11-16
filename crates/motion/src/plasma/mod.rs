//! Plasma profile cutting (#670): 2D through-cuts of closed and open
//! contours with a torch - the kerf offset, the cut direction, arc
//! leads, the small-hole rule and inner-before-outer cut order.
//!
//! Provenance (public sources, behaviour only, our own words and code):
//! - Hypertherm's published cut-quality guidance: outer contours run
//!   clockwise and holes counter-clockwise, so the arc's square edge
//!   falls on the part and the bevelled edge on the scrap.
//! - LinuxCNC Plasma Primer §13 (small holes): below roughly 32 mm or
//!   1.5x the plate thickness, lead in perpendicular from near the
//!   centre, leave out the lead-out (overburn instead) and cut at
//!   60-70 % of the contour feed.
//! - Tormach's public 1300PL Operator's Manual (UM10720) and PathPilot
//!   Post Processor Guidelines: the controller owns the torch's Z (probe,
//!   pierce height, cut height, delay - AutoFS when the program leaves
//!   them blank); G41/G42 is accepted with a straight G1 entry longer
//!   than the tool radius; kerf is about 1.5x the nozzle orifice.
//! - The kerf offset is the textbook offset curve (clipper2's polygon
//!   buffer, `geo2d::Region::offset`), the leads plain circle geometry.
//!
//! The path is the torch-centre truth: every lit move stands kerf / 2
//! off the part on the waste side (or on the centre line of an open
//! run), and the [`PlasmaCut`] record states the kerf, the torch on /
//! off per contour, each contour's role and the contour it lies in.
//! Nothing here guesses: a zero-length or self-crossing contour, a hole
//! the kerf cannot enter, a lead with no room, a lit move whose kerf
//! would reach the part, an edge the kerf never reaches (a slot or
//! notch narrower than the kerf, bridged by the offset - `laws`, #672)
//! - each is refused by name, never trimmed.

mod emit;
mod laws;
mod nest;
mod ring;

use crate::cl::{PlasmaCut, Toolpath};
use crate::geo2d::{Loop, Point2, Region};
use crate::machining2d::GenError;

pub use nest::Contour;

/// The small-hole threshold when the op states none: 32 mm (the
/// Primer's lower bound of the band where hole quality falls off).
pub const SMALL_HOLE_DIA: f64 = 32.0;
/// The small-hole feed factor when the op states none: 65 %, the middle
/// of the Primer's 60-70 % band.
pub const SMALL_HOLE_FEED: f64 = 0.65;
/// The small-hole rule's thickness word: a hole under this many plate
/// thicknesses across is small whatever the threshold says.
pub const SMALL_HOLE_THICKNESS: f64 = 1.5;
/// How close (mm) a lit move's kerf may come to the part before the
/// law refuses it: the offset's round-join flattening (5 µm) plus the
/// arc fit (5 µm) plus slack, far under any plasma cut tolerance.
pub const KERF_TOL: f64 = 0.025;

/// Everything the generator reads, resolved to plain numbers (mm,
/// mm/min, seconds). `None` on a feed or a height means the controller
/// decides (AutoFS): the CL carries no number for it.
#[derive(Clone, Debug, PartialEq)]
pub struct PlasmaParams {
    /// The kerf width (the torch tool's diameter, or the op's override).
    pub kerf: f64,
    /// Post G41/G42 (the control offsets the part contour) instead of
    /// the offset path the computer walked.
    pub control_comp: bool,
    /// Run outer contours counter-clockwise and holes clockwise - the
    /// override of the Hypertherm default, stated in the path's notes.
    pub reversed: bool,
    /// Arc lead-in / lead-out length along the arc, mm (0 = none).
    /// Under control compensation the leads are straight, this long.
    pub lead_in: f64,
    pub lead_out: f64,
    /// The small-hole threshold diameter, mm.
    pub small_hole_dia: f64,
    /// The small-hole feed factor, (0, 1].
    pub small_hole_feed: f64,
    /// How far a small hole's cut runs on past its start, mm (0 = none).
    pub overburn: f64,
    /// Plate thickness, mm (the small-hole rule's 1.5x word).
    pub thickness: f64,
    pub feed: Option<f64>,
    pub clearance_z: f64,
    /// The plane the lit moves ride (the plate top plus the stated cut
    /// height, if any).
    pub cut_z: f64,
    pub pierce_height: Option<f64>,
    pub cut_height: Option<f64>,
    pub pierce_delay: Option<f64>,
    /// How much coarser than the importer's promise the model the part
    /// silhouette was read from is, mm (#1053, the #864 reading): the
    /// kerf and cover laws give it back out of [`KERF_TOL`] while a
    /// positive tolerance is left ([`PlasmaParams::kerf_tol`]). `0.0`
    /// for a model that met the promise, and for every plate drawn by
    /// hand - each law then reads exactly as it always did.
    pub model_excess: f64,
}

impl PlasmaParams {
    /// The kerf and cover laws' tolerance on this model (#1053):
    /// [`KERF_TOL`] less [`Self::model_excess`] while that stays
    /// positive, untouched past that - where the program gate refuses
    /// the operation by name instead (`verify::excess_floor` is the one
    /// test both read). A facet of a curved edge stands inside the true
    /// edge by up to its chord error, so the kerf that clears the
    /// facets by the plain tolerance can reach the real part by the
    /// excess more than it could at full fidelity.
    #[must_use]
    pub fn kerf_tol(&self) -> f64 {
        crate::verify::excess_floor(KERF_TOL, self.model_excess).unwrap_or(KERF_TOL)
    }

    /// The door: every refusal of the numbers by name, before any
    /// geometry is read.
    pub fn door(&self) -> Result<(), GenError> {
        let bad = |w: String| Err(GenError::Invalid(format!("plasma profile: {w}")));
        if !(self.kerf.is_finite() && self.kerf > 0.0) {
            return bad(format!("the kerf width must be a positive number (it is {})", self.kerf));
        }
        for (name, v) in [
            ("lead-in", self.lead_in),
            ("lead-out", self.lead_out),
            ("overburn", self.overburn),
            ("plate thickness", self.thickness),
            ("small-hole diameter", self.small_hole_dia),
        ] {
            if !(v.is_finite() && v >= 0.0) {
                return bad(format!("the {name} must be zero or more (it is {v})"));
            }
        }
        if !(self.small_hole_feed.is_finite() && self.small_hole_feed > 0.0 && self.small_hole_feed <= 1.0) {
            return bad(format!(
                "the small-hole feed factor must lie in (0, 1] (it is {})",
                self.small_hole_feed
            ));
        }
        if self.feed.is_some_and(|f| !(f.is_finite() && f > 0.0)) {
            return bad("a stated feed must be positive - leave it blank for the controller's".into());
        }
        for (name, v) in [
            ("pierce height", self.pierce_height),
            ("cut height", self.cut_height),
            ("pierce delay", self.pierce_delay),
        ] {
            if v.is_some_and(|x| !(x.is_finite() && x >= 0.0)) {
                return bad(format!("a stated {name} must be zero or more - leave it blank for the controller's"));
            }
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
        Ok(())
    }
}

/// Cut `loops` (closed contours) and `chains` (open runs, cut on their
/// centre line) with the torch. `part` is the material the kerf must
/// never reach - the model's through-silhouette; an empty region skips
/// that law and the classification falls back to nesting parity (both
/// said in the notes).
pub fn plasma_profile(
    loops: &[Loop],
    chains: &[Vec<Point2>],
    part: &Region,
    p: &PlasmaParams,
) -> Result<Toolpath, GenError> {
    p.door()?;
    if loops.is_empty() && chains.is_empty() {
        return Err(GenError::Empty("plasma profile: no contour to cut".into()));
    }
    let contours = nest::contours(loops, chains, part)?;
    let order = nest::cut_order(&contours);
    emit::cut(&contours, &order, part, p)
}

/// The record a generated path carries (shared by the emitter and the
/// tests that read it back).
#[must_use]
pub fn header(p: &PlasmaParams) -> PlasmaCut {
    PlasmaCut {
        kerf: p.kerf,
        control_comp: p.control_comp,
        pierce_height: p.pierce_height,
        cut_height: p.cut_height,
        pierce_delay: p.pierce_delay,
        auto_feed: p.feed.is_none(),
        reversed: p.reversed,
        torches: Vec::new(),
    }
}

#[cfg(test)]
mod tests;
