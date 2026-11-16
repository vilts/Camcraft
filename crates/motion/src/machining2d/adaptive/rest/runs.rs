//! The cutting runs of one rest round (issue 2031): the boundary of the
//! room the cutter may stand in without biting deeper than the step -
//! the legal center area minus the remaining material grown by the tool
//! radius less the step - cut only where it bites.
//!
//! Provenance: the engagement front of the in-process stock (J. A.
//! Stori and P. K. Wright, "Constant engagement tool path generation for
//! convex geometries", J. Manufacturing Systems 19(3), 2000) as the
//! boundary of an offset region (M. Held, "On the Computational Geometry
//! of Pocket Machining", LNCS 500, Springer 1991). The room is OPENED by
//! a disc of the phase's radius (J. Serra, "Image Analysis and
//! Mathematical Morphology", Academic Press 1982): its boundary is then
//! the envelope of a rolling disc, tangent to the walls it leaves - in
//! a corner the arc that peels it, across a neck the half loop of
//! trochoidal milling (M. Otkur and I. Lazoglu, "Trochoidal milling",
//! Int. J. Machine Tools and Manufacture 47, 2007). Riding a wall INTO
//! the wedge a larger cutter left reads a long contact arc however thin
//! the wedge; an arc tangent to the wall does not. Where a run bites is
//! read on the stock raster, never inferred from the outline. The
//! margins and thresholds are ours.

use crate::geo2d::{Point2, Region};
use crate::machining2d::GenError;

use super::super::front::dist;
use super::super::raster::Raster;
use super::canon::{Canon, Vtx};

/// One cutting run: its points in travel order, and whether it is a
/// closed ring (first point repeated last).
#[derive(Clone, Debug)]
pub(super) struct Run {
    pub v: Vec<Vtx>,
    pub closed: bool,
}

/// What the runs of a round are read with.
pub(super) struct RunSpec<'a> {
    pub center: &'a Region,
    pub canon: &'a Canon,
    pub ras: &'a Raster,
    /// Tool radius, and the depth of cut that is only a graze.
    pub r: f64,
    pub graze: f64,
    /// Longest chord of a front.
    pub seg: f64,
    /// Wall run added before and after a biting stretch.
    pub margin: f64,
    /// Biting stretches closer than this along the loop are one run.
    pub ride: f64,
    /// The radius the room is opened by; 0 leaves it as it is.
    pub rho: f64,
    pub climb: bool,
}

/// The runs of the room `center - material grown by e`, opened by the
/// phase's radius, each travelled with the material on the climb side
/// (or reversed).
pub(super) fn runs(material: &Region, e: f64, sp: &RunSpec<'_>) -> Result<Vec<Run>, GenError> {
    let room = sp.center.difference(&material.offset(e)?)?;
    // (Opened inside the room: a pass never bites deeper for it.)
    let room = if sp.rho > 0.0 { room.offset(-sp.rho)?.offset(sp.rho)?.intersect(&room)? } else { room };
    let mut out = Vec::new();
    for l in room.loops() {
        let ring = densify_fronts(&sp.canon.snap(l.points()), sp.seg);
        let n = ring.len();
        if n < 3 {
            continue;
        }
        // An edge bites where the cutter standing at its middle holds
        // material to cut: between the room's own depth and a graze.
        let flags: Vec<bool> = (0..n)
            .map(|i| {
                let (a, b) = (ring[i].p, ring[(i + 1) % n].p);
                let m = Point2::new((a.x + b.x) * 0.5, (a.y + b.y) * 0.5);
                dist(a, b) > 1e-9 && sp.ras.touches(m, e - sp.ras.h, sp.r - sp.graze)
            })
            .collect();
        for (mut v, closed) in stretches(&ring, &flags, sp.margin, sp.ride) {
            if !sp.climb {
                v.reverse();
            }
            out.push(Run { v, closed });
        }
    }
    Ok(out)
}

/// Fronts (edges off the wall) cut into chords of at most `seg`.
fn densify_fronts(ring: &[Vtx], seg: f64) -> Vec<Vtx> {
    let n = ring.len();
    let mut out = Vec::with_capacity(n * 2);
    for i in 0..n {
        let (a, b) = (ring[i], ring[(i + 1) % n]);
        out.push(a);
        let wall = matches!((a.at, b.at), (Some(p), Some(q)) if p.0 == q.0);
        let d = dist(a.p, b.p);
        if !wall && d > seg {
            let k = ((d / seg).ceil() as usize).clamp(1, 4096);
            for j in 1..k {
                let t = j as f64 / k as f64;
                out.push(Vtx { p: Point2::new(a.p.x + (b.p.x - a.p.x) * t, a.p.y + (b.p.y - a.p.y) * t), at: None });
            }
        }
    }
    out
}

/// The biting stretches of a closed ring, each grown by `margin` along
/// the ring on both sides (the pass arrives on the wall's thin strip,
/// never square onto the front), stretches closer than `ride` merged;
/// cut at vertices only. A ring that bites all round comes back closed.
fn stretches(ring: &[Vtx], flags: &[bool], margin: f64, ride: f64) -> Vec<(Vec<Vtx>, bool)> {
    let n = ring.len();
    let whole = |from: usize| -> Vec<Vtx> { (0..=n).map(|k| ring[(from + k) % n]).collect() };
    if n == 0 || !flags.iter().any(|&f| f) {
        return Vec::new();
    }
    let Some(gap) = flags.iter().position(|&f| !f) else {
        return vec![(whole(0), true)];
    };
    let mut s = vec![0.0; n + 1];
    for i in 0..n {
        s[i + 1] = s[i] + dist(ring[i].p, ring[(i + 1) % n].p);
    }
    let total = s[n];
    // Arc length at the unrolled vertex index j (j / n laps round).
    let at = |j: usize| s[j % n] + total * (j / n) as f64;
    // Biting runs as unrolled vertex ranges, walked from a gap edge so
    // none wraps the seam; offset one lap so a margin can reach back.
    let base = gap + 1 + n;
    let mut found: Vec<(usize, usize)> = Vec::new();
    let mut cur: Option<usize> = None;
    for k in 0..n {
        let e = base + k;
        match (cur, flags[e % n]) {
            (None, true) => cur = Some(e),
            (Some(a), false) => {
                found.push((a, e));
                cur = None;
            }
            _ => {}
        }
    }
    // Grow to the vertex at or past the margin, then merge.
    let mut grown: Vec<(usize, usize)> = Vec::new();
    for (a, b) in found {
        let (mut a2, mut b2) = (a, b);
        while a2 > 0 && at(a) - at(a2) < margin && a - a2 < n {
            a2 -= 1;
        }
        while at(b2) - at(b) < margin && b2 - b < n {
            b2 += 1;
        }
        match grown.last_mut() {
            Some(g) if at(a2) - at(g.1) < ride || a2 <= g.1 => g.1 = g.1.max(b2),
            _ => grown.push((a2, b2)),
        }
    }
    if grown.len() > 1
        && let (Some(&first), Some(&last)) = (grown.first(), grown.last())
        && at(first.0) + total - at(last.1) < ride
    {
        grown.pop();
        grown[0] = (last.0, first.1 + n);
    }
    grown
        .into_iter()
        .map(|(a, b)| if b - a >= n { (whole(a % n), true) } else { ((a..=b).map(|j| ring[j % n]).collect(), false) })
        .collect()
}
