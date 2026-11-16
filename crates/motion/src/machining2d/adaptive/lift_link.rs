//! The microlift plan's returns (M11.4 C round 4, `lift`): which down
//! ramps a piece offers, and the law a return must pass - its traverse
//! legs inside the center area with their disc over no stock and no
//! wall, its descent reading no stock, no leg near the entry orbit, and
//! no leg crossing an earlier return the way the verifier's crossing
//! law reads it (two legs on one plane, or across a shared height
//! range: O'Rourke's orientation test). Ours, no vendor material.

use crate::geo2d::{Point2, Region};

use super::front::dist;
use super::lift::MICROLIFT;
use super::lift_split::{Piece, seg_dist};
use super::lift_swept::{EDGE, Swept};
use super::raster::Raster;
use super::route::{clearance, route};
use crate::machining2d::slots::chord_clears;

/// The XY run of a return's ramp down (mm; `microlift`'s rise times its
/// run per rise).
pub(super) const RAMP_RUN: f64 = 3.0 * MICROLIFT;

/// The down ramp's runs tried along the lead, as multiples of
/// [`RAMP_RUN`] (the traverse then lands elsewhere and may miss a return
/// the first would cross).
const RAMP_RUNS: [f64; 5] = [1.0, 0.6, 1.5, 2.2, 3.0];

/// The lifts a return's traverse is tried at (mm), in order: two
/// traverses cross only on one plane (the verifier's law), so a return
/// crossing another at the first lift crosses over it at the next.
pub(super) const LIFTS: [f64; 5] = [MICROLIFT, 2.0 * MICROLIFT, 3.0 * MICROLIFT, 4.0 * MICROLIFT, 5.0 * MICROLIFT];

/// A return's swept disc may graze standing stock by this much (mm,
/// the measuring law's in-cut width: half a percent of a D10) - above
/// the floor, never a cut.
pub(super) const GRAZE: f64 = 0.05;

/// A return keeps this far off the entry helix's circle (mm).
const ORBIT_GAP: f64 = 0.05;

/// A microlift return: the XY polyline it climbs along from where the
/// last piece ended (that piece's own air tail; empty: straight up), the
/// traverse's via points (none: the straight chord) and the polyline it
/// descends along, ending on its pass's first point (one point: down the
/// traverse's own last run).
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct Return {
    /// Height of the traverse over the floor (mm): [`LIFTS`].
    pub lift: f64,
    /// Taken over the retract plane instead (`high`): up and across in
    /// rapid, down in rapid onto `down`'s first point at the lift (its
    /// column holds no stock), then down `down` at feed - for a piece no
    /// lifted traverse reaches.
    pub high: bool,
    pub up: Vec<Point2>,
    pub via: Vec<Point2>,
    pub down: Vec<Point2>,
}

/// A return's legs: XY ends and their heights over the plane.
pub(super) type Leg = (Point2, Point2, f64, f64);

/// What a return is proved against.
pub(super) struct LinkLaw<'a> {
    pub ras: &'a Raster,
    pub center: &'a Region,
    pub links: &'a [Vec<Leg>],
    /// The stock the pieces cut so far swept, exactly (issue #1169).
    pub swept: &'a Swept,
    pub r: f64,
    pub rim: f64,
    pub step: f64,
    pub bite: f64,
    pub orbit: (Point2, f64),
}

/// A piece's down ramps, in the order they are tried: down the last
/// [`RAMP_RUNS`] multiples of [`RAMP_RUN`] of its air lead (the spiral's
/// own path onto it; where the lead is that long), then down the chord.
pub(super) fn ramps_of(p: &Piece) -> Vec<Vec<Point2>> {
    let mut out: Vec<Vec<Point2>> = Vec::new();
    for f in RAMP_RUNS {
        let want = f * RAMP_RUN;
        let mut acc = 0.0;
        let mut k = p.lead.len().saturating_sub(1);
        while k > 0 && acc < want {
            acc += dist(p.lead[k - 1], p.lead[k]);
            k -= 1;
        }
        if p.lead.len() >= 2 && acc >= want && out.last().is_none_or(|r| r.len() != p.lead.len() - k) {
            out.push(p.lead[k..].to_vec());
        }
    }
    out.push(vec![p.pts[0]]);
    out
}

/// The legs of the return from `a`, as `microlift` posts it.
pub(super) fn legs(a: Point2, ret: &Return) -> Vec<Leg> {
    let p = crate::machining2d::microlift::profile(a, ret);
    p.windows(2).map(|w| (w[0].0, w[1].0, w[0].1, w[1].1)).collect()
}

/// The first [`RAMP_RUN`] of `tail` past its first point (all of it
/// when shorter): the climb a return takes along a piece's air tail.
fn climb_of(tail: &[Point2]) -> Vec<Point2> {
    let mut out = Vec::new();
    let mut acc = 0.0;
    for w in tail.windows(2) {
        if acc >= RAMP_RUN {
            break;
        }
        acc += dist(w[0], w[1]);
        out.push(w[1]);
    }
    out
}

impl LinkLaw<'_> {
    /// May a traverse leg run from `a` to `b`? Inside the center area,
    /// off the orbit, its disc on no wall and on no stock but a graze of
    /// [`GRAZE`] (lifted over the floor: it never cuts).
    fn leg_ok(&self, a: Point2, b: Point2) -> bool {
        dist(a, b) < 1e-9 || (chord_clears(self.center, a, b) && self.off_orbit(a, b) && self.ras.capsule_clear(a, b, self.r - GRAZE))
    }

    fn off_orbit(&self, a: Point2, b: Point2) -> bool {
        self.orbit.1 <= 1e-6 || seg_dist(self.orbit.0, a, b) > self.orbit.1 + ORBIT_GAP
    }

    /// Is the return from `a` legal? Its climb and its descent along a
    /// lead ride the spiral's own air (the spiral crosses nothing); the
    /// traverse between them is proved leg by leg.
    #[cfg(test)]
    pub(super) fn legal(&self, a: Point2, ret: &Return) -> bool {
        self.legal_xy(a, ret) && !crosses_any(&legs(a, ret), self.links)
    }

    /// [`Self::legal`] but for the crossing law: everything the lift
    /// height does not change.
    fn legal_xy(&self, a: Point2, ret: &Return) -> bool {
        let Some(&t) = ret.down.first() else { return false };
        let lg = legs(a, ret);
        let (climb, lead) = (ret.up.len(), ret.down.len().saturating_sub(1));
        let traverse = &lg[climb.min(lg.len())..lg.len().saturating_sub(lead).max(climb.min(lg.len()))];
        let down: Vec<Point2> = if lead > 0 { ret.down.clone() } else { lg.last().map_or(vec![t], |l| vec![l.0, l.1]) };
        // A descent down the chord ends in a plunge onto the piece's
        // first point: the whole rim there must read nothing (the hop's
        // landing law, `landing`), not only the half facing the traverse.
        let plunge_clear = lead > 0 || self.ras.stock_clear(t, self.rim + self.ras.h);
        plunge_clear
            && lg.iter().all(|l| self.off_orbit(l.0, l.1))
            && self.ras.peek(&down, self.rim, self.step) <= self.bite
            && traverse.iter().all(|l| self.leg_ok(l.0, l.1))
    }

    /// The first legal return from `a` (where a piece with air `tail`
    /// ended) onto piece `p` - climbing along the tail, else straight up;
    /// down one of its ramps; straight across, or - `routed` - through
    /// the cleared area; at the lowest lift no earlier return crosses.
    pub(super) fn find(&self, a: Point2, tail: &[Point2], p: &Piece, routed: Option<&[f64]>) -> Option<Return> {
        let climb = climb_of(tail);
        let climbs: Vec<Vec<Point2>> = if climb.is_empty() { vec![climb] } else { vec![climb, Vec::new()] };
        for up in &climbs {
            for down in ramps_of(p) {
                // The route (when asked) is the same at every lift.
                let via = match routed {
                    Some(clear) => {
                        let from = up.last().copied().unwrap_or(a);
                        match route(self.ras, clear, from, down[0], (self.r - GRAZE, self.orbit, ORBIT_GAP), &|u, v| self.leg_ok(u, v)) {
                            Some(via) => via,
                            None => continue,
                        }
                    }
                    None => Vec::new(),
                };
                let mut ret = Return { lift: LIFTS[0], high: false, up: up.clone(), via, down };
                if !self.legal_xy(a, &ret) {
                    continue;
                }
                for lift in LIFTS {
                    ret.lift = lift;
                    if !crosses_any(&legs(a, &ret), self.links) {
                        return Some(ret);
                    }
                }
            }
        }
        None
    }
}

impl LinkLaw<'_> {
    /// The return over the retract plane onto `p`: down the shortest end
    /// of its lead at least [`RAMP_RUN`] long whose first point's disc
    /// holds no stock (the rapid's column), the descent reading air and
    /// crossing no earlier return's legs. The column is proved twice:
    /// on the planner's cells and on the swept stock (`lift_swept`,
    /// issue #1169) - a rapid lands only where the pieces cut so far
    /// swept the whole footprint, so a wall strip thinner than a cell
    /// is never rapided onto.
    pub(super) fn high(&self, p: &Piece) -> Option<Return> {
        let n = p.lead.len();
        let mut run = 0.0;
        for k in (0..n.saturating_sub(1)).rev() {
            run += dist(p.lead[k], p.lead[k + 1]);
            if run < RAMP_RUN || !self.ras.stock_clear(p.lead[k], self.r) || !self.swept.covers(p.lead[k], self.r - EDGE) {
                continue;
            }
            let ret = Return { lift: LIFTS[0], high: true, up: Vec::new(), via: Vec::new(), down: p.lead[k..].to_vec() };
            let lg = legs(p.lead[k], &ret);
            let ok = lg.iter().all(|l| self.off_orbit(l.0, l.1))
                && self.ras.peek(&ret.down, self.rim, self.step) <= self.bite
                && !crosses_any(&lg, self.links);
            return ok.then_some(ret);
        }
        None
    }

    /// Round 6: a late piece - reached in the walk's own order after
    /// [`Self::high`] refused it - has its lead re-read on the stock as it
    /// stands NOW (the re-ordered stock, not the spiral's, where a lead
    /// the spiral read as air may graze over the bite). Where the lead's
    /// END grazes, the graze joins the piece's cut (`extend`: walked at
    /// the floor like the spiral's own air, read like any cut) and the
    /// lead is trimmed to the air before it, so a return lands on air
    /// only - graze run by graze run, from the lead's end back, until a
    /// return (straight, else over the retract plane) reads legal.
    /// `None` leaves the ladder's hop; a lead grazing from its first point
    /// is left whole: the cut runs forward from the piece, so a graze
    /// AHEAD of the only air cannot join it (the island's corner pieces,
    /// RECOMMENDED "M11.4 C round 6": their 22 mm leads graze 20 deg to
    /// the last 3 mm, which hold stock inside the tool disc).
    pub(super) fn late(&self, at: Point2, tail: &[Point2], p: &mut Piece) -> Option<Return> {
        let marks = self.ras.clone().walk_marks(&p.lead, (self.r, self.rim, self.step));
        let mut g = marks.len();
        while let Some(end) = marks[..g].iter().rposition(|&m| m > self.bite) {
            // The graze run `end` closes starts after the last air mark.
            let j = marks[..end].iter().rposition(|&m| m <= self.bite).map_or(0, |k| k + 1);
            if j == 0 {
                return None;
            }
            let &st = p.lead.get(j)?;
            extend(p, st);
            p.lead.truncate(j + 1);
            if let Some(r) = self.find(at, tail, p, None).or_else(|| self.high(p)) {
                return Some(r);
            }
            g = j;
        }
        None
    }
}

/// The free-space map a routed return is planned on.
pub(super) fn free_map(ras: &Raster) -> Vec<f64> {
    clearance(ras)
}

/// Does any leg of `p` cross a leg of `links` by the verifier's law (a
/// proper XY crossing on one plane, or across a shared height range)?
pub(super) fn crosses_any(p: &[Leg], links: &[Vec<Leg>]) -> bool {
    let bx = |q: &[Leg]| {
        q.iter().fold((f64::INFINITY, f64::INFINITY, f64::NEG_INFINITY, f64::NEG_INFINITY), |b, v| {
            (b.0.min(v.0.x).min(v.1.x), b.1.min(v.0.y).min(v.1.y), b.2.max(v.0.x).max(v.1.x), b.3.max(v.0.y).max(v.1.y))
        })
    };
    let z_hit = |u: &Leg, v: &Leg| {
        if u.2 == u.3 && v.2 == v.3 {
            u.2 == v.2
        } else {
            u.2.max(u.3).min(v.2.max(v.3)) - u.2.min(u.3).max(v.2.min(v.3)) > 0.0
        }
    };
    let a = bx(p);
    links.iter().any(|l| {
        let b = bx(l);
        a.0 <= b.2
            && b.0 <= a.2
            && a.1 <= b.3
            && b.1 <= a.3
            && p.iter().any(|u| l.iter().any(|v| z_hit(u, v) && super::guard::proper(u.0, u.1, v.0, v.1)))
    })
}

/// Where a hop lands on `p`: its first point, or - when stock stands
/// within the rim there (a plunge onto it would bite) - the nearest
/// point of its lead with the rim clear of stock. A point whose whole
/// footprint the pieces cut so far swept (`lift_swept`, issue #1169) is
/// taken before one only the planner's cells read clear: the cells miss
/// a wall strip thinner than one. The hop comes down at plunge feed
/// from over the stock (`ladder_links::pass_link`), never in rapid, so
/// where no point is proved the cells' choice stands - a descent the
/// reader counts, not a rapid into stock.
pub(super) fn landing(ras: &Raster, swept: &Swept, p: &Piece, (r, rim): (f64, f64)) -> Point2 {
    let rad = rim + ras.h;
    let first = p.pts[0];
    let spots = || std::iter::once(first).chain(p.lead.iter().rev().copied()).filter(|&q| ras.stock_clear(q, rad));
    spots().find(|&q| swept.covers(q, r - EDGE)).or_else(|| spots().next()).unwrap_or(first)
}

/// Start `p` at `st` (a point of its lead): the lead from there on is
/// walked first (its fine chords too).
pub(super) fn extend(p: &mut Piece, st: Point2) {
    if dist(st, p.pts[0]) <= 1e-12 {
        return;
    }
    let Some(k) = p.lead.iter().position(|&q| dist(q, st) <= 1e-12) else { return };
    let grow = |pts: &[Point2]| {
        let mut out: Vec<Point2> = p.lead[k..].to_vec();
        out.extend(pts.iter().skip(1).copied());
        out.dedup_by(|a, b| dist(*a, *b) <= 1e-9);
        out
    };
    let fine = p.fine.as_deref().map(grow);
    p.pts = grow(&p.pts);
    p.fine = fine;
}
