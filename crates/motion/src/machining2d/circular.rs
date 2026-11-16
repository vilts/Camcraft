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
use crate::links::Links;
use crate::ring_zig::MIN_ARC_CHORD;

use super::arcs::{ARC_FIT_TOL, Seg};
use super::face::GenError;
use super::levels::{Cert, Motion, cut_levels, run_level_walks};

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
/// CCW for climb, the start pushed once more so the circuit joins
/// (trace's closure law - an unclosed ring would leave its own width
/// uncut at the seam). The polygon is what the descents (the ramp
/// along the first cut, the links) walk; the cut itself rides
/// [`ring_walk`]'s true arcs.
fn ring_loop(center: Point2, rho: f64, climb: bool) -> Vec<Point2> {
    let n = ring_chords(rho, ARC_FIT_TOL);
    let mut pts: Vec<Point2> = Vec::with_capacity(n + 1);
    for k in 0..n {
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
    let center_region: Option<Region> = {
        let mut pts: Vec<Point2> = Vec::with_capacity(RING_MAX_CHORDS + 1);
        for k in 0..RING_MAX_CHORDS {
            let t = PI * 2.0 * k as f64 / RING_MAX_CHORDS as f64;
            pts.push(Point2::new(p.center.x + rc * t.cos(), p.center.y + rc * t.sin()));
        }
        pts.push(pts[0]);
        Loop::new(pts)
            .ok()
            .and_then(|l| Region::new(vec![l]).ok())
    };
    let mut motion = Motion::new(p.clearance_z, p.retract_z, p.feed, p.plunge_feed);
    if p.entry != Entry3D::Plunge {
        motion = motion
            .with_entry(p.entry, p.helix_radius)
            .with_arc_radius(p.tool_diameter);
    }
    let motion = motion.with_arc_radius(p.tool_diameter).with_links(p.links, p.helix_radius).with_top(p.top_z).with_ring_zig();
    let cert = center_region.as_ref().map(Cert::Region);

    let mut b = PathBuilder::approached(Point3::new(0.0, 0.0, p.clearance_z));
    // Ring-major: each circle runs its own ladder to its own floor
    // before the next starts. The nesting is exact (a disk's rings
    // cannot interfere), so the order costs no re-crossing anywhere,
    // and on the vault it is what keeps every cut above the bowl.
    let mut k: usize = 0;
    loop {
        let rho = rc - p.stepover * k as f64;
        if rho <= 0.0 {
            break;
        }
        let ring = ring_loop(p.center, rho, p.climb);
        let runs = vec![ring_walk(p.center, &ring, p.climb)];
        let pass = vec![ring];
        let zs = if p.vault {
            // This ring's floor: the bowl at the band's outer edge.
            // The ladder starts where the previous ring settled (the
            // rim's first cut opens at the top plane itself).
            let floor = p.bowl_z((rho + r).min(p.radius));
            let prev = if k == 0 {
                p.top_z
            } else {
                p.bowl_z((rho + p.stepover + r).min(p.radius))
            };
            cut_levels(prev, floor, p.stepdown)
        } else {
            cut_levels(p.top_z, p.bottom_z, p.stepdown)
        }
        // #871: a part-way refusal hands back the rings cut so far.
        .map_err(|e| e.with_ghost_moves(b.moves()))?;
        run_level_walks(&mut b, &pass, &zs, &motion, Some(&runs), cert.as_ref(), cert.as_ref());
        k += 1;
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

#[cfg(test)]
mod tests {
    use super::*;

    /// #1558: the polygon's chord count comes from the tolerance, not a
    /// fixed 512 - its sagitta stays within it, and it is the fewest
    /// power of two that does (capped at the cutter-center disk's 512).
    #[test]
    fn ring_chords_are_sized_by_the_tolerance() {
        for rho in [0.05, 0.5, 1.0, 3.0, 25.0, 100.0] {
            let n = ring_chords(rho, ARC_FIT_TOL);
            let sag = |n: usize| rho * (1.0 - (PI / n as f64).cos());
            assert!(sag(n) <= ARC_FIT_TOL, "rho {rho}: {n} chords sag {}", sag(n));
            assert!(n == 8 || sag(n / 2) > ARC_FIT_TOL, "rho {rho}: {n} is not the fewest");
            assert!(n.is_power_of_two() && RING_MAX_CHORDS.is_multiple_of(n));
            let ring = ring_loop(Point2::new(0.0, 0.0), rho, true);
            assert_eq!(ring.len(), n + 1, "closed");
        }
        assert_eq!(ring_chords(1.0, ARC_FIT_TOL), 32, "R1 (the issue's ring): 32, not 512");
        assert_eq!(ring_chords(f64::NAN, ARC_FIT_TOL), RING_MAX_CHORDS);
    }

    /// #1558 review: ring ramps are the circular kernel's opt-in; every
    /// other ladder starts without it (and `push_zig_ring` re-proves
    /// the ring itself, so a wrong opt-in still posts chords).
    #[test]
    fn ring_ramps_are_opt_in() {
        assert!(!Motion::new(5.0, None, 100.0, 50.0).ring_zig);
        assert!(Motion::new(5.0, None, 100.0, 50.0).with_ring_zig().ring_zig);
    }

    /// #1558 review: a ring narrower than MIN_ARC_CHORD walks its
    /// polygon, never two half-turn arcs a post could round into full
    /// circles; a wider one walks the arcs.
    #[test]
    fn a_sub_resolution_ring_walks_its_polygon() {
        let c = Point2::new(1.0, 2.0);
        for (rho, arcs) in [(0.004, false), (0.0005, false), (0.006, true), (1.0, true)] {
            let walk = ring_walk(c, &ring_loop(c, rho, true), true);
            assert_eq!(walk.iter().all(|s| matches!(s, Seg::Arc { .. })), arcs, "rho {rho}");
        }
    }
}
