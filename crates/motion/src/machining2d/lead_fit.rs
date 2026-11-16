//! Lead room law for contour leads: a lead-in/out is air motion, so its
//! cutter center must stay in the legal center region the walk was
//! planned in (inside the tool-radius inset for a bore, off islands;
//! outside the grown region for a wall). Specced from general CAM
//! practice (leads start in air, never in the part) and the tangent
//! circle geometry of `arcs.rs`; the fallback ladder - move the ring's
//! start to a straight run, shrink, refuse by name - is ours.

use std::f64::consts::TAU;

use crate::geo2d::Point2;

use super::arcs::{Seg, chord_dev, fit_arcs_sided, lead_in, lead_out, walk_tangent};
use super::contour::{ContourParams, Lead, air_side};
use super::fillet::Legal;
use super::ring::{Ring, dist};

/// Halvings a blocked lead gets before it is refused.
const LEAD_SHRINKS: usize = 4;

/// Sample spacing along a lead (mm): far under any lead a shop uses, and
/// the samples are only membership tests.
const LEAD_STEP: f64 = 0.02;

/// Bound on samples per piece, so a pathological radius cannot hang.
const LEAD_MAX_SAMPLES: usize = 4096;

/// Steps a lead-in start is tried at on its way in off the boundary.
const SEAT_STEPS: usize = 16;

/// A start this close past the boundary (mm) is on it: float noise.
const SEAT_NOISE: f64 = 1e-6;

/// Ring edges tried as a new start, longest first.
const RESEAT_CANDIDATES: usize = 24;

/// Slack a lead sample may stand past the center region's boundary
/// (mm): half the verifier's 10 um gouge budget, leaving the other half
/// to the offsetter's own round-join flattening (5 um). A user chord
/// tolerance tighter than this tightens it further.
const LEAD_SLACK: f64 = 0.005;

pub(super) fn slack(p: &ContourParams) -> f64 {
    p.tolerance.min(LEAD_SLACK)
}

/// The legal side of `legal`'s region, or within `tol` of its boundary.
fn holds(legal: &Legal, q: Point2, tol: f64) -> bool {
    if !(q.x.is_finite() && q.y.is_finite()) {
        return false;
    }
    if legal.region.contains(q) == legal.inside {
        return true;
    }
    legal.region.loops().iter().any(|l| {
        let pts = l.points();
        let n = pts.len();
        (0..n).any(|k| chord_dev(q, pts[k], pts[(k + 1) % n]) <= tol)
    })
}

fn line_holds(legal: &Legal, a: Point2, b: Point2, tol: f64) -> bool {
    let n = ((dist(a, b) / LEAD_STEP).ceil() as usize).clamp(1, LEAD_MAX_SAMPLES);
    (0..=n).all(|k| {
        let u = k as f64 / n as f64;
        holds(legal, Point2::new(a.x + (b.x - a.x) * u, a.y + (b.y - a.y) * u), tol)
    })
}

fn arc_holds(legal: &Legal, from: Point2, c: Point2, ccw: bool, to: Point2, tol: f64) -> bool {
    let rad = dist(from, c);
    let a0 = (from.y - c.y).atan2(from.x - c.x);
    let a1 = (to.y - c.y).atan2(to.x - c.x);
    let sweep = if ccw { a1 - a0 } else { a0 - a1 };
    if !(rad.is_finite() && sweep.is_finite()) {
        return false;
    }
    let sweep = sweep.rem_euclid(TAU);
    let n = ((sweep * rad / LEAD_STEP).ceil() as usize).clamp(8, LEAD_MAX_SAMPLES);
    let dir = if ccw { 1.0 } else { -1.0 };
    (0..=n).all(|k| {
        let a = a0 + dir * sweep * k as f64 / n as f64;
        holds(legal, Point2::new(c.x + rad * a.cos(), c.y + rad * a.sin()), tol)
    }) && holds(legal, to, tol)
}

/// Every cutter center of `segs`, walked from `from`, on the legal side.
pub(super) fn segs_hold(legal: &Legal, from: Point2, segs: &[Seg], tol: f64) -> bool {
    let mut at = from;
    for seg in segs {
        match seg {
            Seg::Line(qs) => {
                for &q in qs {
                    if !line_holds(legal, at, q, tol) {
                        return false;
                    }
                    at = q;
                }
            }
            Seg::Arc { center, ccw, to } => {
                if !arc_holds(legal, at, *center, *ccw, *to, tol) {
                    return false;
                }
                at = *to;
            }
        }
    }
    true
}

fn scaled(lead: Lead, f: f64) -> Lead {
    match lead {
        Lead::None => Lead::None,
        Lead::Linear { length } => Lead::Linear { length: length * f },
        Lead::Arc { radius } => Lead::Arc { radius: radius * f },
        Lead::Tangential { radius } => Lead::Tangential { radius: radius * f },
        Lead::Perpendicular { length } => Lead::Perpendicular { length: length * f },
    }
}

fn size(lead: Lead) -> f64 {
    match lead {
        Lead::None => 0.0,
        Lead::Linear { length } | Lead::Perpendicular { length } => length,
        Lead::Arc { radius } | Lead::Tangential { radius } => radius,
    }
}

fn kind(lead: Lead) -> &'static str {
    match lead {
        Lead::None => "no",
        Lead::Linear { .. } => "linear",
        Lead::Arc { .. } => "arc",
        Lead::Tangential { .. } => "tangential",
        Lead::Perpendicular { .. } => "perpendicular",
    }
}

fn say(notes: &mut Vec<String>, n: String) {
    if !notes.contains(&n) {
        notes.push(n);
    }
}

/// Neither end has a lead (the lead-out mirrors the lead-in unless the
/// approach sets its own, M11.3 C).
fn no_leads(p: &ContourParams) -> bool {
    p.lead == Lead::None && p.lead_out_shape() == Lead::None
}

/// Does the lead `p` asks for, scaled by `f`, fit both ends of `walk`?
fn both_fit(walk: &[Seg], poly: &[Point2], p: &ContourParams, f: f64, legal: &Legal) -> bool {
    let n = poly.len();
    if n < 2 {
        return true;
    }
    let t0 = walk_tangent(walk, poly, true);
    let (e, segs) = lead_in(poly[0], t0, air_side(t0, p.climb), scaled(p.lead, f));
    if !segs_hold(legal, e, &segs, slack(p)) {
        return false;
    }
    let t1 = walk_tangent(walk, poly, false);
    let out = lead_out(poly[n - 1], t1, air_side(t1, p.climb), scaled(p.lead_out_shape(), f));
    segs_hold(legal, poly[n - 1], &out, slack(p))
}

/// A closed ring's start, moved to the midpoint of one of its longest
/// straight runs when the given start leaves the lead no room at any
/// size (a corner of a bore's inset, where every lead leaves the room).
/// A start that holds some halving of the lead is kept - least change;
/// [`plan_leads`] then shrinks per end. Among candidates the largest
/// lead wins: full size at each (longest edge first), then each halving.
/// `None`: keep the start - it fits, there is no lead, or nothing fits
/// anywhere (then [`plan_leads`] refuses by name).
pub(super) fn reseat(
    pts: &[Point2],
    p: &ContourParams,
    legal: &Legal,
    notes: &mut Vec<String>,
) -> Option<Vec<Point2>> {
    if no_leads(p) || pts.len() < 3 {
        return None;
    }
    let walked = |v: Vec<Point2>| {
        let ring = Ring::new(v.clone());
        let poly = ring.arc_between(0.0, ring.len);
        let walk = fit_arcs_sided(&poly, p.tolerance, legal.air_left);
        (v, poly, walk)
    };
    let n = pts.len();
    let mut edges: Vec<(usize, f64)> = (0..n).map(|i| (i, dist(pts[i], pts[(i + 1) % n]))).collect();
    edges.sort_by(|a, b| b.1.total_cmp(&a.1));
    let (_, poly0, walk0) = walked(pts.to_vec());
    if (0..=LEAD_SHRINKS).any(|k| both_fit(&walk0, &poly0, p, 0.5f64.powi(k as i32), legal)) {
        return None;
    }
    let mut starts = Vec::with_capacity(RESEAT_CANDIDATES);
    for &(i, len) in edges.iter().take(RESEAT_CANDIDATES) {
        if !(len.is_finite() && len > 0.0) {
            continue;
        }
        let (a, b) = (pts[i], pts[(i + 1) % n]);
        let mut v = Vec::with_capacity(n + 1);
        v.push(Point2::new((a.x + b.x) * 0.5, (a.y + b.y) * 0.5));
        v.extend((1..=n).map(|k| pts[(i + k) % n]));
        starts.push(walked(v));
    }
    for k in 0..=LEAD_SHRINKS {
        let f = 0.5f64.powi(k as i32);
        for (v, poly, walk) in &starts {
            if !both_fit(walk, poly, p, f, legal) {
                continue;
            }
            let m = v[0];
            say(
                notes,
                format!(
                    "lead: no room for the {} lead at the ring start ({:.3}, {:.3}) - the \
                     ring starts mid-run at ({:.3}, {:.3}) instead",
                    kind(p.lead),
                    pts[0].x,
                    pts[0].y,
                    m.x,
                    m.y
                ),
            );
            return Some(v.clone());
        }
    }
    None
}

/// A lead-in start the slack let stand just past the region's
/// boundary (a straight lead tangent to a curved ring leaves the ring at
/// once: `L^2 / 2R` outside), stepped in along the air normal until it
/// stands on the legal side at least a chord tolerance clear of the
/// boundary - so the level's one descent lands on it and its at-depth
/// link reaches it without meeting the ring's own flattened chords
/// (#14). A start on the legal side or on the boundary, or one no step of
/// up to twice slack plus tolerance seats, is kept as is.
fn seat_inside(legal: &Legal, entry: Point2, air: Point2, segs: &[Seg], p: &ContourParams) -> Point2 {
    let tol = slack(p);
    let need = if p.tolerance.is_finite() { p.tolerance.max(tol) } else { tol };
    let gap = |q: Point2| {
        legal.region.loops().iter().fold(f64::INFINITY, |g, l| {
            let pts = l.points();
            let n = pts.len();
            (0..n).fold(g, |g, k| g.min(chord_dev(q, pts[k], pts[(k + 1) % n])))
        })
    };
    let legal_side = |q: Point2| legal.region.contains(q) == legal.inside;
    let clear = |q: Point2| legal_side(q) && gap(q) >= need;
    // Only a start truly past the boundary moves (one ON a straight wall
    // runs along it and keeps its axis-aligned lead), and only a straight
    // lead's: an arc's start is fixed by its center and radius.
    if legal_side(entry) || gap(entry) <= SEAT_NOISE || !segs.iter().all(|s| matches!(s, Seg::Line(_))) {
        return entry;
    }
    let reach = 2.0 * (tol + need);
    (1..=SEAT_STEPS)
        .map(|k| {
            let d = reach * k as f64 / SEAT_STEPS as f64;
            Point2::new(entry.x + air.x * d, entry.y + air.y * d)
        })
        .find(|&q| clear(q) && segs_hold(legal, q, segs, tol))
        .unwrap_or(entry)
}

/// Lead-in (plunge point + segments onto the walk) and lead-out for one
/// window, each the requested lead where it fits the legal room, halved
/// up to [`LEAD_SHRINKS`] times where it does not, and refused - the
/// cut joins or leaves the wall bare - where nothing fits. Every
/// shrink and refusal is named in `notes`.
pub(super) fn plan_leads(
    walk: &[Seg],
    poly: &[Point2],
    p: &ContourParams,
    legal: &Legal,
    notes: &mut Vec<String>,
) -> (Point2, Vec<Seg>, Vec<Seg>) {
    let n = poly.len();
    let (Some(&s), Some(&e)) = (poly.first(), poly.last()) else {
        return (Point2::new(0.0, 0.0), Vec::new(), Vec::new());
    };
    if no_leads(p) || n < 2 {
        return (s, Vec::new(), Vec::new());
    }
    let t0 = walk_tangent(walk, poly, true);
    let t1 = walk_tangent(walk, poly, false);
    let out_shape = p.lead_out_shape();
    let mut lin = None;
    let mut lout = None;
    for k in 0..=LEAD_SHRINKS {
        let f = 0.5f64.powi(k as i32);
        if lin.is_none() {
            let air = air_side(t0, p.climb);
            let (entry, segs) = lead_in(s, t0, air, scaled(p.lead, f));
            if segs_hold(legal, entry, &segs, slack(p)) {
                lin = Some((seat_inside(legal, entry, air, &segs, p), segs, f));
            }
        }
        if lout.is_none() {
            let segs = lead_out(e, t1, air_side(t1, p.climb), scaled(out_shape, f));
            if segs_hold(legal, e, &segs, slack(p)) {
                lout = Some((segs, f));
            }
        }
        if lin.is_some() && lout.is_some() {
            break;
        }
    }
    let mut word = |end: &str, lead: Lead, at: Point2, f: Option<f64>| match f {
        Some(f) if f < 1.0 => say(
            notes,
            format!(
                "lead: the {:.3} mm {} lead-{end} at ({:.3}, {:.3}) leaves the legal cutter \
                 region - shrunk to {:.3} mm",
                size(lead),
                kind(lead),
                at.x,
                at.y,
                size(lead) * f
            ),
        ),
        Some(_) => {}
        None => say(
            notes,
            format!(
                "lead: no {} lead-{end} fits the legal cutter region at ({:.3}, {:.3}) - \
                 refused, the cut meets the wall there without one",
                kind(lead),
                at.x,
                at.y
            ),
        ),
    };
    word("in", p.lead, s, lin.as_ref().map(|l| l.2));
    word("out", out_shape, e, lout.as_ref().map(|l| l.1));
    let (entry, segs_in) = lin.map_or((s, Vec::new()), |(en, sg, _)| (en, sg));
    let segs_out = lout.map_or(Vec::new(), |(sg, _)| sg);
    (entry, segs_in, segs_out)
}
