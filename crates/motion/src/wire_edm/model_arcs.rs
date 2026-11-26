//! Arcs read back from a meshed model's wire profile (#3215). A model is
//! meshed at import, so its section is a polygon: each circle and fillet
//! is a run of short chords whose vertices stand on the true surface. A
//! run of such vertices that lies on one circle is read back as that
//! circle's arc, and the closed chain of arcs and the remaining lines is
//! cut by the analytic wire contour (`arc_cut`, #3937), so the posts
//! write G2 / G3 in place of chords.
//!
//! - **Fit.** An arc stands on at least [`MIN_CHORDS`] chords. Its circle
//!   passes through the run's two ends and its middle vertex, every other
//!   vertex of the run stands within [`FIT_TOL`] of it, the run turns one
//!   way about its centre, and its radius is from [`MIN_RADIUS`] to
//!   [`MAX_RADIUS`]. Each chord is at least [`MIN_CHORD`] long, so a
//!   cluster of noise vertices is never read as a tiny circle. Vertices
//!   within [`FIT_TOL`] of the line between their kept neighbours (split
//!   points that a section leaves on a chord, beside a polygon corner
//!   too) are dropped first, so they never bend a corner into an arc. A run whose vertices all stand within [`FIT_TOL`] of
//!   its end chord is a straight line, never an arc.
//! - **Bound.** The arc bows off each chord of its run by the chord's
//!   sagitta. That sagitta must stay within the model's chord tolerance
//!   (the mesh's own promise of how far a facet stands from the true
//!   surface). With the vertex fit, the arc never leaves the facets by
//!   more than that chord plus [`FIT_TOL`] (the tolerance the verifier
//!   judges the cut under), and a true polygon (an octagon, a hexagon)
//!   whose sides bow farther stays a polygon.
//! - **Fallback.** A profile with no fitted arc returns `None`: the
//!   caller cuts it on the polygon path as before, byte for byte.
//!
//! Provenance: our own implementation. Plane geometry only: the
//! circumcircle of three points, the sagitta of a chord
//! `r - sqrt(r^2 - (L/2)^2)`, and the property of a tessellated circle
//! that its vertices lie on the circle (inscribed chords).

use std::cmp::Reverse;
use std::collections::BinaryHeap;
use std::f64::consts::{PI, TAU};

use crate::cl::Toolpath;
use crate::geo2d::{Loop, Point2};
use crate::machining2d::GenError;

use super::arc_chain::ArcChain;
use super::arc_span::{cross, dist, dot, norm, sub, Span};
use super::model_tangent::snap_tangents;
use super::{wire_contour_arcs, wire_contour_tapered, WireParams, WireTaper};

/// How far (mm) a vertex may stand off its fitted circle: 1 um, less
/// than half the cover law's tolerance.
pub const FIT_TOL: f64 = 1e-3;

/// The fewest chords an arc stands on. A shorter run (a tiny fillet
/// meshed with one or two chords) stays lines.
pub const MIN_CHORDS: usize = 3;

/// The smallest radius (mm) read as an arc.
pub const MIN_RADIUS: f64 = 0.01;

/// The largest radius (mm) read as an arc; a flatter run stays lines.
pub const MAX_RADIUS: f64 = 1000.0;

/// The shortest chord (mm) an arc may stand on: 5 um. Vertices nearer
/// than that are float noise of the mesh or the section (a cluster at
/// a corner), and any small circle holds them; such a run stays lines.
pub const MIN_CHORD: f64 = 5.0 * FIT_TOL;

/// The longest vertex run one arc may span (bounds the greedy walk).
const MAX_RUN: usize = 4096;

/// A vertex within the fit tolerance of the line through its kept
/// neighbours, and between them, is dropped before the fit: the split
/// points that a section or a silhouette leaves on a facet chord (float
/// noise puts them up to a micrometre off it).
const COLLINEAR: f64 = FIT_TOL;

/// The note the path carries when its arcs were read from the facets.
pub const MODEL_ARCS_NOTE: &str = "wire EDM: arcs read from the model's facets";

/// A model profile read back as lines and arcs.
#[derive(Clone, Debug)]
pub struct ModelArcs {
    pub chain: ArcChain,
    /// How many arcs were read.
    pub arcs: usize,
    /// The model chord tolerance (mm) the fit was bounded by.
    pub chord: f64,
    /// The widest bow (mm) of an arc off one of its chords.
    pub worst_sag: f64,
}

/// One fitted run: centre, radius, sense, sweep and its widest sagitta.
struct Fit {
    c: Point2,
    r: f64,
    ccw: bool,
    sweep: f64,
    sag: f64,
}

/// The one closed model profile of `loops` read back as lines and arcs,
/// each arc bounded by the model's `chord` tolerance (mm). `None` when
/// the profile is not exactly one closed loop, when `chord` is not a
/// positive number, or when no run of its vertices reads as an arc.
#[must_use]
pub fn model_arcs(loops: &[Loop], chains: &[Vec<Point2>], chord: f64) -> Option<ModelArcs> {
    if !chains.is_empty() || !(chord.is_finite() && chord > 0.0) {
        return None;
    }
    let [one] = loops else { return None };
    fit_ring(one.points(), chord)
}

/// [`model_arcs`] for one closed ring of points.
#[must_use]
pub fn fit_ring(ring: &[Point2], chord: f64) -> Option<ModelArcs> {
    let v = simplify(ring);
    let n = v.len();
    if n < MIN_CHORDS + 1 || !v.iter().all(|p| p.x.is_finite() && p.y.is_finite()) {
        return None;
    }
    let bound = chord;
    if let Some(f) = full_circle(&v, bound) {
        let a = v[0];
        let chain = ArcChain::new(vec![Span::Arc { c: f.c, r: f.r, a, b: a, ccw: f.ccw, sweep: TAU }]).ok()?;
        return Some(ModelArcs { chain, arcs: 1, chord, worst_sag: f.sag });
    }
    // Start after the longest edge: a straight side ends there, so no arc
    // is split at the walk's seam unless the profile has no straight side.
    let long = (0..n).fold(0, |best, i| if dist(v[i], v[(i + 1) % n]) > dist(v[best], v[(best + 1) % n]) { i } else { best });
    let walk: Vec<Point2> = (0..=n).map(|i| v[(long + 1 + i) % n]).collect();
    let (mut spans, mut runs, mut arcs, mut worst) = (Vec::new(), Vec::new(), 0usize, 0.0f64);
    let mut s = 0;
    while s < n {
        let mut best: Option<(usize, Fit)> = None;
        let mut e = s + MIN_CHORDS;
        while e <= n && e - s <= MAX_RUN {
            match fit(&walk[s..=e], bound) {
                Some(f) => {
                    best = Some((e, f));
                    e += 1;
                }
                None => break,
            }
        }
        match best {
            Some((e, f)) => {
                spans.push(Span::Arc { c: f.c, r: f.r, a: walk[s], b: walk[e], ccw: f.ccw, sweep: f.sweep });
                runs.push(Some((s, e)));
                arcs += 1;
                worst = worst.max(f.sag);
                s = e;
            }
            None => {
                spans.push(Span::Line { a: walk[s], b: walk[s + 1] });
                runs.push(None);
                s += 1;
            }
        }
    }
    if arcs == 0 {
        return None;
    }
    snap_tangents(&mut spans, &runs, &walk, bound);
    let chain = ArcChain::new(spans).ok()?;
    Some(ModelArcs { chain, arcs, chord, worst_sag: worst })
}

/// Cut the model profile `m` as true arcs (`wire_contour_arcs`), the
/// path's note saying how its arcs were read.
///
/// # Errors
/// Every refusal of the analytic wire contour, by name.
pub fn wire_contour_model_arcs(m: &ModelArcs, p: &WireParams) -> Result<Toolpath, GenError> {
    let mut path = wire_contour_arcs(&m.chain, p)?;
    path.notes.push(format!(
        "{MODEL_ARCS_NOTE}: {} arc(s), each within {:.4} mm of its vertices and bowing at most {:.4} mm off its chords (model chord tolerance {:.4} mm)",
        m.arcs, FIT_TOL, m.worst_sag, m.chord
    ));
    Ok(path)
}

/// The vertical, untabbed cut of a meshed model's one closed profile
/// with its arcs read back (`chord`: the model's chord tolerance); any
/// other cut, and a profile with no arc, is the polygon cut as before
/// (`wire_contour_tapered`).
///
/// # Errors
/// Every refusal of the cut taken, by name.
pub fn wire_contour_facets(loops: &[Loop], chains: &[Vec<Point2>], p: &WireParams, t: &WireTaper, chord: Option<f64>) -> Result<Toolpath, GenError> {
    let read = chord.filter(|_| t.angle_deg == 0.0 && p.tabs.is_none()).and_then(|c| model_arcs(loops, chains, c));
    match read {
        Some(m) => wire_contour_model_arcs(&m, p),
        None => wire_contour_tapered(loops, chains, p, t),
    }
}

/// `ring` without repeated points and without vertices that lie within
/// [`COLLINEAR`] of the line between their neighbours. The vertex that
/// stands nearest its neighbours' line goes first, again and again, so a
/// split point on a short chord goes before the corner beside it (a
/// long side's end, whose line to the split point it nearly continues).
fn simplify(ring: &[Point2]) -> Vec<Point2> {
    let mut pts: Vec<Point2> = Vec::with_capacity(ring.len());
    for &p in ring {
        if !pts.last().is_some_and(|&q| dist(p, q) <= COLLINEAR) {
            pts.push(p);
        }
    }
    while pts.len() >= 2 && pts.first().zip(pts.last()).is_some_and(|(&a, &b)| dist(a, b) <= COLLINEAR) {
        pts.pop();
    }
    let n = pts.len();
    if n < 4 {
        return pts;
    }
    let mut prev: Vec<usize> = (0..n).map(|i| (i + n - 1) % n).collect();
    let mut next: Vec<usize> = (0..n).map(|i| (i + 1) % n).collect();
    let mut ver = vec![0u32; n];
    let mut alive = vec![true; n];
    let dev = |a: Point2, m: Point2, b: Point2| {
        let ab = sub(b, a);
        let l = norm(ab);
        let t = if l > 0.0 { dot(sub(m, a), ab) / (l * l) } else { 0.0 };
        if l > 0.0 && t > 0.0 && t < 1.0 { (cross(ab, sub(m, a)) / l).abs() } else { f64::INFINITY }
    };
    let mut heap: BinaryHeap<Reverse<(u64, usize, u32)>> = BinaryHeap::with_capacity(n);
    for i in 0..n {
        let d = dev(pts[prev[i]], pts[i], pts[next[i]]);
        if d <= COLLINEAR {
            heap.push(Reverse((d.to_bits(), i, 0)));
        }
    }
    let mut left = n;
    while let Some(Reverse((_, i, v))) = heap.pop() {
        if !alive[i] || ver[i] != v {
            continue;
        }
        if left <= 3 {
            break;
        }
        alive[i] = false;
        left -= 1;
        let (a, b) = (prev[i], next[i]);
        next[a] = b;
        prev[b] = a;
        for j in [a, b] {
            ver[j] = ver[j].wrapping_add(1);
            let d = dev(pts[prev[j]], pts[j], pts[next[j]]);
            if d <= COLLINEAR {
                heap.push(Reverse((d.to_bits(), j, ver[j])));
            }
        }
    }
    let Some(first) = alive.iter().position(|&k| k) else { return Vec::new() };
    let mut out = Vec::with_capacity(left);
    let mut i = first;
    loop {
        out.push(pts[i]);
        i = next[i];
        if i == first || out.len() > n {
            break;
        }
    }
    out
}

/// The circle through `a`, `m` and `b`: centre and sense of a → m → b.
fn circumcircle(a: Point2, m: Point2, b: Point2) -> Option<(Point2, bool)> {
    let (u, v) = (sub(m, a), sub(b, a));
    let d = 2.0 * cross(u, v);
    if !(d.is_finite() && d.abs() > 1e-18) {
        return None;
    }
    let (uu, vv) = (dot(u, u), dot(v, v));
    let c = Point2::new(a.x + (v.y * uu - u.y * vv) / d, a.y + (u.x * vv - v.x * uu) / d);
    (c.x.is_finite() && c.y.is_finite()).then_some((c, d > 0.0))
}

/// Judge every vertex and chord of `pts` (walked in order; `closed`
/// adds the chord back to the first) against the circle about `c` of
/// radius `r` turning `ccw`: the total sweep and the widest sagitta, or
/// `None` when a vertex is off the circle, a chord turns the wrong way
/// or bows past `bound`.
pub(super) fn judge(pts: &[Point2], c: Point2, r: f64, ccw: bool, bound: f64, closed: bool) -> Option<(f64, f64)> {
    if !(r.is_finite() && (MIN_RADIUS..=MAX_RADIUS).contains(&r)) {
        return None;
    }
    if pts.iter().any(|&p| (dist(p, c) - r).abs() > FIT_TOL) {
        return None;
    }
    let (mut sweep, mut sag) = (0.0f64, 0.0f64);
    let n = pts.len();
    let chords = if closed { n } else { n - 1 };
    for i in 0..chords {
        let (p, q) = (pts[i], pts[(i + 1) % n]);
        let (u, w) = (sub(p, c), sub(q, c));
        let t = cross(u, w).atan2(dot(u, w));
        let t = if ccw { t } else { -t };
        if !(t > 0.0 && t < PI) {
            return None;
        }
        let half = 0.5 * dist(p, q);
        if (2.0 * half).is_nan() || 2.0 * half < MIN_CHORD {
            return None;
        }
        let s = r - (r * r - half * half).max(0.0).sqrt();
        if s.is_nan() || s > bound {
            return None;
        }
        sweep += t;
        sag = sag.max(s);
    }
    Some((sweep, sag))
}

/// The arc of the open run `pts`, or `None` (module head).
fn fit(pts: &[Point2], bound: f64) -> Option<Fit> {
    let (a, b) = (*pts.first()?, *pts.last()?);
    let m = pts[pts.len() / 2];
    // Near-straight: every vertex within the fit tolerance of the end chord.
    let ab = sub(b, a);
    let l = norm(ab);
    if l > 0.0 && pts.iter().all(|&p| (cross(ab, sub(p, a)) / l).abs() <= FIT_TOL) {
        return None;
    }
    let (c, ccw) = circumcircle(a, m, b)?;
    let r = dist(a, c);
    let (sweep, sag) = judge(pts, c, r, ccw, bound, false)?;
    (sweep < TAU - 1e-9).then_some(Fit { c, r, ccw, sweep, sag })
}

/// The whole ring as one circle, or `None`.
fn full_circle(v: &[Point2], bound: f64) -> Option<Fit> {
    let n = v.len();
    let (c, ccw) = circumcircle(v[0], v[n / 3], v[2 * n / 3])?;
    let r = dist(v[0], c);
    let (sweep, sag) = judge(v, c, r, ccw, bound, true)?;
    ((sweep - TAU).abs() < 1e-6).then_some(Fit { c, r, ccw, sweep: TAU, sag })
}
