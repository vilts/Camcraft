//! The pocket's horizontal arc entry (M11.3, owed): the chosen entry
//! (plunge, ramp, helix) descends at the arc's start, inside the
//! pocket's legal area, and one G17 arc runs tangentially onto the first
//! pass at the cut plane - the tangential arc lead-in contours already
//! fly, laid flat for a pocket.
//!
//! Provenance: the tangential arc lead-in of general machining practice
//! (circular interpolation in the XY plane, RS-274 / ISO 6983 G17 G2/G3
//! semantics as public G-code manuals describe them); the proof is this
//! crate's chord law (`Cert::chord_ok`) and the verifier's Crossing law
//! (`entry::chord_hits_plan`). No vendor material consulted.
//!
//! The geometry: the first cut leaves the pass start `e` along `u`. The
//! circle of radius `r` centred `r` to one side of `e` is tangent to the
//! cut there; the arc ends on `e` travelling along `u` and starts the
//! sweep `theta` back around the centre. The proof: the arc lies between
//! its inscribed chord fan (sagitta at most [`ARC_FIT_TOL`]) and its
//! circumscribed tangent polyline. Both polylines are proved inside the
//! certified area and across none of the level's planned passes, so the
//! arc - squeezed between them - stays in the legal room (no wall cut)
//! and crosses no pass. Where no radius fits, the search halves the
//! radius; past the last try the entry is flown without the arc, named.

use crate::cl::{MoveKind, PathBuilder, lattice};
use crate::entry::{Entry3D, chord_hits_plan};
use crate::entry_arc::{pts_dir, seg_dir};
use crate::geo2d::Point2;
use crate::geometry::Point3;

use super::arcs::ARC_FIT_TOL;
use super::descent::{Cuts, Descent, Lvl, plan_core, push};
use super::levels::Motion;

/// Radii tried: the nominal, then halving - at most this many.
const RADIUS_TRIES: usize = 5;

/// The most chords one arc's proof fan may hold.
const MAX_CHORDS: usize = 512;

/// A certified horizontal arc onto the pass start.
#[derive(Clone, Debug)]
pub(super) struct XyArc {
    /// Where the arc starts (the descent lands here).
    pub start: Point2,
    pub center: Point2,
    pub ccw: bool,
    pub radius: f64,
    /// The level's pass as the descent reads it: the arc's start, the
    /// inscribed fan, then the real pass from its start on.
    pub pass: Vec<Point2>,
    /// The circumscribed tangent polyline, start to the pass start.
    pub hull: Vec<Point2>,
}

impl XyArc {
    /// The cut the arc leaves at depth, as the two polylines it lies
    /// between (inscribed fan, circumscribed hull): a later link at this
    /// plane that crosses neither cannot cross the arc. `n_pass` is the
    /// real pass's length.
    pub(super) fn cuts(&self, n_pass: usize) -> [Vec<Point2>; 2] {
        let k = self.pass.len().saturating_sub(n_pass.saturating_sub(1));
        [self.pass.get(..k).unwrap_or(&[]).to_vec(), self.hull.clone()]
    }

    /// The level as the inner descent reads it: this arc's pass.
    pub(super) fn lvl<'a>(&'a self, l: &Lvl<'a>) -> Lvl<'a> {
        Lvl { cen: l.cen, passes: l.passes, pass: &self.pass, walk: None, mat_top: l.mat_top }
    }
}

/// The note when the arc fit only under its nominal radius.
pub(super) fn smaller_note(r: f64) -> String {
    format!("entry: the horizontal arc fits only under its radius - flown at {r:.3} mm")
}

/// The note when no arc fit: the entry goes straight onto the pass.
pub(super) fn dropped_note(why: &str) -> String {
    format!("entry: the horizontal arc entry does not fit ({why}) - the entry descends onto the pass start without it")
}

/// Plan the arc onto `l.pass[0]`: radius `a.arc_nominal(tool D)`,
/// halving; sweep `a.arc_sweep` (a quarter turn when `0`); both sides of
/// the cut tried at each radius. `Err` names why none fit.
pub(super) fn plan_xy(l: &Lvl, m: &Motion) -> Result<XyArc, &'static str> {
    let a = &m.links.approach;
    let (&e, rest) = l.pass.split_first().ok_or("the pass has no start")?;
    let walk = l.walk.and_then(|w| w.first());
    let unit = |d: Point2| {
        let len = d.x.hypot(d.y);
        (len.is_finite() && len > 0.0).then(|| Point2::new(d.x / len, d.y / len))
    };
    let dir = walk.map_or_else(|| pts_dir(e, rest), |s| seg_dir(e, s)).ok_or("the first cut has no direction")?;
    let u = unit(dir).ok_or("the first cut has no direction")?;
    // The closing edge's tangent is tried second, and only when it is
    // within TANGENT_SLACK of the first cut's: the arc stays tangent to
    // the first cut, and the ring behind the start is off the hull.
    let uc = closing_dir(l.pass).and_then(unit).filter(|c| {
        let cross = u.x * c.y - u.y * c.x;
        let dot = u.x * c.x + u.y * c.y;
        dot > 0.0 && cross.abs() <= TANGENT_SLACK && (c.x != u.x || c.y != u.y)
    });
    let sweep = if a.arc_sweep > 0.0 { a.arc_sweep_rad() } else { std::f64::consts::FRAC_PI_2 };
    let r0 = a.arc_nominal(m.arc_radius);
    if !(r0.is_finite() && r0 > 0.0 && sweep.is_finite() && sweep > 0.0) {
        return Err("the arc has no radius");
    }
    for u in std::iter::once(u).chain(uc) {
        let mut r = r0;
        for _ in 0..RADIUS_TRIES {
            for ccw in [true, false] {
                if let Some(arc) = arc_at(l, e, u, r, sweep, ccw) {
                    return Ok(arc);
                }
            }
            r *= 0.5;
        }
    }
    Err("no radius down to a sixteenth of the nominal stays in the legal area off the other passes")
}

/// The largest turn (radians, as the sine) between the first cut and the
/// closing edge for which the closing edge may give the arc its tangent:
/// the noise of an arc fit or a lattice-step edge, never a ring corner.
const TANGENT_SLACK: f64 = 1e-3;

/// The shortest closing edge (mm) that gives the arc its tangent: a
/// shorter one is a few lattice steps long, and the lattice turns its
/// direction by up to a degree.
const DIR_MIN_LEN: f64 = 1e-3;

/// The direction the pass arrives at its start, when its first ring
/// closes there: from the last vertex at least [`DIR_MIN_LEN`] before
/// the return. The tangent of the first cut can sit a fraction of a
/// micron off that edge (an arc fit, a lattice-step edge), and then the
/// ring behind the start crosses the arc's proof polylines on both sides
/// (#2849). On the closing edge's line, that edge is on the line itself,
/// so the ring behind the start curves to one side only. `None`: the
/// pass does not return to its start (a row, an open chain).
fn closing_dir(pass: &[Point2]) -> Option<Point2> {
    let &e = pass.first()?;
    let back = (3..pass.len()).find(|&i| pass[i] == e)?;
    let p = pass[..back].iter().rev().find(|q| (e.x - q.x).hypot(e.y - q.y) >= DIR_MIN_LEN)?;
    Some(Point2::new(e.x - p.x, e.y - p.y))
}

/// Rotate `v` by `t` radians.
fn rot(v: Point2, t: f64) -> Point2 {
    let (s, c) = t.sin_cos();
    Point2::new(v.x * c - v.y * s, v.x * s + v.y * c)
}

/// The arc at one radius and side, proved, or `None`.
pub(super) fn arc_at(l: &Lvl, e: Point2, u: Point2, r: f64, sweep: f64, ccw: bool) -> Option<XyArc> {
    // The centre on the turning side: left of `u` for G3, right for G2.
    let n = if ccw { Point2::new(-u.y, u.x) } else { Point2::new(u.y, -u.x) };
    let center = Point2::new(lattice(e.x + n.x * r), lattice(e.y + n.y * r));
    let re = Point2::new(e.x - center.x, e.y - center.y);
    // A G3 reaches `e` by turning +sweep: it starts -sweep back.
    let back = if ccw { -sweep } else { sweep };
    let s0 = rot(re, back);
    let start = Point2::new(lattice(center.x + s0.x), lattice(center.y + s0.y));
    if !(start.x.is_finite() && start.y.is_finite()) || (start.x == e.x && start.y == e.y) {
        return None;
    }
    // The fan: chord angle whose sagitta r (1 - cos(d/2)) is the tolerance.
    let half = if ARC_FIT_TOL >= r { std::f64::consts::FRAC_PI_2 } else { (1.0 - ARC_FIT_TOL / r).acos() };
    let k = (sweep / (2.0 * half)).ceil().max(1.0);
    if !(k.is_finite() && k <= MAX_CHORDS as f64) {
        return None;
    }
    let k = k as usize;
    let step = -back / k as f64;
    let at = |t: f64, scale: f64| {
        let v = rot(s0, t);
        Point2::new(center.x + v.x * scale, center.y + v.y * scale)
    };
    let mut fan = vec![start];
    for i in 1..k {
        let q = at(step * i as f64, 1.0);
        fan.push(Point2::new(lattice(q.x), lattice(q.y)));
    }
    fan.push(e);
    // The circumscribed polyline: the tangents' meeting points at the
    // mid-angles, at radius r / cos(d/2).
    let grow = 1.0 / (step.abs() * 0.5).cos();
    let mut hull = vec![start];
    for i in 0..k {
        hull.push(at(step * (i as f64 + 0.5), grow));
    }
    hull.push(e);
    let proved = |pts: &[Point2]| {
        pts.windows(2).all(|w| l.cen.chord_ok(w[0], w[1]) && !chord_hits_plan(w[0], w[1], l.passes))
    };
    if !(proved(&fan) && proved(&hull)) {
        return None;
    }
    let mut pass = fan;
    pass.extend_from_slice(l.pass.get(1..).unwrap_or(&[]));
    Some(XyArc { start, center, ccw, radius: r, pass, hull })
}

/// The ladder's plan with the horizontal arc (`None`: the ladder did not
/// opt in, or the entry is the vertical Arc - its own lead): the arc
/// planned first, the asked descent then planned onto ITS start by the
/// same laws (one entry, plunge height, centre cutting). No arc fits, or
/// no legal descent reaches its start: the entry descends onto the pass
/// start itself, the fallback named.
#[allow(clippy::too_many_arguments)]
pub(super) fn with_arc(
    arm: Entry3D,
    zig: bool,
    l: &Lvl,
    leg: Point2,
    z: f64,
    prev_z: f64,
    m: &Motion,
) -> Option<Descent> {
    if !m.xy_arc || arm == Entry3D::Arc {
        return None;
    }
    let plain = |why: &str| Descent::Named(Box::new(plan_core(arm, zig, l, leg, z, prev_z, m)), dropped_note(why));
    Some(match plan_xy(l, m) {
        Ok(x) => match plan_core(arm, zig, &x.lvl(l), leg, z, prev_z, m) {
            Descent::Refused(_) => plain("no legal descent reaches its start"),
            inner => Descent::XyArc(Box::new(inner), Box::new(x)),
        },
        Err(why) => plain(why),
    })
}

/// Post the inner descent onto the arc's start, then the G17 arc onto
/// the pass start at the lead-in feed. Returns the level's planar cuts:
/// the inner descent's and the arc's two bounding polylines.
pub(super) fn push_xy(b: &mut PathBuilder, inner: &Descent, x: &XyArc, l: &Lvl, z: f64, tz: f64, m: &Motion) -> Cuts {
    let a = &m.links.approach;
    let Some(&e) = l.pass.first() else {
        return Vec::new();
    };
    if x.radius < a.arc_nominal(m.arc_radius) {
        b.note_once(&smaller_note(x.radius));
    }
    let mut cuts = push(b, inner, &x.lvl(l), z, tz, m);
    let at = b.at();
    // Every posted descent ends on its pass start - the arc's start; a
    // refused one posts nothing (the op is refused), and no arc follows.
    if at.x == x.start.x && at.y == x.start.y && at.z == z {
        b.push_arc(MoveKind::Feed, Point3::new(e.x, e.y, z), Some(a.lead_feed(m.feed)), x.center, x.ccw);
        cuts.extend(x.cuts(l.pass.len()));
    }
    cuts
}

/// Reseat the first pass when it is one closed ring (first point = last):
/// started at the midpoint of its longest edge, the same ring walked the
/// same way - so the arc meets it mid-edge, never at a corner, where a
/// tangent arc arriving from behind would have to stand outside the
/// area. Any other first pass (a row, a chained ring) is kept as is.
pub(super) fn reseat_first(mut passes: Vec<Vec<Point2>>) -> Vec<Vec<Point2>> {
    let Some(first) = passes.first_mut() else {
        return passes;
    };
    let n = first.len();
    if n < 4 || first[0] != first[n - 1] {
        return passes;
    }
    let ring = &first[..n - 1];
    let edge = |i: usize| {
        let (a, b) = (ring[i], ring[(i + 1) % ring.len()]);
        (b.x - a.x).hypot(b.y - a.y)
    };
    let Some(i) = (0..ring.len()).max_by(|&i, &j| edge(i).total_cmp(&edge(j))) else {
        return passes;
    };
    let (a, b) = (ring[i], ring[(i + 1) % ring.len()]);
    let mid = Point2::new(lattice((a.x + b.x) * 0.5), lattice((a.y + b.y) * 0.5));
    if !(edge(i).is_finite() && mid != a && mid != b) {
        return passes;
    }
    let mut out = Vec::with_capacity(n + 1);
    out.push(mid);
    out.extend((1..=ring.len()).map(|k| ring[(i + k) % ring.len()]));
    out.push(mid);
    *first = out;
    passes
}

#[cfg(test)]
mod closing_tests {
    use super::*;

    #[test]
    fn the_arc_tangent_is_the_closing_edge_of_the_first_ring() {
        // #2849: the ring starts with a 40 nm edge whose direction the
        // lattice turns; the tangent comes from the closing edge, and a
        // closing vertex nearer than DIR_MIN_LEN is skipped.
        let e = Point2::new(0.051208, -2.296322);
        let p = Point2::new(0.0, -2.297463);
        let near = Point2::new(0.051168, -2.296323);
        let ring = [e, Point2::new(0.051248, -2.296321), Point2::new(0.100384, -2.295225), Point2::new(0.0, 5.0), p, near, e, Point2::new(3.0, 3.0)];
        assert_eq!(closing_dir(&ring), Some(Point2::new(e.x - p.x, e.y - p.y)));
        // A row does not return to its start: no closing edge.
        assert_eq!(closing_dir(&[e, p, Point2::new(1.0, 1.0)]), None);
    }
}
