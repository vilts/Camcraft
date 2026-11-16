//! Pre-drilled entry (#1449, first slice: the 2-D pocket): a level whose
//! certified area holds a hole an earlier operation opened is entered by
//! a straight plunge down that hole, at the plunge feed, to the level's
//! plane - no helix, no ramp - and the cut leaves the hole along the
//! dive link the plunge arm already proves (inside the cutter-centre
//! area, crossing none of the level's planned passes). The generator
//! never decides a hole is there: the caller hands over only holes it
//! has PROVED open under the tool's disc, each with the depth it is
//! open to ([`PreHole::floor`]). A level below every proved floor, or
//! with no hole the link law accepts, keeps the asked entry and says
//! why. A tool that is not centre-cutting keeps the asked entry too:
//! the plunge law reads only this path's own cuts, so a plunge down a
//! hole the path did not cut would be refused (owed, named on #1449).
//!
//!
//! So that the cut CAN leave the hole, the plan is first re-seated on it
//! ([`reseat`]): the pass point nearest the deepest hole inside the
//! cutter-centre area becomes the first pass start - a closed ring is
//! rotated to begin there, an open pass is split there into its tail
//! (cut first, forward) and its head (cut next, reversed, re-entered on
//! the column the tail just cleared) - and that pass moves to the front.
//! Being the NEAREST point of every pass, the short link from the hole
//! to it crosses no planned pass (a crossing would be nearer still).
//!
//! Provenance: the shop practice of drilling a start hole and plunging
//! the end mill down it (machine-tool handbooks: pocket milling entry
//! methods); the selection and proof rules are this crate's. No vendor
//! material consulted.

use crate::cl::lattice;
use crate::geo2d::{Point2, Region};

use super::descent::{Descent, Lvl};
use super::descent_dive::links_to;
use super::levels::Motion;

/// Height slack: a hole open exactly to the level's plane serves it.
const Z_EPS: f64 = 1e-6;

/// One hole the caller proved open: the cutter centre plunges at `at`,
/// and the tool's whole disc (plus the caller's clearance) is open down
/// to `floor` - the highest material the stock left standing under it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PreHole {
    pub at: Point2,
    pub floor: f64,
}

impl PreHole {
    fn usable(&self) -> bool {
        self.at.x.is_finite() && self.at.y.is_finite() && self.floor.is_finite()
    }
}

/// Are pre-drilled holes in play for this ladder at all?
fn active(m: &Motion) -> bool {
    m.links.approach.predrilled && m.holes.iter().any(PreHole::usable)
}

/// The entry down a proved hole onto the pass start of `l` at plane
/// `z`: the hole nearest the pass start that is open to `z` and whose
/// dive link to the start is proved. `None` when no hole serves.
pub(super) fn entry(l: &Lvl, z: f64, m: &Motion) -> Option<Descent> {
    if !active(m) || !m.links.approach.center_cutting {
        return None;
    }
    let e = *l.pass.first()?;
    let d2 = |p: Point2| (p.x - e.x).powi(2) + (p.y - e.y).powi(2);
    let h = m
        .holes
        .iter()
        .filter(|h| h.usable() && h.floor <= z + Z_EPS)
        .filter(|h| l.cen.chord_ok(h.at, h.at) && (h.at == e || links_to(l, h.at, e)))
        .min_by(|a, b| d2(a.at).total_cmp(&d2(b.at)))?;
    Some(Descent::Named(
        Box::new(Descent::Plunge(h.at)),
        format!(
            "entry: down the pre-drilled hole at ({:.3}, {:.3}), open to Z {:.3} - a straight plunge, no helix or ramp",
            h.at.x, h.at.y, h.floor
        ),
    ))
}

/// The asked entry `d`, with the reason no pre-drilled hole served this
/// level said beside it - never a silent helix where a hole was asked.
pub(super) fn unserved(d: Descent, z: f64, m: &Motion) -> Descent {
    if !active(m) {
        return d;
    }
    let why = if !m.links.approach.center_cutting {
        "the tool is not centre-cutting, and the plunge law cannot yet read a hole it did not cut".to_string()
    } else {
        let deepest = m.holes.iter().filter(|h| h.usable()).map(|h| h.floor).fold(f64::INFINITY, f64::min);
        if deepest > z + Z_EPS {
            format!("the deepest proved hole is open only to Z {deepest:.3}, above this level at Z {z:.3}")
        } else {
            format!("no hole open to Z {z:.3} lies where the cut can leave it for the pass start")
        }
    };
    Descent::Named(Box::new(d), format!("entry: pre-drilled entry not used here - {why}; the asked entry stands"))
}

/// The plan re-seated on the deepest proved hole inside `center` (see
/// the module law); the plan unchanged when pre-drilled entry is off,
/// no hole is inside, or no pass has a segment.
pub(super) fn reseat(passes: Vec<Vec<Point2>>, holes: &[PreHole], center: &Region, on: bool) -> Vec<Vec<Point2>> {
    let inside = |h: &&PreHole| h.usable() && (center.contains(h.at) || super::ring::on_boundary(center, h.at));
    let Some(h) = holes.iter().filter(inside).filter(|_| on).min_by(|a, b| a.floor.total_cmp(&b.floor)) else {
        return passes;
    };
    let mut best: Option<(f64, usize, usize, Point2)> = None;
    for (pi, pass) in passes.iter().enumerate() {
        for (i, w) in pass.windows(2).enumerate() {
            let q = nearest_on(h.at, w[0], w[1]);
            let d = (q.x - h.at.x).hypot(q.y - h.at.y);
            if d.is_finite() && best.is_none_or(|b| d < b.0) {
                best = Some((d, pi, i, q));
            }
        }
    }
    let Some((_, pi, i, q)) = best else {
        return passes;
    };
    let p = Point2::new(lattice(q.x), lattice(q.y));
    let mut rest = passes;
    let pass = rest.remove(pi);
    let n = pass.len();
    let mut front: Vec<Vec<Point2>> = Vec::new();
    if n >= 4 && pass.first() == pass.last() {
        // A closed ring: begin and end at `p`, same sense.
        let ring = &pass[..n - 1];
        let m = ring.len();
        let mut out = vec![p];
        out.extend((1..=m).map(|k| ring[(i + k) % m]));
        out.push(p);
        front.push(out);
    } else {
        let tail: Vec<Point2> = std::iter::once(p).chain(pass.iter().skip(i + 1).copied()).collect();
        let head: Vec<Point2> = std::iter::once(p).chain(pass.iter().take(i + 1).rev().copied()).collect();
        front.push(tail);
        front.push(head);
    }
    let mut out: Vec<Vec<Point2>> = front.into_iter().map(dedup).filter(|q| q.len() >= 2).collect();
    out.extend(rest);
    out
}

/// `pts` without consecutive repeats (a split ON a vertex).
fn dedup(mut pts: Vec<Point2>) -> Vec<Point2> {
    pts.dedup();
    pts
}

/// The point of segment `a -> b` nearest `p`.
fn nearest_on(p: Point2, a: Point2, b: Point2) -> Point2 {
    let (dx, dy) = (b.x - a.x, b.y - a.y);
    let dd = dx * dx + dy * dy;
    let t = if dd > 0.0 && dd.is_finite() { (((p.x - a.x) * dx + (p.y - a.y) * dy) / dd).clamp(0.0, 1.0) } else { 0.0 };
    Point2::new(a.x + t * dx, a.y + t * dy)
}

#[cfg(test)]
#[path = "predrilled_tests.rs"]
mod tests;
