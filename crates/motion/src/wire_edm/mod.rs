//! Wire EDM 2-axis contour (#532, part of #290): one closed profile cut
//! through the workpiece by a vertical wire. The wire-centre path is
//! the profile's parallel at the kerf offset - the wire radius plus the
//! condition's spark gap per side - on the STATED side: inside the
//! profile for a die (the opening is the product, the slug falls out),
//! outside it for a punch (the profile is the product). The cut starts
//! and ends at a stated start hole / threading point, joined to the
//! path by a straight lead.
//!
//! Provenance: general wire EDM practice as the public literature the
//! umbrella cites states it (#290: the EDM review papers on wire
//! diameter, kerf and spark gap; the public design guides on start
//! holes and inside corners) - the wire-centre path is the profile
//! offset by wire radius plus gap, an enclosed cut is threaded through
//! a start hole, and no inside corner can be cut tighter than that
//! offset - read through the umbrella's decisions D1, D2 and D8. The
//! parallel curve is the textbook polygon offset
//! (`geo2d::Region::offset_sharp`, clipper2); the leads and the laws
//! are plain 2D geometry. Our own code; no vendor post, control manual
//! table or CAM product behaviour was consulted.
//!
//! The path is the wire-centre truth on the lower programmed plane.
//! Its moves state no pose (D2: a vertical cut posts as plain XY) and
//! no feed (the control's generator sets the rate from the condition).
//! Nothing here guesses: an open, crossing or area-less profile, a
//! missing or misplaced start hole, an inside corner or a neck the wire
//! cannot cut - each is refused by name, never trimmed or rounded.
//!
//! An OPEN run (#1109) - a cut-off line, a slot or a relief entered
//! from the stock's edge - is cut by `open`: the kerf on the stated
//! left or right of the run's travel, threaded outside the stock (no
//! start hole), see that module's head.
//!
//! Seams left for the family (built by none of this): the wire post
//! (#533) reads the feed moves between the threading point's visits;
//! verification (#534), taper as a pose per move (#536) and tabs
//! (#539) stand on this path; skims (#538, `skims.rs`) are further
//! passes of it, each at its own condition's offset; the no-core
//! burnout of a pocket (#540) is `burnout`; the 4-axis cut between two
//! profiles (#537) is `ruled`.

mod burnout;
mod laws;
mod pairing;
mod open;
mod ring;
mod ruled;
mod ruled_laws;
mod skims;
mod tabs;
mod taper;

pub use skims::{check_passes, wire_passes, WirePass};

pub use ruled::{wire_contour_ruled, WirePairing, WireProfile, WireRuled, WireSync, SYNC_TOL};
pub use tabs::{cutoff_split, TabParams, MAX_TABS};

pub use taper::{wire_contour_tapered, WireCorner, WireTaper};

pub use burnout::{clearance_door, wire_burnout, CORE_AREA_TOL, RING_STEP};
pub use open::{CLEARANCE_MAX, EDGE_TOL};

use crate::cl::{MoveKind, PathBuilder, Toolpath};
use crate::geo2d::{Loop, Point2, Region};
use crate::geometry::Point3;
use crate::machining2d::GenError;

/// How far (mm) past the kerf offset a point of the profile may stand
/// from the wire-centre path before the cover law calls it uncut: 2 µm.
/// It absorbs the facet turns of a tessellated round (a 15° turn at a
/// 0.2 mm offset reads 1.7 µm) and nothing a drawing would call a
/// corner: an inside radius 5 µm under the minimum is already refused.
pub const COVER_TOL: f64 = 0.002;
/// How far (mm) under the kerf offset the path may come to the profile
/// before the kerf law refuses it: 10 nm, ten steps of the offsetter's
/// lattice. The path is built to stand exactly at the offset.
pub const PATH_TOL: f64 = 1e-5;
/// The shortest lead (mm) that is a move at all.
const MIN_LEAD: f64 = 1e-6;

/// Which side of the profile is the product - stated, never inferred
/// (#290 D8).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WireSide {
    /// The opening is the product: the wire runs INSIDE the profile.
    Die,
    /// The profile is the product: the wire runs OUTSIDE it.
    Punch,
    /// An open run (#1109): the wire runs on the LEFT of the run's
    /// travel (its point order); the right side is kept.
    Left,
    /// An open run (#1109): the wire runs on the RIGHT of the run's
    /// travel; the left side is kept.
    Right,
}

impl WireSide {
    /// The side's word, as the notes and refusals say it.
    #[must_use]
    pub fn word(self) -> &'static str {
        match self {
            WireSide::Die => "die",
            WireSide::Punch => "punch",
            WireSide::Left => "left",
            WireSide::Right => "right",
        }
    }

    /// Whether the side is an open run's (left / right of travel) rather
    /// than a closed profile's (die / punch).
    #[must_use]
    pub fn is_open_side(self) -> bool {
        matches!(self, WireSide::Left | WireSide::Right)
    }
}

/// Everything the generator reads, resolved to plain numbers (mm).
#[derive(Clone, Debug, PartialEq)]
pub struct WireParams {
    /// The wire's diameter, from the machine's condition.
    pub wire_diameter: f64,
    /// The spark gap per side, from the machine's condition.
    pub gap: f64,
    /// The condition's id, as the control selects it - said in the
    /// path's notes, never interpreted here.
    pub condition: String,
    pub side: WireSide,
    /// The start hole / threading point; `None` is not stated.
    pub start: Option<Point2>,
    /// The lower programmed plane the moves ride.
    pub plane_z: f64,
    /// The tabs (#539); `None` is off - the default (#290 D6).
    pub tabs: Option<TabParams>,
    /// The stock's outline in XY - the edge an open run is entered
    /// from (#1109). A closed profile never reads it.
    pub stock: Option<Region>,
    /// How far (mm) clear of the stock's edge an open run's kerf is
    /// threaded and run out (#1109): the wire's centre stands at least
    /// the kerf offset plus this off the stock. A closed profile never
    /// reads it.
    pub clearance: f64,
}

fn invalid(why: String) -> GenError {
    GenError::Invalid(format!("wire contour: {why}"))
}

impl WireParams {
    /// How far the wire's centre stands from the profile: the wire
    /// radius plus the spark gap.
    #[must_use]
    pub fn kerf_offset(&self) -> f64 {
        self.wire_diameter * 0.5 + self.gap
    }

    /// The width of the slot the wire burns: its diameter plus the gap
    /// on both sides - twice the offset, the distance a die's path and
    /// a punch's path on one profile stand apart.
    #[must_use]
    pub fn kerf_width(&self) -> f64 {
        2.0 * self.kerf_offset()
    }

    /// The door: every refusal of the numbers by name, before any
    /// geometry is read.
    ///
    /// # Errors
    /// A wire diameter that is not positive, a gap below zero, or a
    /// non-finite plane or start point.
    pub fn door(&self) -> Result<(), GenError> {
        if !(self.wire_diameter.is_finite() && self.wire_diameter > 0.0) {
            return Err(invalid(format!(
                "the wire diameter must be a positive number (the condition states {})",
                self.wire_diameter
            )));
        }
        if !(self.gap.is_finite() && self.gap >= 0.0) {
            return Err(invalid(format!("the spark gap must be zero or more (the condition states {})", self.gap)));
        }
        if !self.kerf_offset().is_finite() {
            return Err(invalid("the kerf offset is not a finite number".into()));
        }
        if !self.plane_z.is_finite() {
            return Err(invalid("the programmed plane is not a finite height".into()));
        }
        if self.start.is_some_and(|s| !(s.x.is_finite() && s.y.is_finite())) {
            return Err(invalid("the start hole's position is not a finite point".into()));
        }
        Ok(())
    }
}

/// Cut the one closed profile in `loops` with the wire - or, when
/// `chains` holds an open run, that run from the stock's edge (#1109,
/// [`open`]).
///
/// # Errors
/// Every refusal the module head lists, by name; none is a panic.
pub fn wire_contour(loops: &[Loop], chains: &[Vec<Point2>], p: &WireParams) -> Result<Toolpath, GenError> {
    cut_pass(loops, chains, p, false)
}

/// One pass round the profile at `p`'s offset, every law judged at
/// that offset; `reverse` walks it clockwise (a skim's reversal, #538).
pub(super) fn cut_pass(loops: &[Loop], chains: &[Vec<Point2>], p: &WireParams, reverse: bool) -> Result<Toolpath, GenError> {
    p.door()?;
    if !chains.is_empty() {
        if p.tabs.is_some() {
            return Err(invalid(
                "tabs are stated on an open run - an open run closes no slug for a tab to hold; tabs hold a closed profile's slug (#539)".into(),
            ));
        }
        return open::wire_open(loops, chains, p);
    }
    let (given, profile) = ring::profile(loops, chains)?;
    if p.side.is_open_side() {
        return Err(invalid(format!(
            "the side is {} of travel, but the profile is closed - a closed profile states its die or punch side (left / right is an open run's kerf side)",
            p.side.word()
        )));
    }
    let k = p.kerf_offset();
    let path_ring = ring::wire_ring(&profile, p.side, k)?;
    // The corner minimum (#290 D8): every point of the profile must be
    // touched by the kerf - judged before the start hole, so a profile
    // the wire cannot cut is refused whatever else is stated.
    laws::cover(&given, &path_ring, p)?;
    let Some(start) = p.start else {
        return Err(invalid(format!(
            "no start hole is stated - a closed profile is cut from a threading point ({}); the wire cannot be threaded through solid material",
            match p.side {
                WireSide::Die => "a start hole in the slug, inside the profile",
                WireSide::Punch | WireSide::Left | WireSide::Right => "a start hole or the stock's edge, outside the profile",
            }
        )));
    };
    let (seg, foot) = ring::nearest(&path_ring, start);
    let lead = ring::dist(start, foot);
    if !(lead.is_finite() && lead > MIN_LEAD) {
        return Err(GenError::invalid_at(
            format!(
                "wire contour: the start hole at X{:.3} Y{:.3} stands on the wire path itself - move it off the profile by more than the kerf offset ({k:.4} mm)",
                start.x, start.y
            ),
            Point3::new(start.x, start.y, p.plane_z),
        ));
    }
    start_side(&profile, &path_ring, start, p)?;
    let z = p.plane_z;
    let mut b = PathBuilder::new(Point3::new(start.x, start.y, z));
    let tabbed = match &p.tabs {
        // A tabbed pass walks its tabs in the rough pass's direction;
        // a reversed one is refused by name, never cut half-tabbed.
        Some(_) if reverse => {
            return Err(invalid("a reversed pass is stated with tabs - tabs are cut on the rough pass's walk only (#538 x #539)".into()))
        }
        Some(t) => Some(tabs::tabbed(&path_ring, &given, start, p, t)?),
        None => None,
    };
    let pts: Vec<Point2> = match &tabbed {
        Some(tb) => tb.main.iter().chain(&tb.cutoff).copied().collect(),
        None => {
            let mut v = ring::walk_from(&path_ring, seg, foot);
            if reverse {
                v.reverse();
            }
            v.push(start);
            v
        }
    };
    for q in pts {
        b.push(MoveKind::Feed, Point3::new(q.x, q.y, z), None);
    }
    let mut path = b.finish();
    if path.moves.len() < 4 {
        return Err(invalid("the wire path lays down no cut".into()));
    }
    // The certificate: no move brings the kerf into the product.
    laws::kerf(&path, &given, true, k, p.plane_z)?;
    let mut said = pass_notes(p, start, lead, reverse);
    if let Some(tb) = tabbed {
        said.splice(1..2, tb.notes);
    }
    path.notes.extend(said);
    Ok(path)
}

/// The start hole must stand on the waste side, at least the kerf
/// offset from the profile - inside the wire path for a die, outside
/// it for a punch.
fn start_side(profile: &Region, path_ring: &[Point2], start: Point2, p: &WireParams) -> Result<(), GenError> {
    let k = p.kerf_offset();
    let inside_path = Region::new(vec![Loop::new(path_ring.to_vec())?])?.contains(start);
    let inside_profile = profile.contains(start);
    let why = match (p.side, inside_path, inside_profile) {
        (WireSide::Left | WireSide::Right, _, _) => {
            format!("the side is {} of travel - a closed profile states die or punch", p.side.word())
        }
        (WireSide::Die, true, _) | (WireSide::Punch, false, _) => return Ok(()),
        (WireSide::Die, false, true) | (WireSide::Punch, true, false) => format!(
            "the start hole at X{:.3} Y{:.3} stands closer to the profile than the kerf offset ({k:.4} mm) - the wire would burn the {} there; move it away from the profile",
            start.x,
            start.y,
            p.side.word()
        ),
        (WireSide::Die, false, false) => format!(
            "the start hole at X{:.3} Y{:.3} lies outside the profile - a die is cut from a start hole inside it, in the slug",
            start.x, start.y
        ),
        (WireSide::Punch, true, true) => format!(
            "the start hole at X{:.3} Y{:.3} lies inside the profile - a punch is cut from outside it; the wire would burn through the punch",
            start.x, start.y
        ),
    };
    Err(GenError::invalid_at(format!("wire contour: {why}"), Point3::new(start.x, start.y, p.plane_z)))
}

/// What the path says of itself: the side, the offset and its two
/// terms, the kerf, the threading point and the leads.
fn notes(p: &WireParams, start: Point2, lead: f64) -> Vec<String> {
    pass_notes(p, start, lead, false)
}

/// [`notes`] for a pass walked either way round (#538).
fn pass_notes(p: &WireParams, start: Point2, lead: f64, reverse: bool) -> Vec<String> {
    let cond = if p.condition.is_empty() { String::new() } else { format!(", condition {}", p.condition) };
    vec![
        format!(
            "wire EDM: {} side - the wire runs {} the profile at {:.4} mm (wire radius {:.4} + spark gap {:.4}{cond}); kerf width {:.4} mm",
            p.side.word(),
            match p.side {
                WireSide::Die => "inside",
                WireSide::Punch => "outside",
                WireSide::Left | WireSide::Right => "beside",
            },
            p.kerf_offset(),
            p.wire_diameter * 0.5,
            p.gap,
            p.kerf_width()
        ),
        format!(
            "wire EDM: threaded at X{:.3} Y{:.3}; straight lead-in of {lead:.3} mm to the path's nearest point, the profile cut {}, the lead-out back along the lead-in to the threading point",
            start.x,
            start.y,
            if reverse { "clockwise" } else { "counter-clockwise" }
        ),
        "wire EDM: a vertical cut - the moves state no pose (XY only) and no feed (the control's generator sets the rate); not verified here (no wire sweep against the part yet)".into(),
    ]
}

#[cfg(test)]
mod open_tests;
#[cfg(test)]
mod ruled_tests;
#[cfg(test)]
mod tests;
