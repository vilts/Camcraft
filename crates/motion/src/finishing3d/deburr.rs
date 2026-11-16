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
//!   `drop = width / tan(half-angle)`. The cone's surface then meets
//!   the edge's own plane at radius `width` from the axis: on a
//!   horizontal top face the break is `width` wide, on the mirrored
//!   45-deg case it is exactly what `chamfer.rs` posts at a wall -
//!   the congruence test (45-deg tool, horizontal-top/vertical-wall
//!   corner) lands on `corner_z` and the wall loop's XY, byte law.
//!   No XY offset exists to get wrong: a vertical-axis cone that
//!   breaks a convex edge lies OVER it, and the corner's material -
//!   the only material there is - lies under.
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
//!   drop (#303).
//! - **The window.** A tip below `bottom_z` or above `top_z` is
//!   clipped by the segment's own line - deburr breaks the edges the
//!   operation reaches, and a rim running off the programmed floor
//!   stops there; the ceiling keeps the window honest about what
//!   sits under the rapids plane (`clearance_z >= top_z`, validated).
//! - **Prime Directive.** The cone's flank beyond one break is
//!   envelope law, not this file's claim: `verify`'s Mode::Envelope
//!   measures what the shank does near neighboring faces, and an
//!   edge nothing exposes answers `Empty` by name, never a path.

use std::collections::HashMap;

use crate::cl::{EdgeBreak, MoveKind, PathBuilder, Toolpath};
use crate::geometry::{Point3, Triangle};
use crate::machining2d::GenError;

use super::crease::{add3, chain, cmp_pt, scale3, sub3, third, vkey, vkey_scale, Seg};

/// One deburr ride's numbers. `width` is the horizontal break at the
/// edge's own plane; the tip depth follows from it and the tool's
/// cone and is never an independent input - that is what makes the
/// operation's floor a law, not a second promise (chamfer's derived
/// corner, same grammar).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DeburrParams {
    /// Break width, mm: the cone's radius where it meets the edge plane.
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
}

impl DeburrParams {
    /// How far below the edge the virtual tip stands. Stated once so
    /// the door, the pane and the path cannot disagree.
    #[must_use]
    pub fn drop(&self) -> f64 {
        self.width / (0.5 * self.tip_angle_deg.to_radians()).tan()
    }

    fn validate(&self, facets: &[Triangle]) -> Result<(), GenError> {
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

/// The edge tally: an unordered vertex pair to the witnesses standing
/// on it, each a (facet, from, to) - the directed half so a run can
/// walk the edge in its own face's order.
type EdgeBook = HashMap<(usize, usize), Vec<(usize, usize, usize)>>;

/// Break every exposed edge of the model: one chained circuit per
/// rim or crease run, deterministic in the chain law's own order
/// (runs sorted by their first point's `cmp_pt`).
pub fn deburr(facets: &[Triangle], p: &DeburrParams) -> Result<Toolpath, GenError> {
    p.validate(facets)?;
    let drop = p.drop();
    if !drop.is_finite() || drop <= 0.0 {
        // #677: names no spot - a tool/width mismatch, true at every edge.
        return Err(GenError::Invalid(format!(
            "deburr tip depth {drop} is not a standable drop for width {} at {} deg",
            p.width, p.tip_angle_deg
        )));
    }

    // Weld vertices; bucket directed edges by the unordered pair so
    // a rim and both faces of one crease meet in one entry (pencil's
    // index, the same walk).
    let s = vkey_scale(facets);
    let mut verts: Vec<Point3> = Vec::new();
    let mut ids: HashMap<(i64, i64, i64), usize> = HashMap::new();
    let mut vix: Vec<[usize; 3]> = Vec::with_capacity(facets.len());
    for f in facets {
        vix.push([
            vid(f.a, s, &mut verts, &mut ids),
            vid(f.b, s, &mut verts, &mut ids),
            vid(f.c, s, &mut verts, &mut ids),
        ]);
    }
    let mut edges: EdgeBook = HashMap::new();
    for (fi, tri) in vix.iter().enumerate() {
        for (from, to) in [(tri[0], tri[1]), (tri[1], tri[2]), (tri[2], tri[0])] {
            if from == to {
                continue; // a degenerate triangle's own repeat is no edge
            }
            let key = if from < to { (from, to) } else { (to, from) };
            edges.entry(key).or_default().push((fi, from, to));
        }
    }
    let mut keys: Vec<(usize, usize)> = edges.keys().copied().collect();
    keys.sort_unstable();

    // The cone's flank rises `slope` mm per plan mm (cot of the half-
    // angle). An edge climbing faster than that cannot be broken by a
    // vertical cone: riding it, the apex under its low end stands
    // inside the cone's reach of its high end, so the ride cuts deeper
    // than the drop - a vertical corner turns into a countersink down
    // to the window floor (#303). Such an edge is skipped out loud.
    let slope = 1.0 / (0.5 * p.tip_angle_deg.to_radians()).tan();
    let mut steeps = 0usize;
    let mut segs: Vec<Seg> = Vec::new();
    let mut flips = 0usize;
    let mut manifold = 0usize;
    for &(lo, hi) in &keys {
        let list = &edges[&(lo, hi)];
        let (e0, e1) = (verts[lo], verts[hi]);
        if list.len() == 1 {
            // A rim: an open mesh's own edge. Nothing tilts here, so
            // nothing but the window can refuse it.
            if let Some((q0, q1)) = clip(e0, e1, p, drop) {
                if steep(q0, q1, slope) {
                    steeps += 1;
                } else {
                    segs.push(seg(q0, q1, s));
                }
            }
            continue;
        }
        if list.len() > 2 {
            manifold += 1;
            continue; // a weld gone wrong is no edge to break
        }
        let (fi, a0, a1) = list[0];
        let (fj, b0, b1) = list[1];
        if (a0, a1) == (b0, b1) {
            flips += 1; // same direction in both faces: one winding is flipped
            continue;
        }
        if fi == fj {
            continue; // the same face twice is a degenerate triangle, not a crease
        }
        let (na, nb) = (facets[fi].normal(), facets[fj].normal());
        if !(na.dot(na) > 0.0 && nb.dot(nb) > 0.0) {
            continue; // a degenerate facet owns no corner
        }
        let (na, nb) = (
            scale3(na, 1.0 / na.dot(na).sqrt()),
            scale3(nb, 1.0 / nb.dot(nb).sqrt()),
        );
        if na.dot(nb).clamp(-1.0, 1.0) > 1.0 - 1e-12 {
            continue; // flat meeting: tangent faces show no edge to break
        }
        // The mirrored gate: pencil keeps corners whose opposite
        // vertices stand above the other face; what bends the OTHER
        // way (or stands under on one side - a knife edge) is convex
        // enough to burr.
        let qa = third(facets[fj], e0, e1); // inside face B
        let qb = third(facets[fi], e0, e1); // inside face A
        if sub3(qa, e0).dot(na) > 0.0 && sub3(qb, e0).dot(nb) > 0.0 {
            continue; // concave: the pencil pair's corner, not a burr
        }
        if let Some((q0, q1)) = clip(e0, e1, p, drop) {
            if steep(q0, q1, slope) {
                steeps += 1;
            } else {
                segs.push(seg(q0, q1, s));
            }
        }
    }

    let mut runs = chain(&segs);
    if runs.is_empty() {
        return Err(GenError::Empty(
            "deburr found no exposed edge inside the programmed window - an exposed edge is an open mesh's rim or a convex crease".into(),
        ));
    }
    runs.sort_by(|a, b| cmp_pt(a[0], b[0]));

    let mut b = PathBuilder::approached(Point3::new(0.0, 0.0, p.clearance_z));
    if flips > 0 {
        b.note(format!("deburr: {flips} shared edges skipped for inconsistent winding"));
    }
    if manifold > 0 {
        b.note(format!(
            "deburr: {manifold} edges touch more than two facets and were skipped"
        ));
    }
    if steeps > 0 {
        b.note(format!(
            "deburr: {steeps} edges climb steeper than the cone's flank and were skipped - a vertical cone cannot break them"
        ));
    }
    for run in &runs {
        let first = run[0];
        // Leave the previous run's cut straight up before travelling
        // (#304): the cone ends a run still in the edge it broke, so a
        // diagonal rapid from there would drag it up and across the part.
        retract(&mut b, p.clearance_z);
        b.push(
            MoveKind::Rapid,
            Point3::new(first.x, first.y, p.clearance_z),
            None,
        );
        let tip = |q: Point3| Point3::new(q.x, q.y, q.z - drop);
        b.push(MoveKind::Plunge, tip(first), Some(p.plunge_feed));
        for &pt in &run[1..] {
            b.push(MoveKind::Feed, tip(pt), Some(p.feed));
        }
    }
    retract(&mut b, p.clearance_z);
    let mut path = b.finish();
    if !path.all_finite() {
        return Err(GenError::Invalid("deburr path left the finite horizon".into()));
    }
    // The breaks this ride was asked to cut (#303): every run at the
    // edge's own height, the drop and the cone's slope - the edges the
    // window admitted, never read back off the moves.
    path.breaks = runs
        .iter()
        .map(|run| EdgeBreak { edge: run.clone(), depth: drop, slope, volume: None })
        .filter(EdgeBreak::is_sane)
        .collect();
    Ok(path)
}

/// A vertical rapid from where the path stands up to the clearance
/// plane, or nothing when it is already there (the path's start).
/// Vertical because the cone ends a run touching the edge it broke:
/// straight up is the one direction that leaves the break without
/// crossing more of the part.
fn retract(b: &mut PathBuilder, clearance_z: f64) {
    let cur = b.at();
    if cur.z < clearance_z {
        b.push(MoveKind::Rapid, Point3::new(cur.x, cur.y, clearance_z), None);
    }
}

/// Does the edge climb faster than the cone's flank (see `deburr`)?
/// A vertical edge always does.
fn steep(q0: Point3, q1: Point3, slope: f64) -> bool {
    let plan = ((q1.x - q0.x).powi(2) + (q1.y - q0.y).powi(2)).sqrt();
    (q1.z - q0.z).abs() > slope * plan + 1e-9
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

/// Keep the stretch whose TIP (z minus the drop) stands inside the
/// programmed window; None when the whole segment misses it. The
/// clip rides the segment's own line, so a rim leaving the floor is
/// cut where the floor is, not dropped whole.
fn clip(p0: Point3, p1: Point3, p: &DeburrParams, drop: f64) -> Option<(Point3, Point3)> {
    let (lo, hi) = (p.bottom_z + drop, p.top_z + drop);
    let dz = p1.z - p0.z;
    if dz.abs() < 1e-15 * (p0.z.abs() + p1.z.abs() + 1.0) {
        return (p0.z >= lo && p0.z <= hi).then_some((p0, p1));
    }
    let (mut t0, mut t1) = ((lo - p0.z) / dz, (hi - p0.z) / dz);
    if t0 > t1 {
        std::mem::swap(&mut t0, &mut t1);
    }
    let t0 = t0.clamp(0.0, 1.0);
    let t1 = t1.clamp(0.0, 1.0);
    if t1 <= t0 {
        return None;
    }
    let d = sub3(p1, p0);
    let at = |t: f64| add3(p0, scale3(d, t));
    Some((at(t0), at(t1)))
}

fn vid(pt: Point3, s: f64, verts: &mut Vec<Point3>, ids: &mut HashMap<(i64, i64, i64), usize>) -> usize {
    let k = vkey(pt, s);
    if let Some(&i) = ids.get(&k) {
        return i;
    }
    let i = verts.len();
    verts.push(pt);
    ids.insert(k, i);
    i
}
