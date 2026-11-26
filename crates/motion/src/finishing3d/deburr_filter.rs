//! Deburr edge filters (#1888): which of the exposed edges the ride
//! keeps - picked chains, excluded chains, the minimum edge angle and
//! the round-hole filter. Provenance: our own point-to-segment and
//! point-to-axis arithmetic over the weld `deburr` already builds; the
//! holes are `drilling::recognize`'s own findings. No vendor material.
//!
//! ## The laws here
//!
//! - **A pick narrows, it never widens.** A picked chain selects among
//!   the edges the weld exposes (rims and convex creases). A pick on a
//!   concave or flat meeting keeps nothing: a vertical cone over a
//!   concave corner cuts both faces, so the pick is spoken, not ridden.
//! - **A pick is the machinist's word.** The minimum edge angle and
//!   the hole filter judge the AUTOMATIC edge set only; an excluded
//!   chain is left out of both.
//! - **The bend.** A crease's angle is the angle between its two
//!   face normals; a crease that bends less than the minimum is the
//!   facet seam of a smooth surface, not an edge.

use crate::drilling::{recognize, recognize_cross};
use crate::geometry::{Point3, Triangle};
use crate::machining2d::GenError;

use super::crease::{add3, scale3, sub3};
use super::deburr::DeburrParams;

/// The picked edge chains of one deburr ride, as polylines in the
/// facets' own frame. Both lists empty is the automatic ride.
#[derive(Clone, Copy, Debug, Default)]
pub struct DeburrEdges<'a> {
    /// Ride only the exposed edges that lie on these chains.
    pub include: &'a [Vec<Point3>],
    /// Leave out the exposed edges that lie on these chains.
    pub exclude: &'a [Vec<Point3>],
}

/// How far a rim vertex may stand off a recognized hole's cylinder:
/// the recognizer's own fit tolerance (20 um) plus its lattice snap.
const RIM_TOL: f64 = 0.05;

/// One recognized hole as an axis: a point on it, its unit direction,
/// the radius and the wall's span along the direction.
struct Bore {
    origin: Point3,
    dir: Point3,
    radius: f64,
    lo: f64,
    hi: f64,
}

impl Bore {
    fn holds(&self, q: Point3) -> bool {
        let v = sub3(q, self.origin);
        let d = v.dot(self.dir);
        if d < self.lo - RIM_TOL || d > self.hi + RIM_TOL {
            return false;
        }
        let r = sub3(v, scale3(self.dir, d));
        (r.dot(r).sqrt() - self.radius).abs() <= RIM_TOL
    }
}

/// The edge verdicts of one ride and the tally its notes speak.
pub(crate) struct EdgeFilter {
    include: Vec<(usize, Point3, Point3)>,
    hit: Vec<bool>,
    exclude: Vec<(usize, Point3, Point3)>,
    bores: Vec<Bore>,
    min_deg: f64,
    tol: f64,
    shallow: usize,
    excluded: usize,
    in_holes: usize,
}

impl EdgeFilter {
    /// Validate the filter numbers and read the holes. `weld` is the
    /// mesh's own weld scale: a chain point stands on a mesh vertex to
    /// a thousand of them.
    pub(crate) fn new(facets: &[Triangle], p: &DeburrParams, edges: &DeburrEdges<'_>, weld: f64) -> Result<Self, GenError> {
        if !(p.min_edge_angle_deg.is_finite() && (0.0..180.0).contains(&p.min_edge_angle_deg)) {
            return Err(GenError::Invalid(format!(
                "deburr minimum edge angle must be 0 or more and less than 180 deg, not {}",
                p.min_edge_angle_deg
            )));
        }
        let mut bores = Vec::new();
        if p.exclude_holes {
            let (lo, hi) = (p.hole_min_diameter, p.hole_max_diameter);
            if !(lo.is_finite() && hi.is_finite() && lo >= 0.0 && hi >= lo) {
                return Err(GenError::Invalid(format!(
                    "deburr hole diameter range must be 0 <= minimum <= maximum, not {lo} to {hi}"
                )));
            }
            let wanted = |radius: f64| (lo..=hi).contains(&(2.0 * radius));
            for h in recognize(facets).into_iter().filter(|h| wanted(h.radius)) {
                bores.push(Bore {
                    origin: Point3::new(h.xy.x, h.xy.y, 0.0),
                    dir: Point3::new(0.0, 0.0, 1.0),
                    radius: h.radius,
                    lo: h.floor_z.min(h.entry_z),
                    hi: h.floor_z.max(h.entry_z),
                });
            }
            // A directed hole's wall point at travel `d` stands at
            // `origin - dir * d` (`CrossHole`): along `dir` the wall
            // spans the negated travel scalars.
            for c in recognize_cross(facets).into_iter().filter(|c| wanted(c.radius)) {
                bores.push(Bore {
                    origin: c.origin,
                    dir: c.dir,
                    radius: c.radius,
                    lo: (-c.floor_d).min(-c.entry_d),
                    hi: (-c.floor_d).max(-c.entry_d),
                });
            }
        }
        Ok(Self {
            include: segments(edges.include)?,
            hit: vec![false; edges.include.len()],
            exclude: segments(edges.exclude)?,
            bores,
            min_deg: p.min_edge_angle_deg,
            tol: weld * 1e3,
            shallow: 0,
            excluded: 0,
            in_holes: 0,
        })
    }

    /// Does the ride keep the exposed edge e0-e1? `bend_cos` is the
    /// dot of a crease's two unit normals; a rim has none.
    pub(crate) fn keeps(&mut self, e0: Point3, e1: Point3, bend_cos: Option<f64>) -> bool {
        let manual = !self.hit.is_empty();
        if manual {
            let Some(chain) = on_chain(&self.include, e0, e1, self.tol) else {
                return false; // not picked: nothing to say
            };
            if let Some(h) = self.hit.get_mut(chain) {
                *h = true;
            }
        } else if let Some(cos) = bend_cos
            && cos.clamp(-1.0, 1.0).acos().to_degrees() < self.min_deg - 1e-9
        {
            self.shallow = self.shallow.saturating_add(1);
            return false;
        }
        if on_chain(&self.exclude, e0, e1, self.tol).is_some() {
            self.excluded = self.excluded.saturating_add(1);
            return false;
        }
        if !manual && self.bores.iter().any(|b| b.holds(e0) && b.holds(e1)) {
            self.in_holes = self.in_holes.saturating_add(1);
            return false;
        }
        true
    }

    /// The refusal of a ride that kept nothing, by the filter's name
    /// when a pick or a filter emptied it; `None` is the kernel's own.
    pub(crate) fn empty(&self) -> Option<GenError> {
        if !self.hit.is_empty() {
            return Some(GenError::Empty(
                "deburr: the picked edges hold no exposed edge inside the programmed window - a picked edge must be an open mesh's rim or a convex crease".into(),
            ));
        }
        (self.shallow + self.excluded + self.in_holes > 0).then(|| {
            GenError::Empty(format!(
                "deburr: the edge filters left no exposed edge inside the programmed window ({} below the minimum edge angle, {} excluded, {} on filtered holes)",
                self.shallow, self.excluded, self.in_holes
            ))
        })
    }

    /// What the filters skipped, spoken once each.
    pub(crate) fn notes(&self) -> Vec<String> {
        let mut out = Vec::new();
        let missed = self.hit.iter().filter(|h| !**h).count();
        if missed > 0 {
            out.push(format!(
                "deburr: {missed} picked edge chains lie on no rim or convex crease and were skipped"
            ));
        }
        if self.shallow > 0 {
            out.push(format!(
                "deburr: {} convex edges bend less than {} deg and were skipped",
                self.shallow, self.min_deg
            ));
        }
        if self.excluded > 0 {
            out.push(format!("deburr: {} edges excluded by pick", self.excluded));
        }
        if self.in_holes > 0 {
            out.push(format!(
                "deburr: {} edges on round holes inside the diameter range were skipped",
                self.in_holes
            ));
        }
        out
    }
}

/// Every chain's segments, each tagged with its chain's index.
fn segments(chains: &[Vec<Point3>]) -> Result<Vec<(usize, Point3, Point3)>, GenError> {
    let mut out = Vec::new();
    for (i, ch) in chains.iter().enumerate() {
        if ch.iter().any(|q| !q.is_finite()) {
            return Err(GenError::Invalid(format!(
                "deburr: picked edge chain {} has a non-finite point",
                i.saturating_add(1)
            )));
        }
        out.extend(ch.windows(2).map(|w| (i, w[0], w[1])));
    }
    Ok(out)
}

/// The chain one of whose segments carries BOTH ends of the edge.
fn on_chain(segs: &[(usize, Point3, Point3)], e0: Point3, e1: Point3, tol: f64) -> Option<usize> {
    segs.iter()
        .find(|&&(_, a, b)| off_segment(e0, a, b) <= tol && off_segment(e1, a, b) <= tol)
        .map(|&(i, _, _)| i)
}

/// Distance from `q` to the segment a-b.
fn off_segment(q: Point3, a: Point3, b: Point3) -> f64 {
    let d = sub3(b, a);
    let len2 = d.dot(d);
    let t = if len2 > 0.0 { (sub3(q, a).dot(d) / len2).clamp(0.0, 1.0) } else { 0.0 };
    let off = sub3(q, add3(a, scale3(d, t)));
    off.dot(off).sqrt()
}
