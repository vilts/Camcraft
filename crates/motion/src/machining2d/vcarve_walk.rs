//! V-carve walk (#1443): roll the inscribed circle once around each
//! loop of the outline and keep the tool positions, refined until the
//! straight moves between them follow the true locus within the chord
//! tolerance.
//!
//! Provenance: adaptive midpoint subdivision of a parametric curve
//! (numerical-analysis textbooks) over the tangent-circle samples of
//! `vcarve_axis`; the walk of the boundary that covers each branch of
//! the medial axis from its two walls follows from the contact map of
//! the medial axis transform (Choi, Choi and Moon, 1997). Our own code.
//! No vendor material consulted.

use crate::geo2d::Point2;

use super::face::GenError;
use super::vcarve_axis::{Outline, Piece, Sample};

/// The first split of an edge, mm.
const STEP: f64 = 0.5;
/// The first split of a corner fan, radians (5 degrees).
const FAN_STEP: f64 = 0.087_266_462_599_716_47;
/// The deepest halving of one first-split interval.
const MAX_HALVINGS: u32 = 12;
/// A straight move may leave the true locus by this much, mm.
pub(super) const CHORD_TOL: f64 = 0.005;
/// The stock the cone leaves on the walls, mm: it covers the chord
/// error of the centre and of the radius together.
pub(super) const ALLOW: f64 = 2.0 * CHORD_TOL;
/// A corner that turns less than this is a chord joint of a curve, not
/// a corner of the drawing (20 degrees): the tip does not run into it.
const MIN_CORNER: f64 = 0.349_065_850_398_865_9;
/// The edge tests one V-carve may spend before it is refused.
const MAX_WORK: u64 = 4_000_000_000;

/// One tool position: the tip's XY and the radius of the cone's circle
/// in the top plane, before the wall allowance.
#[derive(Clone, Copy, Debug)]
pub(super) struct Pt {
    pub(super) c: Point2,
    pub(super) rho: f64,
}

/// What one level keeps of the walk.
#[derive(Clone, Copy)]
pub(super) struct Keep {
    /// The radius of the cone's circle at this level's depth.
    pub(super) cap_r: f64,
    /// A place whose inscribed radius is at or under this was cut to
    /// its final depth by an earlier level.
    pub(super) done_r: f64,
}

impl Keep {
    fn keeps(&self, s: &Sample) -> bool {
        if !s.r.is_finite() || s.r <= self.done_r {
            return false;
        }
        // On the wall of a wide region the cone stands a full radius
        // off the wall: no branch of the axis is ridden. Elsewhere a
        // branch into a nearly straight corner is not ridden.
        s.r >= self.cap_r || s.sep >= MIN_CORNER
    }

    fn pt(&self, s: &Sample) -> Pt {
        let rho = s.r.min(self.cap_r);
        Pt { c: s.centre(rho), rho }
    }
}

/// The kept tool positions of one loop, in walk order, with `None`
/// where the walk dropped a stretch.
pub(super) fn walk_loop(outline: &Outline, pieces: &[Piece], keep: &Keep) -> Result<Vec<Option<Pt>>, GenError> {
    let mut out: Vec<Option<Pt>> = Vec::new();
    for &piece in pieces {
        let span = outline.span(piece);
        let n = match piece {
            // An even count: the middle of each edge is a sample, where
            // a regular polygon's inscribed circle touches every edge.
            Piece::Edge(_) => (2.0 * (span / (2.0 * STEP)).ceil()).clamp(2.0, 4096.0),
            Piece::Fan(_) => (span / FAN_STEP).ceil().clamp(1.0, 72.0),
        } as usize;
        let mut a = outline.eval(piece, 0.0);
        emit(&mut out, keep, &a);
        for i in 0..n {
            let (t0, t1) = (i as f64 / n as f64, (i + 1) as f64 / n as f64);
            let b = outline.eval(piece, t1);
            refine(outline, piece, keep, (t0, &a), (t1, &b), MAX_HALVINGS, &mut out);
            a = b;
        }
        if outline.work.get() > MAX_WORK {
            return Err(GenError::Invalid(
                "V-carve region has too much detail to carve in one operation; carve fewer shapes at a time".into(),
            ));
        }
    }
    Ok(thin(out))
}

/// Drop each position that lies on the straight move between its
/// neighbours: a straight wall is one move, not one move for each
/// sample. The centre and the radius must both be on the line.
fn thin(walk: Vec<Option<Pt>>) -> Vec<Option<Pt>> {
    let tol = 0.1 * CHORD_TOL;
    let on_line = |a: &Pt, m: &Pt, b: &Pt| {
        let d = (b.c.x - a.c.x, b.c.y - a.c.y, b.rho - a.rho);
        let w = (m.c.x - a.c.x, m.c.y - a.c.y, m.rho - a.rho);
        let dd = d.0 * d.0 + d.1 * d.1 + d.2 * d.2;
        if dd <= 0.0 {
            return false;
        }
        let t = (w.0 * d.0 + w.1 * d.1 + w.2 * d.2) / dd;
        let e = (w.0 - t * d.0, w.1 - t * d.1, w.2 - t * d.2);
        (0.0..=1.0).contains(&t) && (e.0 * e.0 + e.1 * e.1 + e.2 * e.2).sqrt() <= tol
    };
    let mut out: Vec<Option<Pt>> = Vec::with_capacity(walk.len());
    // The positions dropped since the last kept one: each must stay on
    // the line to the next candidate.
    let mut dropped: Vec<Pt> = Vec::new();
    for next in walk {
        if let (Some(b), [.., Some(a), Some(m)]) = (&next, out.as_slice())
            && on_line(a, m, b)
            && dropped.iter().all(|q| on_line(a, q, b))
        {
            dropped.push(*m);
            out.pop();
            out.push(next);
            continue;
        }
        dropped.clear();
        out.push(next);
    }
    out
}

fn emit(out: &mut Vec<Option<Pt>>, keep: &Keep, s: &Sample) {
    let next = keep.keeps(s).then(|| keep.pt(s));
    match (out.last(), &next) {
        (Some(None), None) => {}
        (Some(Some(last)), Some(pt)) if same(last, pt) => {}
        _ => out.push(next),
    }
}

pub(super) fn same(a: &Pt, b: &Pt) -> bool {
    (a.c.x - b.c.x).hypot(a.c.y - b.c.y) + (a.rho - b.rho).abs() <= 0.25 * CHORD_TOL
}

/// Emit the samples after `a` up to and including `b`.
fn refine(
    outline: &Outline,
    piece: Piece,
    keep: &Keep,
    a: (f64, &Sample),
    b: (f64, &Sample),
    halvings: u32,
    out: &mut Vec<Option<Pt>>,
) {
    let (ka, kb) = (keep.keeps(a.1), keep.keeps(b.1));
    if halvings == 0 || (!ka && !kb && halvings < MAX_HALVINGS) {
        emit(out, keep, b.1);
        return;
    }
    let tm = 0.5 * (a.0 + b.0);
    let m = outline.eval(piece, tm);
    let km = keep.keeps(&m);
    if ka && kb && km {
        let (pa, pm, pb) = (keep.pt(a.1), keep.pt(&m), keep.pt(b.1));
        let ex = pm.c.x - 0.5 * (pa.c.x + pb.c.x);
        let ey = pm.c.y - 0.5 * (pa.c.y + pb.c.y);
        let er = pm.rho - 0.5 * (pa.rho + pb.rho);
        if ex.hypot(ey) + er.abs() <= CHORD_TOL {
            emit(out, keep, b.1);
            return;
        }
    } else if !ka && !kb && !km {
        emit(out, keep, b.1);
        return;
    }
    refine(outline, piece, keep, a, (tm, &m), halvings - 1, out);
    refine(outline, piece, keep, (tm, &m), b, halvings - 1, out);
}

/// May the tip feed straight from `a` to `b`? Each point between them
/// must keep the cone's circle inside the outline with a quarter of the
/// wall allowance to spare.
pub(super) fn link_ok(outline: &Outline, region: &crate::geo2d::Region, a: &Pt, b: &Pt) -> bool {
    let len = (b.c.x - a.c.x).hypot(b.c.y - a.c.y);
    if !len.is_finite() {
        return false;
    }
    let n = ((len / 0.25).ceil() as usize).clamp(1, 4096);
    (0..=n + 1).all(|i| {
        let t = i as f64 / (n + 1) as f64;
        let rho = a.rho + t * (b.rho - a.rho);
        // At the surface the tip cuts nothing.
        if rho <= ALLOW {
            return true;
        }
        let c = Point2::new(a.c.x + t * (b.c.x - a.c.x), a.c.y + t * (b.c.y - a.c.y));
        region.contains(c) && outline.dist(c) >= rho - 0.75 * ALLOW
    })
}
