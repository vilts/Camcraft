//! V-carve (#1443): a V-bit cuts a closed region with the tip on the
//! medial axis. At each point the tip stands at the depth where the
//! cone touches the walls, so a narrow stroke is cut shallow, a wide one
//! deep, and a sharp corner rises to the surface. A maximum depth limits
//! the cut; where the region is wider than the cone reaches there, the
//! cone rides the walls and an optional pass clears the floor.
//!
//! Provenance: the medial axis transform and its simplification, from
//! the papers named in `vcarve_axis` (Blum 1967; Choi, Choi and Moon
//! 1997; Ma, Bae and Choi 2012; Foskey, Lin and Manocha 2003); the cone
//! law `depth = radius / tan(included angle / 2)` is machine-tool
//! handbook geometry; the floor rings are the offset pocketing of M.
//! Held, "On the Computational Geometry of Pocket Machining" (LNCS 500,
//! 1991). Written from those sources. No vendor material and no other
//! program's code or output was consulted.
//!
//! The safety law: at tip depth `d` the cone's circle in the top plane
//! has the radius `d * tan(angle / 2)`, and every lower circle of the
//! cone is smaller. Each tool position is the centre of a circle inside
//! the region, and the cone's circle is that circle less a wall
//! allowance. Each straight move is measured against the outline before
//! it is written; a move that fails is replaced by a retract. So the
//! cone never cuts outside the region (Prime Directive).

use crate::cl::{MoveKind, PathBuilder, Toolpath};
use crate::geo2d::{Point2, Region};
use crate::geometry::Point3;

use super::face::GenError;
use super::vcarve_axis::Outline;
use super::vcarve_walk::{link_ok, same, walk_loop, Keep, Pt, ALLOW};

/// The tip rapids to this height over the top plane, then feeds.
const PLUNGE_HEIGHT: f64 = crate::approach::DEFAULT_PLUNGE_HEIGHT;
/// The floor rings stand this much farther from the wall than the cone
/// pass: the offset's arcs are chords within 0.005 mm.
const FLOOR_MARGIN: f64 = 0.01;
/// The smallest floor stepover, mm.
const MIN_STEPOVER: f64 = 0.01;
/// The most floor rings one level may have.
const MAX_RINGS: usize = 20_000;

/// One V-carve's numbers.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct VCarveParams {
    /// The V-bit's included angle across the axis, degrees in (0, 180).
    pub angle_deg: f64,
    /// The cone's widest diameter: the cone cuts no wider than this.
    pub tool_diameter: f64,
    pub clearance_z: f64,
    /// Hop plane between strokes; `None` hops at clearance.
    pub retract_z: Option<f64>,
    /// The plane the carving starts in (the start depth).
    pub top_z: f64,
    /// The deepest plane the tip may reach: `top_z - bottom_z` is the
    /// maximum depth.
    pub bottom_z: f64,
    /// The most one level cuts under the level before it.
    pub stepdown: f64,
    pub feed: f64,
    pub plunge_feed: f64,
    /// Clear the floor the cone cannot reach at each level.
    pub flat_clear: bool,
    /// The distance between two floor rings, mm.
    pub flat_stepover: f64,
}

impl VCarveParams {
    pub(super) fn validate(&self) -> Result<(), GenError> {
        let mut finite = [
            self.angle_deg,
            self.tool_diameter,
            self.clearance_z,
            self.top_z,
            self.bottom_z,
            self.stepdown,
            self.feed,
            self.plunge_feed,
            self.flat_stepover,
        ]
        .into_iter()
        .all(f64::is_finite);
        finite &= self.retract_z.is_none_or(f64::is_finite);
        let bad = |msg: &str| Err(GenError::Invalid(msg.into()));
        if !finite {
            return bad("parameters must be finite");
        }
        if !(self.angle_deg > 0.0 && self.angle_deg < 180.0) {
            return bad("V-bit angle must be between 0 and 180 degrees");
        }
        if self.tool_diameter <= 0.0 {
            return bad("V-bit diameter must be positive");
        }
        if self.bottom_z >= self.top_z {
            return bad("maximum depth must be positive: the bottom plane must be under the top plane");
        }
        if self.stepdown <= 0.0 {
            return bad("stepdown must be positive");
        }
        if self.feed <= 0.0 || self.plunge_feed <= 0.0 {
            return bad("feeds must be positive");
        }
        if self.clearance_z < self.top_z {
            return bad("clearance plane below the carving's top plane");
        }
        if self.retract_z.is_some_and(|rz| rz < self.top_z) {
            return bad("retract plane sits inside the material");
        }
        if self.flat_clear && self.flat_stepover < MIN_STEPOVER {
            return bad("flat clearing stepover must be 0.01 mm or more");
        }
        Ok(())
    }

    /// `tan` of the cone's half angle: the cone's radius for each unit
    /// of height above the tip.
    #[must_use]
    pub fn tan_half(&self) -> f64 {
        (self.angle_deg.to_radians() * 0.5).tan()
    }

    /// The height of the cone: a deeper cut would bring the straight
    /// land above the cone into the wall.
    #[must_use]
    pub fn cone_height(&self) -> f64 {
        0.5 * self.tool_diameter / self.tan_half()
    }
}

/// Writes the strokes. It feeds from one position to the next where the
/// straight move keeps the cone inside the outline, and hops elsewhere.
struct Carver<'a> {
    b: PathBuilder,
    outline: &'a Outline,
    region: &'a Region,
    p: &'a VCarveParams,
    hop_z: f64,
    tan_half: f64,
    /// The position the tip stands at in the cut, if it is in the cut.
    down: Option<Pt>,
}

impl Carver<'_> {
    fn z(&self, pt: &Pt) -> f64 {
        (self.p.top_z - (pt.rho - ALLOW).max(0.0) / self.tan_half).max(self.p.bottom_z)
    }

    fn go(&mut self, pt: &Pt) {
        let to = Point3::new(pt.c.x, pt.c.y, self.z(pt));
        if let Some(from) = self.down {
            // The same place again: no move.
            if same(&from, pt) {
                return;
            }
            if link_ok(self.outline, self.region, &from, pt) {
                self.b.push(MoveKind::Feed, to, Some(self.p.feed));
                self.down = Some(*pt);
                return;
            }
            self.lift();
        }
        let at = self.b.at();
        self.b.push(MoveKind::Rapid, Point3::new(to.x, to.y, at.z), None);
        let over = (self.p.top_z + PLUNGE_HEIGHT).min(at.z).max(to.z);
        self.b.push(MoveKind::Rapid, Point3::new(to.x, to.y, over), None);
        self.b.push(MoveKind::Plunge, to, Some(self.p.plunge_feed));
        self.down = Some(*pt);
    }

    fn lift(&mut self) {
        if self.down.take().is_some() {
            let at = self.b.at();
            self.b.push(MoveKind::Rapid, Point3::new(at.x, at.y, self.hop_z), None);
        }
    }

    /// One closed ring of positions: enter at its shallowest point.
    fn ring(&mut self, ring: &[Pt]) {
        let Some(start) = (0..ring.len()).min_by(|&i, &j| ring[i].rho.total_cmp(&ring[j].rho)) else {
            return;
        };
        for k in 0..=ring.len() {
            self.go(&ring[(start + k) % ring.len()]);
        }
    }

    /// A walked loop with dropped stretches: each kept run in order,
    /// starting after a dropped stretch.
    fn runs(&mut self, walk: &[Option<Pt>]) {
        let Some(gap) = walk.iter().position(Option::is_none) else {
            let ring: Vec<Pt> = walk.iter().flatten().copied().collect();
            self.ring(&ring);
            return;
        };
        for k in 1..=walk.len() {
            if let Some(pt) = &walk[(gap + k) % walk.len()] {
                self.go(pt);
            }
        }
    }
}

/// V-carve `region`: one walk of each loop for each level, then that
/// level's floor rings where flat clearing is on.
pub fn v_carve(region: &Region, p: &VCarveParams) -> Result<Toolpath, GenError> {
    p.validate()?;
    if region.is_empty() {
        return Err(GenError::Empty("empty V-carve region".into()));
    }
    let outline = Outline::new(region)?;
    let tan_half = p.tan_half();
    let cone_h = p.cone_height();
    if !(tan_half.is_finite() && tan_half > 0.0 && cone_h.is_finite() && cone_h > 0.0) {
        return Err(GenError::Invalid("V-bit angle leaves no cone".into()));
    }
    let asked = p.top_z - p.bottom_z;
    let depth = asked.min(cone_h);
    let zs = super::levels::cut_levels(p.top_z, p.top_z - depth, p.stepdown)?;

    let hop_z = p.retract_z.unwrap_or(p.clearance_z).min(p.clearance_z);
    let mut b = PathBuilder::approached(Point3::new(0.0, 0.0, p.clearance_z));
    if depth < asked {
        b.note(format!(
            "V-carve: the maximum depth is limited to the cone height {depth:.3} (the V-bit is {} wide at {} degrees)",
            p.tool_diameter, p.angle_deg
        ));
    }
    let mut carver = Carver { b, outline: &outline, region, p, hop_z, tan_half, down: None };
    let mut done_r = -1.0;
    for z in zs {
        let cap_r = (p.top_z - z) * tan_half + ALLOW;
        let keep = Keep { cap_r, done_r };
        for pieces in &outline.loops {
            let walk = walk_loop(&outline, pieces, &keep)?;
            carver.runs(&walk);
        }
        if p.flat_clear {
            floor(&mut carver, cap_r)?;
        }
        carver.lift();
        done_r = cap_r;
    }
    carver.lift();
    let mut b = carver.b;
    let cur = b.at();
    b.push(MoveKind::Rapid, Point3::new(cur.x, cur.y, p.clearance_z), None);

    let path = b.finish();
    if !path.all_finite() {
        return Err(GenError::Invalid("path left the finite horizon".into()));
    }
    if !path.moves.iter().any(|m| m.kind != MoveKind::Rapid && m.to.z < p.top_z) {
        return Err(GenError::Empty(
            "the V-carve region is too narrow to cut: no place is wider than the wall allowance".into(),
        ));
    }
    Ok(path)
}

/// The floor of one level: rings from the wall pass inward, one
/// stepover apart, with the tip at the level's depth.
fn floor(carver: &mut Carver<'_>, cap_r: f64) -> Result<(), GenError> {
    let step = carver.p.flat_stepover;
    for j in 1..=MAX_RINGS {
        let inset = cap_r + FLOOR_MARGIN + step * j as f64;
        let rings = carver.region.offset(-inset)?;
        if rings.is_empty() {
            return Ok(());
        }
        for l in rings.loops() {
            // Start each ring at the point nearest to the tip.
            let pts = l.points();
            let at = carver.b.at();
            let near = |q: &Point2| (q.x - at.x).hypot(q.y - at.y);
            let Some(start) = (0..pts.len()).min_by(|&i, &k| near(&pts[i]).total_cmp(&near(&pts[k]))) else {
                continue;
            };
            for k in 0..=pts.len() {
                let c = pts[(start + k) % pts.len()];
                // A ring point the outline does not clear is not cut.
                if carver.outline.dist(c) < cap_r - 0.5 * ALLOW || !carver.region.contains(c) {
                    carver.lift();
                    continue;
                }
                carver.go(&Pt { c, rho: cap_r });
            }
        }
    }
    Err(GenError::Invalid("flat clearing needs more than 20000 rings; use a larger stepover".into()))
}
