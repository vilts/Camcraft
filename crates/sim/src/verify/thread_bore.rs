//! The thread mill's grant in the solid law (#1071): a threaded hole's
//! model shows its MINOR cylinder (what CAD draws of a tapped bore), and
//! the helix cuts the thread depth past it, out to the major circle, by
//! design. Material between the two circles is the thread being cut, not
//! part the tool must keep. Specced from the ISO 724 minor/major relation
//! the generator cites (`machining2d::thread_mill`) read against our own
//! solid law ([`super::solid`]).
//!
//! The grant is narrow on purpose. A hit is excused only when BOTH hold:
//! - the facet is the thread's own material: the hole's minor wall
//!   (near-vertical, every vertex on the declared minor circle within
//!   [`WALL_BAND`]), or the face the hole opens through (a facet with a
//!   vertex on the minor circle, wholly at or above the hole's entry
//!   height, where the thread breaks out at its mouth). Either way the
//!   facet's NEAREST POINT to the axis must be on the band too, not only
//!   its vertices: a chord between two on-circle vertices (a D-bore's
//!   flat, a coarse polygon's side) dips inside the circle by its
//!   sagitta, and is read as a wall only when that dip fits the band
//!   (PR #1099 third review). The wall then stands at that nearest point,
//!   credited at most [`CHORD_CREDIT`] of the dip - tessellation of a
//!   round hole, never a flat. A floor, a wall of another radius, another
//!   hole, or anything below the entry that is not the wall is never
//!   excused;
//! - the swept cutter piece stays inside the major cylinder OF THE MODEL'S
//!   OWN WALL: the farther tip end's distance from the axis plus the
//!   atom's radius is at most the hit facet's own nearest distance to the
//!   axis plus the thread depth plus the law's allowance (PR #1099
//!   reviews).
//!
//! The band only RECOGNISES the wall; the cap measures the cut. A hole
//! declared wider than the model's, or off its centre, by more than the
//! allowance therefore convicts - the helix would cut that much past the
//! model's real major circle - however small the mismatch, and so do a
//! thread cut past its major circle and a plunge through the hole's floor.

use super::laws::dist2_seg;
use crate::geo2d::Point2;
use crate::geometry::{Point3, Triangle};

/// How far a wall vertex may stand off the declared minor circle and
/// still be read as on it, mm, as a share of the radius with a floor:
/// the drill rim law's own band (`rim_is` in the operation layer).
const WALL_BAND_SHARE: f64 = 0.02;
const WALL_BAND: f64 = 0.1;

/// The most of a facet's chord dip read as tessellation of a round wall,
/// mm: the program sim's verdict tolerance, twice the chord the STEP
/// importer meshes at (`CHORD_CAP`), so a full-fidelity round hole is
/// always credited whole and a flat that dips further is graded where
/// it stands, less this.
pub const CHORD_CREDIT: f64 = crate::stock_sim::DEV_TOL;

/// The most a wall facet's unit normal may lean out of the XY plane.
const WALL_NZ: f64 = 0.1;

/// One threaded hole: its axis in XY, the declared minor radius (the
/// model's wall), the major radius the helix is cut to, and the height
/// the hole opens at (its entry).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ThreadBore {
    pub xy: Point2,
    pub minor: f64,
    pub major: f64,
    pub top: f64,
}

impl ThreadBore {
    fn rho(&self, p: Point3) -> f64 {
        (p.x - self.xy.x).hypot(p.y - self.xy.y)
    }

    /// The model's own wall radius about the axis when `t` is the thread's
    /// own material (module doc: the minor wall, or the face the hole opens
    /// through) - the facet's nearest point to the axis in plan, which must
    /// itself be on the minor circle's band, credited at most
    /// [`CHORD_CREDIT`] toward its nearest on-circle vertex - else `None`.
    fn wall_radius(&self, t: &Triangle) -> Option<f64> {
        let band = (WALL_BAND_SHARE * self.minor).max(WALL_BAND);
        let on = |r: f64| (r - self.minor).abs() <= band;
        let ons: Vec<f64> = [t.a, t.b, t.c].iter().map(|p| self.rho(*p)).filter(|r| on(*r)).collect();
        let plan = self.plan_nearest(t);
        if ons.is_empty() || !on(plan) {
            return None;
        }
        let vertex = ons.iter().copied().fold(f64::INFINITY, f64::min);
        let nearest = plan + (vertex - plan).clamp(0.0, CHORD_CREDIT);
        let ons = ons.len();
        if ons == 3 {
            let (u, v) = (t.b - t.a, t.c - t.a);
            let n = u.cross(v);
            let len = n.dot(n).sqrt();
            if len > f64::EPSILON && (n.z / len).abs() <= WALL_NZ {
                return Some(nearest);
            }
        }
        (t.a.z.min(t.b.z).min(t.c.z) >= self.top - WALL_BAND).then_some(nearest)
    }

    /// The plan distance from the axis to triangle `t`: 0 when the axis
    /// falls inside its plan, else the nearest of its edges (a chord's
    /// middle, not its ends). Non-finite input reads as infinite, which no
    /// band holds.
    fn plan_nearest(&self, t: &Triangle) -> f64 {
        let (a, b, c) = (Point2::new(t.a.x, t.a.y), Point2::new(t.b.x, t.b.y), Point2::new(t.c.x, t.c.y));
        let o = self.xy;
        let side = |u: Point2, v: Point2| (v.x - u.x) * (o.y - u.y) - (v.y - u.y) * (o.x - u.x);
        let (d0, d1, d2) = (side(a, b), side(b, c), side(c, a));
        let area = (b.x - a.x) * (c.y - a.y) - (b.y - a.y) * (c.x - a.x);
        let mixed = (d0 < 0.0 || d1 < 0.0 || d2 < 0.0) && (d0 > 0.0 || d1 > 0.0 || d2 > 0.0);
        if area != 0.0 && !mixed {
            return 0.0;
        }
        let d2 = dist2_seg(o, a, b).min(dist2_seg(o, b, c)).min(dist2_seg(o, c, a));
        if d2.is_finite() { d2.sqrt() } else { f64::INFINITY }
    }

    /// Whether a disc of radius `rho` swept between tips `p` and `q`
    /// stays inside the major cylinder of a wall standing at `wall` from
    /// the axis, `allow` granted. The farthest point of a segment from
    /// the axis is one of its ends.
    fn holds(&self, wall: f64, p: Point3, q: Point3, rho: f64, allow: f64) -> bool {
        self.rho(p).max(self.rho(q)) + rho <= wall + (self.major - self.minor) + allow
    }
}

/// Whether the hit of an upright cutter disc of radius `rho`, swept
/// between tips `p` and `q`, on facet `t` is a thread being cut.
pub(super) fn excused(bores: &[ThreadBore], t: &Triangle, p: Point3, q: Point3, rho: f64, allow: f64) -> bool {
    bores.iter().any(|b| {
        b.minor.is_finite()
            && b.major >= b.minor
            && b.top.is_finite()
            && b.wall_radius(t).is_some_and(|wall| b.holds(wall, p, q, rho, allow))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bore(minor: f64) -> ThreadBore {
        ThreadBore { xy: Point2::new(0.0, 0.0), minor, major: minor + 0.5413, top: 20.0 }
    }

    /// A vertical wall facet on the chord between plan points `a` and `b`.
    fn wall(a: (f64, f64), b: (f64, f64)) -> Triangle {
        Triangle::new(Point3::new(a.0, a.1, 10.0), Point3::new(b.0, b.1, 10.0), Point3::new(b.0, b.1, 20.0))
    }

    fn on(r: f64, deg: f64) -> (f64, f64) {
        (r * deg.to_radians().cos(), r * deg.to_radians().sin())
    }

    #[test]
    fn a_flat_between_on_circle_corners_is_not_the_wall() {
        // A D-bore's flat from (3, 4) to (-3, 4): 1 mm inside r 5.
        assert_eq!(bore(5.0).wall_radius(&wall((3.0, 4.0), (-3.0, 4.0))), None);
        // An octagon's side on r 5 (4.62 at its middle).
        assert_eq!(bore(5.0).wall_radius(&wall(on(5.0, 0.0), on(5.0, 45.0))), None);
    }

    #[test]
    fn a_tessellated_round_wall_reads_its_circle_and_a_coarse_one_its_chord() {
        // 96 sides on r 15: an 8 um dip, credited whole.
        let fine = bore(15.0).wall_radius(&wall(on(15.0, 0.0), on(15.0, 3.75))).unwrap_or(0.0);
        assert!((fine - 15.0).abs() < 1e-9, "{fine}");
        // 32 sides on r 15: a 72 um dip, still on the band, credited only
        // CHORD_CREDIT - graded near its chord, not at its corners.
        let coarse = bore(15.0).wall_radius(&wall(on(15.0, 0.0), on(15.0, 11.25))).unwrap_or(f64::NAN);
        let chord = 15.0 * (11.25f64 / 2.0).to_radians().cos();
        assert!((coarse - (chord + CHORD_CREDIT)).abs() < 1e-9, "{coarse} vs {chord}");
    }

    #[test]
    fn a_step_reaching_inside_the_mouth_is_not_the_mouth() {
        // A top-level facet with one vertex on r 5 that reaches to r 3.
        let t = Triangle::new(Point3::new(5.0, 0.0, 20.0), Point3::new(3.0, 1.0, 20.0), Point3::new(3.0, -1.0, 20.0));
        assert_eq!(bore(5.0).wall_radius(&t), None);
        // A mouth facet outside the circle still is.
        let m = Triangle::new(Point3::new(5.0, 0.0, 20.0), Point3::new(9.0, 1.0, 20.0), Point3::new(9.0, -1.0, 20.0));
        assert!(bore(5.0).wall_radius(&m).is_some());
    }
}
