//! Standalone boring: circular interpolation to a finished hole wall
//! (M2.4 C).
//!
//! Provenance: back-boring and single-diameter boring by helical/
//! circular interpolation - a cutter smaller than the finished hole
//! riding a circle whose rim is the wall - is standard milling practice
//! described in machine-tool handbooks (it is what a machining center
//! does when the bore is wider than any bar it carries). Written
//! clean-room from that surface definition; the slide cycle M1.6 kept
//! in `drilling` is a different strategy and stays what it was.
//!
//! The law in one line: the cutter center rides `rc = hole_radius -
//! tool_radius - leave`, so the disc's rim stands on the finished wall,
//! and one radial sweep followed by one closed ring clears the annulus
//! of each level - the disk's waterline reduced to the one ring that
//! reaches the wall (`circular`'s ring walk with `stepover` past `rc`).
//! The entry family certifies against the cutter-center disk exactly
//! where the material opens: a counterbore-shaped step (`stepped`)
//! would promise the fold's hidden ring to nobody, so it is declined
//! for this strategy by name - the word M1.6's bore cycle already
//! speaks. A through hole's last level drops the exit allowance under
//! the floor, the same parting gift the cycles pay.
//!
//! Prime Directive: the wall is a circle only the model's own recognition
//! draws (`holes` arrive as params data, like the drill's); a tool too
//! wide to enter the mouth, a hole with no floor under its entry, and
//! every non-finite number are refused before any move is built.

use std::f64::consts::PI;

use crate::cl::{MoveKind, PathBuilder, Toolpath};
use crate::entry::Entry3D;
use crate::geo2d::{Loop, Point2, Region};
use crate::geometry::Point3;
use crate::links::Links;

use super::face::GenError;
use super::levels::{Cert, Motion, cut_levels, run_levels};

/// One finished round hole the ride is paid to. `xy`/`radius` are the
/// model's wall; `floor_z` is where the rim's bottom edge stops.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BoreHole {
    pub xy: Point2,
    pub radius: f64,
    /// The material's entry surface at this hole.
    pub top_z: f64,
    /// The hole's floor.
    pub floor_z: f64,
    pub through: bool,
    /// The recognition folded a counterbore into this hole; the hidden
    /// ring is another operation's promise (declined below).
    pub stepped: bool,
}

impl BoreHole {
    fn all_finite(&self) -> bool {
        self.xy.x.is_finite()
            && self.xy.y.is_finite()
            && self.radius.is_finite()
            && self.top_z.is_finite()
            && self.floor_z.is_finite()
    }
}

/// The boring ride's numbers. One ladder per hole, holes hopped at
/// `retract_z` (or clearance) the way the pocket family hops islands.
#[derive(Clone, Debug, PartialEq)]
pub struct BoreParams {
    pub holes: Vec<BoreHole>,
    pub tool_diameter: f64,
    /// Wall allowance: the rim stands `leave` short of the model's
    /// circle. `0.0` finishes to nominal.
    pub stock_to_leave: f64,
    /// Axial bite per level.
    pub stepdown: f64,
    /// Below-floor overtravel on through holes, the cycles' exit law.
    pub through_allowance: f64,
    pub clearance_z: f64,
    pub retract_z: Option<f64>,
    pub feed: f64,
    pub plunge_feed: f64,
    /// CCW (climb on the wall) when true.
    pub climb: bool,
    /// Sagitta budget converting the ride to arcs at post time.
    pub tolerance: f64,
    pub entry: Entry3D,
    pub helix_radius: f64,
    /// Level transitions and links (M11.2 C/D) within each hole's
    /// ladder; [`Links::PLANAR`] is every earlier bore.
    pub links: Links,
}

impl BoreParams {
    pub(super) fn validate(&self) -> Result<(), GenError> {
        let mut finite = [
            self.tool_diameter,
            self.stock_to_leave,
            self.stepdown,
            self.through_allowance,
            self.clearance_z,
            self.feed,
            self.plunge_feed,
            self.tolerance,
            self.helix_radius,
        ]
        .into_iter()
        .all(f64::is_finite);
        finite &= self.retract_z.is_none_or(f64::is_finite);
        finite &= self.holes.iter().all(BoreHole::all_finite);
        if !finite {
            return Err(GenError::Invalid("parameters must be finite".into()));
        }
        if self.holes.is_empty() {
            return Err(GenError::Invalid("boring needs at least one hole".into()));
        }
        if self.tool_diameter <= 0.0 {
            return Err(GenError::Invalid("tool diameter must be positive".into()));
        }
        if self.stock_to_leave < 0.0 || self.stepdown <= 0.0 || self.through_allowance < 0.0 {
            return Err(GenError::Invalid(
                "leave and through allowance must be non-negative, stepdown positive".into(),
            ));
        }
        if self.feed <= 0.0 || self.plunge_feed <= 0.0 || self.tolerance <= 0.0 {
            return Err(GenError::Invalid("feeds and tolerance must be positive".into()));
        }
        if self.clearance_z
            < self
                .holes
                .iter()
                .map(|h| h.top_z)
                .fold(f64::NEG_INFINITY, f64::max)
        {
            return Err(GenError::Invalid("clearance plane below a hole entry".into()));
        }
        // The hop must clear the tops the ride crosses between holes.
        let top = self
            .holes
            .iter()
            .map(|h| h.top_z)
            .fold(f64::NEG_INFINITY, f64::max);
        if self.retract_z.is_some_and(|rz| rz < top) {
            return Err(GenError::Invalid("retract plane sits inside the material".into()));
        }
        self.links.validate().map_err(|e| GenError::Invalid(e.into()))?;
        for h in &self.holes {
            if self.links.transition == Some(Entry3D::Helix) && self.helix_radius <= 0.0 {
                return Err(GenError::Invalid("helix radius must be positive".into()));
            }
            if h.radius <= 0.0 {
                return Err(GenError::Invalid("hole radius must be positive".into()));
            }
            if h.floor_z >= h.top_z {
                return Err(GenError::Invalid(
                    "a bored hole must have its floor below its entry".into(),
                ));
            }
            // #261 part 2: a refusal about this hole points at its mouth.
            let mouth = Point3::new(h.xy.x, h.xy.y, h.top_z);
            if h.stepped {
                return Err(GenError::invalid_at(
                    format!(
                        "hole at ({}, {}) carries a counterbore fold - circular boring finishes \
                         the wall it can see and would silently miss the ring it cannot",
                        h.xy.x, h.xy.y
                    ),
                    mouth,
                ));
            }
            // The mouth door: the rim's circle must stay inside the
            // wall - a cutter that cannot enter the hole mouth has no
            // ride to post (never let the rc <= 0 door below become a
            // silent empty path).
            let rc = h.radius - (self.tool_diameter * 0.5 + self.stock_to_leave);
            if rc <= 0.0 {
                return Err(GenError::invalid_at(
                    format!(
                        "a {} mm cutter has no circle left inside the {} mm hole at ({}, {})",
                        self.tool_diameter,
                        h.radius * 2.0,
                        h.xy.x,
                        h.xy.y
                    ),
                    mouth,
                ));
            }
        }
        Ok(())
    }
}

/// The ride's closed wall circle at `rho`: one entry chord plus the
/// fitted circuit; `run_levels` walks it as the level's finish pass.
fn ring_loop(center: Point2, rho: f64, climb: bool) -> Vec<Point2> {
    const N: usize = 256;
    let mut pts: Vec<Point2> = Vec::with_capacity(N + 1);
    for k in 0..N {
        let t = if climb {
            PI * 2.0 * k as f64 / N as f64
        } else {
            -PI * 2.0 * k as f64 / N as f64
        };
        pts.push(Point2::new(center.x + rho * t.cos(), center.y + rho * t.sin()));
    }
    if let Some(&s) = pts.first() {
        pts.push(s);
    }
    pts
}

/// Circular-interpolation boring over a hole list (world coordinates,
/// the recognition's own frame). Returns `Ok(empty)` never: every hole
/// passed validation, so every hole earns moves.
pub fn bore(p: &BoreParams) -> Result<Toolpath, GenError> {
    p.validate()?;
    let r = p.tool_diameter * 0.5;
    let mut motion = Motion::new(p.clearance_z, p.retract_z, p.feed, p.plunge_feed);
    if p.entry != Entry3D::Plunge {
        motion = motion
            .with_entry(p.entry, p.helix_radius)
            .with_arc_radius(p.tool_diameter);
    }
    let mut motion = motion.with_arc_radius(p.tool_diameter).with_links(p.links, p.helix_radius);
    let mut b = PathBuilder::approached(Point3::new(0.0, 0.0, p.clearance_z));

    for h in &p.holes {
        let rc = h.radius - (r + p.stock_to_leave);
        // Air below the floor for a through hole; the cycles' exit
        // allowance, in the ride's own words.
        let bottom = if h.through {
            h.floor_z - p.through_allowance
        } else {
            h.floor_z
        };
        // The cutter-center disk: every dive and link chord certifies
        // inside it, the pocket's law on the wall circle's inside.
        let mut dpts: Vec<Point2> = Vec::with_capacity(257);
        for k in 0..256 {
            let t = PI * 2.0 * k as f64 / 256.0;
            dpts.push(Point2::new(h.xy.x + rc * t.cos(), h.xy.y + rc * t.sin()));
        }
        dpts.push(dpts[0]);
        let disk = Loop::new(dpts)
            .ok()
            .and_then(|l| Region::new(vec![l]).ok());
        let cert = disk.as_ref().map(Cert::Region);
        // Radial sweep opens the level (center to wall), the closed
        // ring finishes it: waterline order with one ring.
        let start = ring_loop(h.xy, rc, p.climb);
        let Some(&s0) = start.first() else {
            continue;
        };
        let sweep = vec![h.xy, s0];
        let passes = [sweep, start];
        // #871: a part-way refusal hands back the holes bored so far.
        let zs = cut_levels(h.top_z, bottom, p.stepdown).map_err(|e| e.with_ghost_moves(b.moves()))?;
        // Each hole's own entry surface is its top of cut (M11.3 B).
        if h.top_z.is_finite() {
            motion.top_z = h.top_z.min(motion.clearance_z);
        }

        // The cross between holes is `run_levels`' own first-entry
        // law: it lifts to clearance, positions over the dive point,
        // and only then opens the ladder - the same motion the pocket
        // posts between its islands, so no second hop is added here.
        run_levels(&mut b, &passes, &zs, &motion, Some(p.tolerance), cert.as_ref(), cert.as_ref());
        // Close this hole at the clearance plane so the next hole's
        // hop (or the path's end) starts from a safe Z.
        let cur = b.at();
        b.push(MoveKind::Rapid, Point3::new(cur.x, cur.y, p.clearance_z), None);
    }

    let path = b.finish();
    if let Some(why) = crate::approach::refusal(&path) {
        // #261 part 3: the moves built so far ride the refusal, display-only.
        return Err(GenError::Invalid(why).with_ghost(&path));
    }
    if !path.all_finite() {
        return Err(GenError::Invalid("path left the finite horizon".into()));
    }
    debug_assert!(!path.moves.is_empty(), "a validated bore posts moves");
    Ok(path)
}
