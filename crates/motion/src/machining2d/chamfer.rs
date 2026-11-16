//! 2D chamfer: a cone-ruled ride along the picked edge (M2.4 B).
//!
//! Provenance: the chamfer mill's cut law is machine-tool handbook
//! geometry - a conical cutter whose tangent generatrix lies ON the
//! chamfer face. The face joins the point `width` back from the wall
//! (in the top plane) to the point `width * tan(angle)` down the wall;
//! a cone of half-angle from the axis `90 - angle` degrees, riding the
//! wall with its virtual tip at the corner, has exactly that surface.
//! The included angle across such a tool's cone is `180 - 2 * angle`
//! (a 90-degree chamfer mill for a 45-degree chamfer, the shop's own
//! catalog law). Written clean-room from that geometry, no vendor
//! material; the CL convention (moves carry TIP positions) is this
//! codebase's own, so the posted Z is the corner depth.
//!
//! The path is one closed circuit per picked loop at one constant Z -
//! `top_z - width * tan(angle_deg)`, the derived floor the operation
//! states instead of carrying - entered by the axial dive trace and
//! the slot post (a chamfer ride has no bounded air beside the start
//! the entry arms could certify against: the wall is the start).
//! The congruence doors (the tool's cone, its reach) stand in the
//! app's dispatch where the Tool is visible; here the law is stated
//! on the face alone. Prime Directive: refused before cut, never
//! approximated.

use crate::cl::{EdgeBreak, MoveKind, PathBuilder, Toolpath};
use crate::geo2d::{Loop, Point2};
use crate::geometry::Point3;

use super::face::GenError;
use super::levels::{Motion, run_levels};

/// One chamfer ride's numbers. `top_z` is the plane the chamfer's wide
/// end opens in (the picked edge's top); the corner depth follows from
/// it and is never an independent input - that is what makes the
/// operation's floor a law, not a second promise.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ChamferParams {
    /// Horizontal extent of the chamfer face in the top plane.
    pub width: f64,
    /// The face's angle from that plane, degrees in (0, 90).
    pub angle_deg: f64,
    pub clearance_z: f64,
    /// Hop plane; validated at or above `top_z`, never the corner -
    /// between the corner and the top the wall still stands.
    pub retract_z: Option<f64>,
    pub top_z: f64,
    pub feed: f64,
    pub plunge_feed: f64,
    /// Climb milling under M3 (#1555): the material stays on the RIGHT
    /// of travel - a boss edge rides clockwise, a bore edge
    /// counter-clockwise; conventional reverses each loop.
    pub climb: bool,
    /// The picked region is a bore's air (Side Inside), not the part's
    /// material: which side of each stored loop the material stands on.
    pub inside: bool,
}

impl ChamferParams {
    pub(super) fn validate(&self) -> Result<(), GenError> {
        let mut finite = [
            self.width,
            self.angle_deg,
            self.clearance_z,
            self.top_z,
            self.feed,
            self.plunge_feed,
        ]
        .into_iter()
        .all(f64::is_finite);
        finite &= self.retract_z.is_none_or(f64::is_finite);
        if !finite {
            return Err(GenError::Invalid("parameters must be finite".into()));
        }
        if self.width <= 0.0 {
            return Err(GenError::Invalid("chamfer width must be positive".into()));
        }
        if !(0.0..90.0).contains(&self.angle_deg) {
            return Err(GenError::Invalid(
                "chamfer angle must be between 0 and 90 degrees".into(),
            ));
        }
        if self.feed <= 0.0 || self.plunge_feed <= 0.0 {
            return Err(GenError::Invalid("feeds must be positive".into()));
        }
        if self.clearance_z < self.top_z {
            return Err(GenError::Invalid("clearance plane below the chamfer's top plane".into()));
        }
        if self.retract_z.is_some_and(|rz| rz < self.top_z) {
            return Err(GenError::Invalid("retract plane sits inside the material".into()));
        }
        Ok(())
    }

    /// The corner depth the ride posts - the derived floor, stated in
    /// one place so the panel, the door and the path cannot disagree.
    #[must_use]
    pub fn corner_z(&self) -> f64 {
        self.top_z - self.width * self.angle_deg.to_radians().tan()
    }
}

/// Chamfer every loop of the picked region: one circuit per loop, in
/// `|area|` order for determinism (trace's ordering law), at the
/// corner depth. The centers ride the loops exactly - the tip is on
/// the wall, the generatrix on the face - so no offset exists to get
/// wrong, and a picked edge that is not the chamfer's own wall is
/// caught by the stock simulation, not papered over here.
pub fn chamfer(region: &crate::geo2d::Region, p: &ChamferParams) -> Result<Toolpath, GenError> {
    p.validate()?;
    if region.is_empty() {
        return Err(GenError::Empty("empty chamfer region".into()));
    }
    let mut loops: Vec<&Loop> = region.loops().iter().collect();
    loops.sort_by(|x, y| y.area().abs().total_cmp(&x.area().abs()));
    let z = p.corner_z();
    if !z.is_finite() {
        return Err(GenError::Invalid("corner depth left the finite horizon".into()));
    }
    let mut passes: Vec<Vec<Point2>> = Vec::with_capacity(loops.len());
    for l in &loops {
        // A stored loop keeps its region on the left: the material for a
        // boss edge (climb reverses it), the bore's air for an Inside
        // pick (climb keeps it) - material right of travel either way.
        let mut pass: Vec<Point2> = super::ring::walked(l.points(), p.climb == p.inside, false);
        if let Some(&start) = pass.first() {
            pass.push(start);
        }
        passes.push(pass);
    }

    let mut b = PathBuilder::approached(Point3::new(0.0, 0.0, p.clearance_z));
    // The one level the ride owns: not a ladder - the cone's
    // generatrix meets the wall at exactly one depth.
    let motion = Motion::new(p.clearance_z, p.retract_z, p.feed, p.plunge_feed);
    run_levels(&mut b, &passes, &[z], &motion, None, None, None);
    let cur = b.at();
    b.push(MoveKind::Rapid, Point3::new(cur.x, cur.y, p.clearance_z), None);

    let mut path = b.finish();
    if !path.all_finite() {
        return Err(GenError::Invalid("path left the finite horizon".into()));
    }
    // The break this ride was asked to cut (#303): each picked loop at
    // the top plane, the face's depth and slope from the two authored
    // numbers - never read back off the moves, so the sim judges the
    // ride against the promise.
    let slope = p.angle_deg.to_radians().tan();
    path.breaks = passes
        .iter()
        .map(|pass| EdgeBreak {
            edge: pass.iter().map(|q| Point3::new(q.x, q.y, p.top_z)).collect(),
            depth: p.top_z - z,
            slope,
        })
        .filter(EdgeBreak::is_sane)
        .collect();
    Ok(path)
}
