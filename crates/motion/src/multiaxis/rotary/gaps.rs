//! Rotary Parallel's reach reading (#2970 review): where the ball
//! centres of a planned path stand ([`Cover`]), and which places of the
//! turned surface no pass reaches ([`unreached`]). The two halves read
//! the cover to keep each place cut once and to lose no cut the single
//! laid frame made; the path notes name the places no half reaches.
//!
//! Provenance: our own design; ray-triangle intersection (Moller and
//! Trumbore, 1997) and a uniform grid hash are textbook geometry.

use std::collections::HashMap;

use crate::cl::{MoveKind, Toolpath};
use crate::finishing3d::closest_pt_tri;
use crate::geometry::{Point3, Triangle};

use super::halves::chords;

/// Samples per chord are capped: a corrupt chord of huge length costs a
/// bounded walk, never a hang.
const MAX_SAMPLES: usize = 4096;
/// The station x angle grid of the reach reading: fixed, so its cost
/// does not grow with the path.
const MAX_STATIONS: usize = 48;
const ANGLES: usize = 72;

/// The ball centres (tip + `r` up the laid frame's z) of the cutting
/// moves of a path, sampled every half `d` along each chord, in a grid
/// hash of cell `d`. `near` answers "does a centre stand within `d`".
pub(super) struct Cover {
    d: f64,
    map: HashMap<(i64, i64, i64), Vec<Point3>>,
}

impl Cover {
    pub(super) fn of(base: &Toolpath, r: f64, d: f64) -> Cover {
        let d = if d.is_finite() && d > 1e-6 { d } else { 1.0 };
        let mut cover = Cover { d, map: HashMap::new() };
        for m in base.moves.iter().filter(|m| m.kind == MoveKind::Feed) {
            for (a, b) in chords(m) {
                let len = (b - a).dot(b - a).sqrt();
                let n = if len.is_finite() { ((2.0 * len / d).ceil() as usize).clamp(1, MAX_SAMPLES) } else { 1 };
                for i in 0..=n {
                    let t = i as f64 / n as f64;
                    let c = Point3::new(a.x + (b.x - a.x) * t, a.y + (b.y - a.y) * t, a.z + (b.z - a.z) * t + r);
                    if c.x.is_finite() && c.y.is_finite() && c.z.is_finite() {
                        cover.map.entry(cover.key(c)).or_default().push(c);
                    }
                }
            }
        }
        cover
    }

    fn key(&self, c: Point3) -> (i64, i64, i64) {
        let k = |v: f64| (v / self.d).floor().clamp(-1e12, 1e12) as i64;
        (k(c.x), k(c.y), k(c.z))
    }

    /// Whether a sampled centre stands within `d` of the centre `c`.
    pub(super) fn near(&self, c: Point3) -> bool {
        let (i, j, k) = self.key(c);
        let d2 = self.d * self.d;
        (-1..=1).any(|di| {
            (-1..=1).any(|dj| {
                (-1..=1).any(|dk| {
                    self.map
                        .get(&(i + di, j + dj, k + dk))
                        .is_some_and(|v| v.iter().any(|q| (*q - c).dot(*q - c) <= d2))
                })
            })
        })
    }
}

/// The places of the turned surface that no pass of `base` reaches,
/// as notes. On a fixed grid of stations (inside the model's ends by
/// `r`) and angles about the axis, a ray from the axis finds the
/// outermost surface; a place under the Bottom (`bottom - tol`, laid
/// frame) is left as the operation asks. The ball would stand `r` out
/// along that facet's outward normal; a place where such a ball cuts
/// into the model (a corner it cannot reach) is no pass gap. A place
/// with no centre of `cover` within its spacing stays uncut, and the
/// note names its arc (degrees about the axis from the laid frame's top
/// toward its +y side) and its stations (mm along the axis).
pub(super) fn unreached(local: &[Triangle], cover: &Cover, r: f64, bottom: f64, tol: f64) -> Vec<String> {
    let (x0, x1) = local
        .iter()
        .flat_map(|t| [t.a.x, t.b.x, t.c.x])
        .filter(|x| x.is_finite())
        .fold((f64::INFINITY, f64::NEG_INFINITY), |(l, h), x| (l.min(x), h.max(x)));
    let len = x1 - x0 - 2.0 * r;
    if !(len.is_finite() && len > 0.0) {
        return Vec::new();
    }
    let ns = ((len / cover.d).ceil() as usize).clamp(1, MAX_STATIONS);
    let mut open = vec![vec![false; ANGLES]; ns];
    for (i, row) in open.iter_mut().enumerate() {
        let s = x0 + r + (i as f64 + 0.5) * len / ns as f64;
        let here: Vec<&Triangle> = local.iter().filter(|t| spans(t, s, 0.0)).collect();
        let around: Vec<&Triangle> = local.iter().filter(|t| spans(t, s, r)).collect();
        for (j, cell) in row.iter_mut().enumerate() {
            let th = (j as f64 + 0.5) * std::f64::consts::TAU / ANGLES as f64;
            let dir = Point3::new(0.0, th.sin(), th.cos());
            let Some((p, n)) = outermost(&here, Point3::new(s, 0.0, 0.0), dir) else { continue };
            if p.z < bottom - tol {
                continue;
            }
            let e = Point3::new(p.x + r * n.x, p.y + r * n.y, p.z + r * n.z);
            let lim = (r - tol).max(0.0);
            let fits = around.iter().all(|t| {
                let q = closest_pt_tri(e, &[t.a, t.b, t.c]);
                (q - e).dot(q - e) >= lim * lim - 1e-9
            });
            *cell = fits && !cover.near(e);
        }
    }
    arcs(&open, x0 + r, len / ns as f64)
}

fn spans(t: &Triangle, s: f64, pad: f64) -> bool {
    let (lo, hi) = (t.a.x.min(t.b.x).min(t.c.x), t.a.x.max(t.b.x).max(t.c.x));
    lo - pad <= s && s <= hi + pad
}

/// The farthest hit of the ray `o + t dir` (t > 0) on `facets`, with the
/// hit facet's unit normal turned to face along the ray (outward).
fn outermost(facets: &[&Triangle], o: Point3, dir: Point3) -> Option<(Point3, Point3)> {
    let mut best: Option<(f64, Point3)> = None;
    for t in facets {
        let (e1, e2) = (t.b - t.a, t.c - t.a);
        let pv = dir.cross(e2);
        let det = e1.dot(pv);
        if !det.is_finite() || det.abs() < 1e-12 {
            continue;
        }
        let tv = o - t.a;
        let u = tv.dot(pv) / det;
        let qv = tv.cross(e1);
        let v = dir.dot(qv) / det;
        let d = e2.dot(qv) / det;
        if (0.0..=1.0).contains(&u) && v >= 0.0 && u + v <= 1.0 && d > 1e-9 && d.is_finite() && best.is_none_or(|(b, _)| d > b) {
            let n = e1.cross(e2);
            let l = n.dot(n).sqrt();
            if l > 1e-12 {
                let n = Point3::new(n.x / l, n.y / l, n.z / l);
                best = Some((d, if n.dot(dir) < 0.0 { Point3::new(-n.x, -n.y, -n.z) } else { n }));
            }
        }
    }
    best.map(|(d, n)| (Point3::new(o.x + d * dir.x, o.y + d * dir.y, o.z + d * dir.z), n))
}

/// The open cells grouped into arcs of neighbouring angles (round the
/// turn), each with the stations it spans.
fn arcs(open: &[Vec<bool>], s0: f64, ds: f64) -> Vec<String> {
    let step = 360.0 / ANGLES as f64;
    let any = |j: usize| open.iter().any(|row| row[j % ANGLES]);
    if !(0..ANGLES).any(any) {
        return Vec::new();
    }
    // Start the walk at a closed angle, so an arc through 0 stays whole.
    let start = (0..ANGLES).find(|j| !any(*j)).unwrap_or(0);
    let mut out = Vec::new();
    let mut j = 0usize;
    while j < ANGLES {
        if !any(start + j) {
            j += 1;
            continue;
        }
        let first = j;
        while j < ANGLES && any(start + j) {
            j += 1;
        }
        let rows: Vec<usize> = (0..open.len()).filter(|i| (first..j).any(|k| open[*i][(start + k) % ANGLES])).collect();
        let (lo, hi) = (rows.first().copied().unwrap_or(0), rows.last().copied().unwrap_or(0));
        let a0 = ((start + first) % ANGLES) as f64 * step;
        let a1 = a0 + (j - first) as f64 * step;
        out.push(format!(
            "Rotary Parallel: no pass reaches the surface over {a0:.0}..{a1:.0} deg about the axis (from the top of the laid bar toward its +y side) at stations {:.3}..{:.3} mm along it - that surface stays uncut",
            s0 + lo as f64 * ds,
            s0 + (hi + 1) as f64 * ds
        ));
    }
    out
}
