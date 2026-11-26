//! Deburr (M2.5 E B): the cone-ruled edge break, in 3D.
//!
//! Provenance: a V-groove (chamfer mill) run along a convex edge is
//! machine-tool handbook geometry - the same tangent-generatrix law
//! [`super::super::machining2d::chamfer`] states on a wall, read here
//! on a real dihedral. Clean-room from that geometry and published
//! deburring practice; no vendor material of any kind.
//!
//! ## The laws here
//!
//! - **The ride.** The tool axis stays vertical through the edge
//!   point itself; the virtual tip stands `drop` below the edge,
//!   `drop = s / tan(half-angle)`. The cone's surface then meets
//!   the edge's own plane at radius `s` from the axis: on a
//!   horizontal top face the setback is `s`, on the mirrored
//!   45-deg case it is exactly what `chamfer.rs` posts at a wall -
//!   the congruence test (45-deg tool, horizontal-top/vertical-wall
//!   corner) lands on `corner_z` and the wall loop's XY, byte law.
//!   No XY offset exists to get wrong: a vertical-axis cone that
//!   breaks a convex edge lies OVER it, and the corner's material -
//!   the only material there is - lies under.
//! - **The size (#1890).** `width` is the chamfer's chord `w`; the
//!   tip drop comes from the actual cone contacts on both faces
//!   (`deburr_contact`). Constant Depth holds the perpendicular
//!   edge-to-chord distance. A rim assumes a perpendicular second face.
//!   Runs chain only edges of one drop, so each
//!   run keeps one depth and one declared break.
//! - **The loci are pencil's mirror.** An exposed edge is either a
//!   rim (a facet edge of an open mesh - one face, nothing beside
//!   it) or a convex crease (two faces, material wedge under 180°).
//!   The crease test is pencil's concavity gate read the other way:
//!   pencil keeps corners whose opposite vertices stand ABOVE the
//!   other face's plane; deburr keeps every bent pair that fails
//!   that - flat meetings (normals within the same skin) belong to
//!   neither, they are tangent, there is no edge to break. Rims are
//!   welded in 3D here, not borrowed from the drape's boundary
//!   loops: that walk flattens to XY to ground Z, and this ride
//!   needs the loop's own Z.
//! - **No steep edges.** An edge climbing faster than the cone's
//!   flank (a vertical corner, always) is skipped with a note: the
//!   apex under its low end would cut its high end deeper than the
//!   drop (#303). `UnreachableEdges::Refuse` (#1889) refuses the whole
//!   ride by name instead.
//! - **The window.** A tip below `bottom_z` or above `top_z` is
//!   clipped by the segment's own line - deburr breaks the edges the
//!   operation reaches, and a rim running off the programmed floor
//!   stops there; the ceiling keeps the window honest about what
//!   sits under the rapids plane (`clearance_z >= top_z`, validated).
//! - **Prime Directive.** The cone's flank beyond one break is
//!   envelope law, not this file's claim: `verify`'s Mode::Envelope
//!   measures what the shank does near neighboring faces, and an
//!   edge nothing exposes answers `Empty` by name, never a path.

use std::collections::BTreeMap;

use crate::cl::{EdgeBreak, PathBuilder, Toolpath};
use crate::geometry::{Point3, Triangle};
use crate::machining2d::GenError;

use super::crease::{chain, cmp_pt, vkey, vkey_scale, Seg};
use super::deburr_contact::contact_drop;
use super::deburr_filter::{DeburrEdges, EdgeFilter};
use super::deburr_scan::{scan, EdgeScan, Exposed};
use super::deburr_size::{clip, emit_runs, steep, BreakProfile, DeburrBreak, MAX_PASSES, RIM_WEDGE_DEG};

/// What the ride does with an edge that is out of the cone's reach
/// (#1889): an edge inside the programmed window that climbs steeper
/// than the cone's flank. An edge the window leaves out is no
/// candidate - the window is the machinist's choice, not a reach limit.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum UnreachableEdges {
    /// Leave the edge, break the others, and name the count in a note.
    #[default]
    SkipWithNote,
    /// Refuse the whole ride by name when one edge is out of reach.
    Refuse,
}

/// One deburr ride's numbers. `width` is the chamfer's chord (#1890);
/// the setback and the tip depth follow from it, the edge's wedge and
/// the tool's cone and are never an independent input - that is what makes the
/// operation's floor a law, not a second promise (chamfer's derived
/// corner, same grammar).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DeburrParams {
    /// Break width, mm: the chamfer's chord `w = 2 s sin(theta / 2)`
    /// across the edge (#1890), for Constant Width.
    pub width: f64,
    /// The V-tool's INCLUDED cone angle, degrees, open (0, 180).
    pub tip_angle_deg: f64,
    pub clearance_z: f64,
    /// Tip ceiling of the ride; edges whose TIP stands above it are
    /// clipped - the window's top, as every finishing op's.
    pub top_z: f64,
    /// Tip floor of the ride; edges running below it are clipped.
    pub bottom_z: f64,
    pub feed: f64,
    pub plunge_feed: f64,
    /// A convex crease that bends less than this (the angle between
    /// its two face normals, degrees) is a facet seam, not an edge:
    /// skipped (#1888). 0 keeps every crease.
    pub min_edge_angle_deg: f64,
    /// Skip the rims of recognized round holes whose diameter lies in
    /// `hole_min_diameter..=hole_max_diameter` (#1888).
    pub exclude_holes: bool,
    pub hole_min_diameter: f64,
    pub hole_max_diameter: f64,
    /// What an edge out of the cone's reach does to the ride (#1889).
    pub unreachable: UnreachableEdges,
    /// The break mode, depth, profile and passes (#1890).
    pub sizing: DeburrBreak,
}

impl DeburrParams {
    /// Nominal symmetric 90-degree reference drop for range checks.
    /// Actual path drops use both face contacts (`deburr_contact`).
    /// NaN when the size is not sane.
    #[must_use]
    pub fn drop(&self) -> f64 {
        self.drop_at(RIM_WEDGE_DEG).unwrap_or(f64::NAN)
    }

    /// Nominal symmetric wedge drop, used to check the size range and
    /// count steep edges. Actual path drops use both face contacts.
    /// `None` when the size law does not reach that wedge (#1890).
    #[must_use]
    pub fn drop_at(&self, wedge_deg: f64) -> Option<f64> {
        let s = self.sizing.setback(self.width, wedge_deg)?;
        let d = s / (0.5 * self.tip_angle_deg.to_radians()).tan();
        (d.is_finite() && d > 0.0).then_some(d)
    }

    pub(crate) fn validate(&self, facets: &[Triangle]) -> Result<(), GenError> {
        if !(self.width.is_finite()
            && self.tip_angle_deg.is_finite()
            && self.clearance_z.is_finite()
            && self.top_z.is_finite()
            && self.bottom_z.is_finite()
            && self.feed.is_finite()
            && self.plunge_feed.is_finite())
        {
            return Err(GenError::Invalid(
                "deburr parameters must be finite".into(),
            ));
        }
        if self.clearance_z < self.top_z {
            return Err(GenError::Invalid(
                "deburr clearance plane below the window top - the plunge would start inside the cut".into(),
            ));
        }
        if self.width <= 0.0 {
            return Err(GenError::Invalid("deburr width must be positive".into()));
        }
        if !self.sizing.is_sane() {
            return Err(GenError::Invalid(format!(
                "deburr break depth must be positive and passes 1 to {MAX_PASSES}: depth {}, passes {}",
                self.sizing.depth, self.sizing.passes
            )));
        }
        if self.sizing.profile == BreakProfile::Round {
            return Err(GenError::Invalid(
                "deburr: a Round break profile needs a ball-tipped cutter, and the chamfer mill is a sharp cone - set Break profile to Chamfer".into(),
            ));
        }
        if !(0.0..180.0).contains(&self.tip_angle_deg) {
            return Err(GenError::Invalid(
                "deburr needs a cone tool: an included angle of 0 or 180 deg cuts no chamfer".into(),
            ));
        }
        if self.feed <= 0.0 || self.plunge_feed <= 0.0 {
            return Err(GenError::Invalid("feeds must be positive".into()));
        }
        if facets.is_empty() {
            return Err(GenError::Empty("model has no facets".into()));
        }
        for f in facets {
            if !(f.a.is_finite() && f.b.is_finite() && f.c.is_finite()) {
                return Err(GenError::Invalid("model has a non-finite facet".into()));
            }
        }
        Ok(())
    }
}

/// Break every exposed edge of the model: one chained circuit per
/// rim or crease run, deterministic in the chain law's own order
/// (runs sorted by their first point's `cmp_pt`).
pub fn deburr(facets: &[Triangle], p: &DeburrParams) -> Result<Toolpath, GenError> {
    deburr_edges(facets, p, &DeburrEdges::default())
}

/// [`deburr`] with picked chains (#1888): `edges.include` narrows the
/// ride to the exposed edges on those chains, `edges.exclude` leaves
/// its chains out. The filter laws are `deburr_filter`'s.
pub fn deburr_edges(facets: &[Triangle], p: &DeburrParams, edges: &DeburrEdges<'_>) -> Result<Toolpath, GenError> {
    p.validate(facets)?;
    let drop = p.drop();
    if !drop.is_finite() || drop <= 0.0 {
        // #677: names no spot - a tool/width mismatch, true at every edge.
        return Err(GenError::Invalid(format!(
            "deburr tip depth {drop} is not a standable drop for width {} at {} deg on a 90-deg edge",
            p.width, p.tip_angle_deg
        )));
    }

    // The weld, the edge pairing and the convexity gate are the shared
    // scan's (#1894); the filter, the size and the reach stay here.
    let s = vkey_scale(facets);
    let mut filter = EdgeFilter::new(facets, p, edges, s)?;
    let EdgeScan { edges: exposed, flips, manifold } = scan(facets, s);

    // The cone's flank rises `slope` mm per plan mm (cot of the half-
    // angle). An edge climbing faster than that cannot be broken by a
    // vertical cone: riding it, the apex under its low end stands
    // inside the cone's reach of its high end, so the ride cuts deeper
    // than the drop - a vertical corner turns into a countersink down
    // to the window floor (#303). Such an edge is skipped out loud.
    let slope = 1.0 / (0.5 * p.tip_angle_deg.to_radians()).tan();
    let mut steeps = 0usize;
    let mut segs: Vec<(f64, Seg)> = Vec::new();
    let mut out_of_size = 0usize;
    for (e0, e1, kind) in exposed {
        let (qa, qb, bend) = match kind {
            Exposed::Rim { inside, .. } => {
                if !filter.keeps(e0, e1, None) || e0.z.max(e1.z) <= p.bottom_z {
                    continue;
                }
                if steep(e0, e1, slope) {
                    steeps += usize::from(clip(e0, e1, p, drop).is_some());
                } else {
                    let dr = contact_drop(e0, e1, inside, None, p)?;
                    if let Some((q0, q1)) = clip(e0, e1, p, dr) {
                        segs.push((dr, seg(q0, q1, s)));
                    }
                }
                continue;
            }
            Exposed::Crease { qa, qb, bend, .. } => (qa, qb, bend),
        };
        if !filter.keeps(e0, e1, Some(bend)) {
            continue;
        }
        // #1890: the wedge's own angle sizes this edge's setback.
        let wedge = 180.0 - bend.acos().to_degrees();
        let Some(dr) = p.drop_at(wedge) else {
            out_of_size += 1;
            continue;
        };
        if e0.z.max(e1.z) <= p.bottom_z {
            continue;
        }
        if steep(e0, e1, slope) {
            steeps += usize::from(clip(e0, e1, p, dr).is_some());
        } else {
            let dr = contact_drop(e0, e1, qa, Some(qb), p)?;
            if let Some((q0, q1)) = clip(e0, e1, p, dr) {
                segs.push((dr, seg(q0, q1, s)));
            }
        }
    }
    // #1889: under `Refuse` one edge out of reach stops the whole ride,
    // before any path exists - and before `Empty`, which would hide it.
    if steeps > 0 && p.unreachable == UnreachableEdges::Refuse {
        return Err(GenError::Invalid(format!(
            "deburr: {steeps} edges climb steeper than the cone's flank - a vertical cone cannot break them, and Unreachable edges is set to Refuse; set it to Skip with note to break the other edges"
        )));
    }
    // #1890: chain each depth on its own, so one run keeps one depth
    // and one declared break (a nanometre groups a depth's last bits).
    let mut groups: BTreeMap<i64, (f64, Vec<Seg>)> = BTreeMap::new();
    for (dr, sg) in segs {
        groups.entry((dr * 1e9).round() as i64).or_insert_with(|| (dr, Vec::new())).1.push(sg);
    }
    let mut runs: Vec<(Vec<Point3>, f64)> =
        groups.into_values().flat_map(|(dr, gs)| chain(&gs).into_iter().map(move |r| (r, dr))).collect();
    if runs.is_empty() {
        if let Some(why) = filter.empty() {
            return Err(why);
        }
        return Err(GenError::Empty(
            "deburr found no exposed edge inside the programmed window - an exposed edge is an open mesh's rim or a convex crease".into(),
        ));
    }
    runs.sort_by(|a, b| cmp_pt(a.0[0], b.0[0]));

    let mut b = PathBuilder::approached(Point3::new(0.0, 0.0, p.clearance_z));
    if flips > 0 {
        b.note(format!("deburr: {flips} shared edges skipped for inconsistent winding"));
    }
    if manifold > 0 {
        b.note(format!(
            "deburr: {manifold} edges touch more than two facets and were skipped"
        ));
    }
    for note in filter.notes() {
        b.note(note);
    }
    if steeps > 0 {
        b.note(format!(
            "deburr: {steeps} edges climb steeper than the cone's flank and were skipped - a vertical cone cannot break them"
        ));
    }
    if out_of_size > 0 {
        b.note(format!(
            "deburr: {out_of_size} edges have a wedge angle outside the break size's range and were skipped - Constant Width sizes wedges from 30 deg, Constant Depth up to 150 deg"
        ));
    }
    emit_runs(&mut b, &runs, p)?;
    let mut path = b.finish();
    if !path.all_finite() {
        return Err(GenError::Invalid("deburr path left the finite horizon".into()));
    }
    // The breaks this ride was asked to cut (#303): every run at the
    // edge's own height, the drop and the cone's slope - the edges the
    // window admitted, never read back off the moves.
    path.breaks = runs
        .iter()
        .map(|(run, dr)| EdgeBreak { edge: run.clone(), depth: *dr, slope, volume: None })
        .filter(EdgeBreak::is_sane)
        .collect();
    Ok(path)
}

/// Weld one edge's endpoints to a chain segment.
fn seg(p0: Point3, p1: Point3, s: f64) -> Seg {
    Seg {
        c0: p0,
        c1: p1,
        k0: vkey(p0, s),
        k1: vkey(p1, s),
    }
}
