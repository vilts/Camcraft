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

use super::face::GenError;
use super::levels::{Cert, Motion, cut_levels, run_levels};

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

/// The ring's closed circle: 512 chords, CCW, the start pushed once
/// more so the circuit joins (trace's closure law - an unclosed ring
/// would leave its own width uncut at the seam).
fn ring_loop(center: Point2, rho: f64, climb: bool) -> Vec<Point2> {
    const N: usize = 512;
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
        let mut pts: Vec<Point2> = Vec::with_capacity(513);
        for k in 0..512 {
            let t = PI * 2.0 * k as f64 / 512.0;
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
    let motion = motion.with_arc_radius(p.tool_diameter).with_links(p.links, p.helix_radius).with_top(p.top_z);
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
        let pass = vec![ring_loop(p.center, rho, p.climb)];
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
        run_levels(&mut b, &pass, &zs, &motion, None, cert.as_ref(), cert.as_ref());
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
