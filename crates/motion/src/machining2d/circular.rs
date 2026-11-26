//! Circular interpolation: concentric rings climbing or falling a
//! cone-of-revolution ladder (M2.4 B).
//!
//! Provenance: circular ramp/helix interpolation on a lathe-turned or
//! milled round pocket is standard CNC practice - the cutter runs
//! closed circles at decreasing radius, each circle at the depth where
//! its band's material reaches the target surface. Flat mode is the
//! pocket waterline's law on a disk (rings at `stepover`, every ring
//! down the whole ladder); the vault mode states the same rings on a
//! paraboloid `z(r) = bottom + (top - bottom) * (r / R)^2` - the
//! smooth-wall lamp-reflector sweep this codebase names a vault.
//! Written clean-room from that surface definition.
//!
//! Ring-major, unlike the pocket's z-major grid: the rings of a disk
//! nest without interference, and on a vault each ring owns exactly
//! the depth band its plane opens into, so running a ring to its
//! floor before the next starts is the shape that never re-crosses a
//! settled surface. The plane of the ring at `rho` is the bowl at the
//! band's OUTER edge `rho + r` - the highest point of the surface the
//! ring's cut could touch - so every cut stands at or above the
//! target, and the floor of any column is the minimum over the rings
//! whose bands hold it, each of them at or above the bowl there: the
//! vault is provably never-gouge, ring spacing (rest scallop) is the
//! pocket waterline's honest remainder.
//!
//! The door `stepover <= r` is the reach law in words: the innermost
//! ring stands a step at most from the axis, and a step within the
//! cutter radius means the last circle's own disc covers the core -
//! no center special case, nothing left uncut. Entry certifies
//! against the cutter-center disk of radius `R - r - leave` exactly
//! as the pocket certifies against its eroded region. Prime
//! Directive: refused before cut, never approximated.

use std::f64::consts::PI;

use crate::cl::{MoveKind, PathBuilder, Toolpath};
use crate::entry::Entry3D;
use crate::geo2d::{Loop, Point2, Region};
use crate::geometry::Point3;
use crate::links::{Link, Links};
use crate::ring_zig::MIN_ARC_CHORD;

use super::arcs::{ARC_FIT_TOL, Seg};
use super::circular_helix::{Descents, descents, ring_phase};
use super::face::GenError;
use super::levels::{Cert, Motion, run_level_walks};
use super::{PASS_START_NOTE, PassStart};

#[path = "circular_levels.rs"]
mod circular_levels;
use circular_levels::{ring_levels, stock_top};

/// One circular ride's numbers. The circle is authored (center and
/// radius), not picked - this is the interpolation kernel, and a
/// region thinner than the tool it was drawn around is the failure it
/// must name. `vault` swaps the flat ladder for the paraboloid.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CircularParams {
    pub center: Point2,
    /// Radius of the finished circle's rim.
    pub radius: f64,
    /// Flat-floor vault target: the bowl's own floor plane. Ignored
    /// when `vault` (then `top_z`/`bottom_z` define the paraboloid).
    pub bottom_z: f64,
    pub top_z: f64,
    pub tool_diameter: f64,
    pub stock_to_leave: f64,
    pub stepover: f64,
    pub stepdown: f64,
    pub clearance_z: f64,
    pub retract_z: Option<f64>,
    pub feed: f64,
    pub plunge_feed: f64,
    /// CCW (material on the left) when true, CW reversed per ring.
    pub climb: bool,
    /// Fall ring by ring down the paraboloid instead of running every
    /// ring down the whole flat ladder.
    pub vault: bool,
    /// Radial order of the rings, independent of climb and entry phase.
    pub pass_start: PassStart,
    pub entry: Entry3D,
    pub helix_radius: f64,
    /// Level transitions and links (M11.2 C/D) within each ring's
    /// ladder; [`Links::PLANAR`] is every earlier circular op.
    pub links: Links,
}

impl CircularParams {
    pub(super) fn validate(&self) -> Result<(), GenError> {
        let mut finite = [
            self.center.x,
            self.center.y,
            self.radius,
            self.bottom_z,
            self.top_z,
            self.tool_diameter,
            self.stock_to_leave,
            self.stepover,
            self.stepdown,
            self.clearance_z,
            self.feed,
            self.plunge_feed,
            self.helix_radius,
        ]
        .into_iter()
        .all(f64::is_finite);
        finite &= self.retract_z.is_none_or(f64::is_finite);
        if !finite {
            return Err(GenError::Invalid("parameters must be finite".into()));
        }
        if self.radius <= 0.0 {
            return Err(GenError::Invalid("circle radius must be positive".into()));
        }
        if self.tool_diameter <= 0.0 {
            return Err(GenError::Invalid("tool diameter must be positive".into()));
        }
        if self.stock_to_leave < 0.0 {
            return Err(GenError::Invalid("negative stock to leave".into()));
        }
        if self.stepover <= 0.0 || self.stepdown <= 0.0 {
            return Err(GenError::Invalid("steps must be positive".into()));
        }
        // The reach law: the innermost ring must stand within a cutter
        // radius of the axis, or its disc leaves an uncut core (pocket
        // waterline's law in the disk's words).
        if self.stepover > self.tool_diameter * 0.5 {
            return Err(GenError::Invalid(
                "stepover must stay within the cutter radius - the last ring could leave an uncut core".into(),
            ));
        }
        if self.feed <= 0.0 || self.plunge_feed <= 0.0 {
            return Err(GenError::Invalid("feeds must be positive".into()));
        }
        if self.top_z < self.bottom_z {
            return Err(GenError::Invalid("cut top below cut bottom".into()));
        }
        if self.clearance_z < self.top_z {
            return Err(GenError::Invalid("clearance plane below the first cut level".into()));
        }
        if self.retract_z.is_some_and(|rz| rz < self.top_z) {
            return Err(GenError::Invalid("retract plane sits inside the material".into()));
        }
        self.links.validate().map_err(|e| GenError::Invalid(e.into()))?;
        if (self.entry == Entry3D::Helix || self.links.transition == Some(Entry3D::Helix))
            && !(0.0..=(self.tool_diameter * 0.5)).contains(&self.helix_radius)
        {
            return Err(GenError::Invalid(
                "helix radius must be positive and no larger than the cutter radius".into(),
            ));
        }
        Ok(())
    }

    /// The vault's target surface: the paraboloid through (0, bottom)
    /// and (R, top), stated in one place so the ring law above it and
    /// any later verdict read the same bowl.
    #[must_use]
    pub fn bowl_z(&self, rho: f64) -> f64 {
        let t = (rho / self.radius).min(1.0);
        self.bottom_z + (self.top_z - self.bottom_z) * t * t
    }
}

/// Most chords a ring's polygon carries: the cutter-center disk's own
/// tessellation, so every ring vertex is one of its vertices (the
/// counts below are powers of two that divide it) and a dive or link
/// that lands on a ring vertex certifies exactly as before #1558.
const RING_MAX_CHORDS: usize = 512;

/// The fewest chords (a power of two, at least 8, at most
/// [`RING_MAX_CHORDS`]) whose sagitta on a circle of radius `rho`
/// stays within `tol` (#1558): `rho * (1 - cos(pi / n)) <= tol`. A
/// non-finite or non-positive input takes the cap - the old shape.
pub(crate) fn ring_chords(rho: f64, tol: f64) -> usize {
    let mut n: usize = 8;
    if !(rho.is_finite() && tol.is_finite() && rho > 0.0 && tol > 0.0) {
        return RING_MAX_CHORDS;
    }
    while n < RING_MAX_CHORDS && rho * (1.0 - (PI / n as f64).cos()) > tol {
        n *= 2;
    }
    n
}

/// The ring's closed circle as a polygon of [`ring_chords`] chords,
/// CCW for climb, starting at vertex `phase` (#3109: a later ring's
/// helix starts clear of earlier ones; 0 is the +X vertex), the
/// start pushed once more so the circuit joins
/// (trace's closure law - an unclosed ring would leave its own width
/// uncut at the seam). The polygon is what the descents (the ramp
/// along the first cut, the links) walk; the cut itself rides
/// [`ring_walk`]'s true arcs.
fn ring_loop(center: Point2, rho: f64, climb: bool, phase: usize) -> Vec<Point2> {
    let n = ring_chords(rho, ARC_FIT_TOL);
    let mut pts: Vec<Point2> = Vec::with_capacity(n + 1);
    for k in phase..phase.saturating_add(n) {
        let t = if climb {
            PI * 2.0 * k as f64 / n as f64
        } else {
            -PI * 2.0 * k as f64 / n as f64
        };
        pts.push(Point2::new(center.x + rho * t.cos(), center.y + rho * t.sin()));
    }
    if let Some(&s) = pts.first() {
        pts.push(s);
    }
    pts
}

/// The ring's cut as two half-turn arcs about `center` (#1558): from
/// the polygon's start to its opposite vertex, then home - G3 for
/// climb, G2 otherwise, the same travel as the polygon. Two halves,
/// because a full circle has no sweep `push_arc` can tell from none.
/// A ring whose half-turn chord (its diameter) is under
/// [`MIN_ARC_CHORD`] walks its polygon instead: a post could round the
/// arc's ends together and a control would cut a full circle.
fn ring_walk(center: Point2, ring: &[Point2], climb: bool) -> Vec<Seg> {
    let n = ring.len().saturating_sub(1);
    let wide = ring.first().is_some_and(|p| 2.0 * (p.x - center.x).hypot(p.y - center.y) >= MIN_ARC_CHORD);
    match (ring.get(n / 2), ring.last()) {
        (Some(&half), Some(&home)) if n >= 2 && wide => vec![
            Seg::Arc { center, ccw: climb, to: half },
            Seg::Arc { center, ccw: climb, to: home },
        ],
        _ => vec![Seg::Line(ring.get(1..).unwrap_or(&[]).to_vec())],
    }
}

/// Concentric-ring clearance of a disk: ring 0 rides the wall at
/// `R - r - leave`, each next ring steps inward by `stepover`, and
/// the last stands a step at most from the axis - within the cutter
/// radius by the reach door, so its disc closes the core.
pub fn circular(_region: &Region, p: &CircularParams) -> Result<Toolpath, GenError> {
    p.validate()?;
    let r = p.tool_diameter * 0.5;
    let rc = p.radius - (r + p.stock_to_leave);
    if rc <= 0.0 {
        return Err(GenError::Invalid(
            "the cutter has no room inside the circle".into(),
        ));
    }
    // The cutter-center disk: the region every dive and link chord
    // must stay inside (the pocket's law, its disk's shape).
    let center_region = disk(p.center, rc);
    let forced = p.pass_start != PassStart::Auto;
    let top = if forced { stock_top(p) } else { p.links.approach.entry_top(p.top_z) };
    if forced && p.clearance_z < top {
        return Err(GenError::Invalid("the clearance plane must clear the stock above the rings".into()));
    }
    // A forced ring may enter uncut stock above Top Z. Use clearance for
    // any hop whose configured plane lies inside that stock.
    let retract = p.retract_z.filter(|&z| !forced || z >= top);
    let mut motion = Motion::new(p.clearance_z, retract, p.feed, p.plunge_feed);
    if p.entry != Entry3D::Plunge {
        motion = motion
            .with_entry(p.entry, p.helix_radius)
            .with_arc_radius(p.tool_diameter);
    }
    let mut links = p.links;
    if p.entry == Entry3D::Ramp {
        // #1807: start just above the remaining material, including stock
        // carried over Top Z; retain a smaller configured entry gap.
        links.approach.plunge_height = links.approach.plunge_height.min(0.1);
        if links.level == Link::Clearance
            && links.arm(p.entry) == Entry3D::Ramp
            && !links.approach.ramp_to_depth
        {
            // A closed ring ends at its next ramp start. Reuse the proved
            // keep-down link; each new ring still enters from clearance.
            links.level = Link::KeepDown;
        }
    }
    let motion = motion.with_arc_radius(p.tool_diameter).with_links(links, p.helix_radius).with_top(top).with_ring_zig();
    // #3109: on a flat disk every ring's helix shares each level's band.
    let helix = (!p.vault || forced) && (p.entry == Entry3D::Helix || p.links.transition == Some(Entry3D::Helix));
    let mut flown = Descents::default();

    let mut b = PathBuilder::approached(Point3::new(0.0, 0.0, p.clearance_z));
    if retract != p.retract_z && links.level != Link::Clearance {
        b.note("Circular: links that need a lift use clearance because the retract plane is inside the stock");
    }
    // Use the same radius expression in either order, including the last
    // positive remainder. A quotient rounded to an integer can contribute
    // an extra candidate at the axis; the positivity test discards it.
    super::face::pass_budget("stepover", rc, p.stepover)?;
    let last = (rc / p.stepover).floor() as usize;
    for index in 0..=last {
        let k = if p.pass_start == PassStart::Inside { last - index } else { index };
        let rho = rc - p.stepover * k as f64;
        if rho <= 0.0 {
            continue;
        }
        let zs = ring_levels(p, k, rho).map_err(|e| e.with_ghost_moves(b.moves()))?;
        if zs.is_empty() {
            continue;
        }
        // The last Inside helix on a flat floor starts on the rim. A polygon
        // with a wall margin cannot locate its internally tangent orbit.
        // Use the same analytic disk for choosing the phase and flying it.
        let rim_helix = k == 0 && p.pass_start == PassStart::Inside && helix;
        let band;
        let cert = if (forced && p.vault) || rim_helix {
            let room = if k == 0 {
                &center_region
            } else {
                band = disk(p.center, rho);
                &band
            };
            let tess = room.as_ref().ok_or_else(|| {
                let site = if p.vault { "vault ring" } else { "rim helix" };
                GenError::Invalid(format!("cannot certify the {site} at radius {rho:.6}")).with_ghost_moves(b.moves())
            })?;
            Some(Cert::RingDisk { center: p.center, radius: rho, tess })
        } else {
            center_region.as_ref().map(Cert::Region)
        };
        if forced && cert.is_none() {
            return Err(GenError::Invalid("cannot certify the circular entry region".into()).with_ghost_moves(b.moves()));
        }
        let phase = match (&cert, helix) {
            (Some(room), true) => {
                let at = |i: usize| ring_loop(p.center, rho, p.climb, i).first().copied().unwrap_or(p.center);
                ring_phase(ring_chords(rho, ARC_FIT_TOL), at, room, p.helix_radius, &flown)
            }
            _ => 0,
        };
        let ring = ring_loop(p.center, rho, p.climb, phase);
        let runs = vec![ring_walk(p.center, &ring, p.climb)];
        let pass = vec![ring];
        if forced {
            b.note_once(&format!("{PASS_START_NOTE} {}: the first ring can cut the full tool width in solid stock", p.pass_start.label()));
        }
        // #2898: climb straight up off the last ring's floor; this ring's
        // entry traverses at clearance, over the core still to be cut.
        let at = b.at();
        if at.z < p.clearance_z {
            b.push(MoveKind::Rapid, Point3::new(at.x, at.y, p.clearance_z), None);
        }
        let before = b.moves_len();
        run_level_walks(&mut b, &pass, &zs, &motion, Some(&runs), cert.as_ref(), cert.as_ref());
        if helix
            && let Err(why) = flown.add(descents(b.moves().get(before..).unwrap_or(&[])))
        {
            return Err(GenError::Invalid(why).with_ghost_moves(b.moves()));
        }
    }
    let cur = b.at();
    b.push(MoveKind::Rapid, Point3::new(cur.x, cur.y, p.clearance_z), None);

    let path = b.finish();
    if let Some(why) = crate::approach::refusal(&path) {
        // #261 part 3: the moves built so far ride the refusal, display-only.
        return Err(GenError::Invalid(why).with_ghost(&path));
    }
    if !path.all_finite() {
        return Err(GenError::Invalid("path left the finite horizon".into()));
    }
    Ok(path)
}

/// Inscribed polygon for entry room. Every possible ring phase is a
/// vertex of its own band's polygon.
fn disk(center: Point2, radius: f64) -> Option<Region> {
    let mut pts = Vec::with_capacity(RING_MAX_CHORDS + 1);
    for k in 0..RING_MAX_CHORDS {
        let t = 2.0 * PI * k as f64 / RING_MAX_CHORDS as f64;
        pts.push(Point2::new(center.x + radius * t.cos(), center.y + radius * t.sin()));
    }
    pts.push(*pts.first()?);
    Region::new(vec![Loop::new(pts).ok()?]).ok()
}

#[cfg(test)]
#[path = "circular_tests.rs"]
mod tests;
