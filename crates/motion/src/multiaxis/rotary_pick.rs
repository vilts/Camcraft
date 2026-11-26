//! Rotary Parallel over picked faces (#2848). The raster is planned on
//! the whole model, as it always was - the axis fit, the tangency law
//! and the gouge laws read every facet - and the planned cut is then
//! CLIPPED to the picked faces' radial shadow: a cut point is kept
//! where the line from the axis out through the ball's CENTER meets a
//! picked facet. That is the face the radially posed tool stands over.
//! Nothing is planned again: a kept point is a point the whole-model
//! raster already certified, and the radial relink
//! ([`super::rotary_links`]) builds every link between the kept
//! pieces, as it does for the whole raster.
//!
//! [`faces_outward`] and [`groups`] are the two questions a caller
//! asks before the clip, so that a pick the radial tool cannot reach,
//! or picks that do not touch, are refused by name and never dropped.
//!
//! Provenance: containment of a surface toolpath to selected faces is
//! the vocabulary of any CAM manual's finishing chapter; the ray /
//! triangle test is the Moller-Trumbore construction (public
//! literature), the grouping a union-find over welded vertices. Our
//! own design; nothing of any vendor's consulted.

use std::collections::HashMap;

use crate::cl::{Move, MoveKind, Toolpath};
use crate::geometry::{Point3, Triangle};
use crate::machining2d::ARC_FIT_TOL;

use super::axis::Axis;

/// The barycentric slack of the ray test: a ray through an edge two
/// picked facets share is over both, never between them.
const EDGE_EPS: f64 = 1e-9;
/// The longest step between two shadow samples on one cut chord, mm.
const STEP_MAX: f64 = 0.25;
/// Bisection rounds on a shadow crossing: 2^-40 of one step.
const BISECT: usize = 40;
/// The sine below which a facet is edge-on to its radial (1 degree).
const EDGE_ON: f64 = 0.0175;
/// Two vertices closer than this are one, mm (the weld of [`groups`]).
const WELD: f64 = 1e-6;

/// The picked facets in the laid frame (local x the axis line), binned
/// by station so a sample reads only the facets at its own station.
pub(super) struct Shadow {
    tris: Vec<Triangle>,
    lo: f64,
    hi: f64,
    width: f64,
    bins: Vec<Vec<u32>>,
}

impl Shadow {
    /// None when `local` holds no finite facet.
    pub(super) fn of(local: &[Triangle]) -> Option<Self> {
        let tris: Vec<Triangle> =
            local.iter().filter(|t| t.a.is_finite() && t.b.is_finite() && t.c.is_finite()).copied().collect();
        let (lo, hi) = tris
            .iter()
            .flat_map(|t| [t.a.x, t.b.x, t.c.x])
            .fold((f64::INFINITY, f64::NEG_INFINITY), |(l, h), x| (l.min(x), h.max(x)));
        if tris.is_empty() || lo > hi {
            return None;
        }
        let n = ((tris.len() as f64).sqrt().ceil() as usize).clamp(1, 4096);
        let width = ((hi - lo) / n as f64).max(f64::MIN_POSITIVE);
        let mut bins = vec![Vec::new(); n];
        let bin = |x: f64| (((x - lo) / width).floor().max(0.0) as usize).min(n - 1);
        for (i, t) in tris.iter().enumerate() {
            let (a, b) = (t.a.x.min(t.b.x).min(t.c.x), t.a.x.max(t.b.x).max(t.c.x));
            for slot in bins.iter_mut().take(bin(b) + 1).skip(bin(a)) {
                slot.push(i as u32);
            }
        }
        Some(Shadow { tris, lo, hi, width, bins })
    }

    /// Whether the radial through the ball center `c` meets a picked
    /// facet. A center on the axis line has no radial: not held.
    fn holds(&self, c: Point3) -> bool {
        let rho = c.y.hypot(c.z);
        if !(rho.is_finite() && rho > 1e-9 && c.x >= self.lo - EDGE_EPS && c.x <= self.hi + EDGE_EPS) {
            return false;
        }
        let o = Point3::new(c.x, 0.0, 0.0);
        let d = Point3::new(0.0, c.y / rho, c.z / rho);
        let last = self.bins.len().saturating_sub(1);
        let k = (((c.x - self.lo) / self.width).floor().max(0.0) as usize).min(last);
        // A station on a bin's edge reads both sides of it.
        let Some(near) = self.bins.get(k.saturating_sub(1)..=(k + 1).min(last)) else {
            return false;
        };
        near.iter().flatten().any(|&i| self.tris.get(i as usize).is_some_and(|t| ray_meets(o, d, t)))
    }
}

/// Whether the ray `o + t d` (t > 0) meets the facet `t` (the
/// Moller-Trumbore test). A ray in the facet's own plane meets nothing.
fn ray_meets(o: Point3, d: Point3, t: &Triangle) -> bool {
    let (e1, e2) = (t.b - t.a, t.c - t.a);
    let h = d.cross(e2);
    let det = e1.dot(h);
    let scale = e1.dot(e1).sqrt() * e2.dot(e2).sqrt();
    if !(det.is_finite() && scale > 0.0 && det.abs() > 1e-12 * scale) {
        return false;
    }
    let f = 1.0 / det;
    let s = o - t.a;
    let u = f * s.dot(h);
    if !(-EDGE_EPS..=1.0 + EDGE_EPS).contains(&u) {
        return false;
    }
    let q = s.cross(e1);
    let v = f * d.dot(q);
    if v < -EDGE_EPS || u + v > 1.0 + EDGE_EPS {
        return false;
    }
    f * e2.dot(q) > 0.0
}

fn lerp(a: Point3, b: Point3, t: f64) -> Point3 {
    if t <= 0.0 {
        a
    } else if t >= 1.0 {
        b
    } else {
        Point3::new(a.x + (b.x - a.x) * t, a.y + (b.y - a.y) * t, a.z + (b.z - a.z) * t)
    }
}

/// The parts of the cut chord `a..b` (tips; the ball center stands `r`
/// up local z) whose centers the shadow holds, as parameter spans in
/// order. Every span end is a point the shadow holds.
fn spans(a: Point3, b: Point3, r: f64, sh: &Shadow) -> Vec<(f64, f64)> {
    let held = |t: f64| {
        let p = lerp(a, b, t);
        sh.holds(Point3::new(p.x, p.y, p.z + r))
    };
    let d = b - a;
    let len = d.dot(d).sqrt();
    if !len.is_finite() {
        return Vec::new();
    }
    let n = ((len / STEP_MAX).ceil() as usize).clamp(1, 1 << 20);
    // The crossing between a held and an unheld sample, on the held side.
    let cross = |mut inside: f64, mut outside: f64| {
        for _ in 0..BISECT {
            let mid = 0.5 * (inside + outside);
            if held(mid) {
                inside = mid;
            } else {
                outside = mid;
            }
        }
        inside
    };
    let mut out: Vec<(f64, f64)> = Vec::new();
    let mut prev = (0.0, held(0.0));
    let mut open: Option<f64> = prev.1.then_some(0.0);
    for i in 1..=n {
        let t = i as f64 / n as f64;
        let now = held(t);
        match (prev.1, now) {
            (false, true) => open = Some(cross(t, prev.0)),
            (true, false) => {
                if let Some(s) = open.take() {
                    out.push((s, cross(prev.0, t)));
                }
            }
            _ => {}
        }
        prev = (t, now);
    }
    if let Some(s) = open {
        out.push((s, 1.0));
    }
    out.retain(|(s, e)| e > s);
    out
}

/// `base` (the laid-frame raster, vertical-ball tips) with every
/// cutting move kept to the picked faces' radial shadow. Links
/// (rapids, plunges) pass through: the radial relink rebuilds them
/// from the cut runs. Where the cut leaves the shadow and comes back,
/// a plunge marker stands between the two pieces, so the relink reads
/// two runs and links them over the retract cylinder, never with a
/// straight feed across the stretch that was left
/// ([`super::rotary_ends::clip`]'s own law). An arc the shadow cuts
/// rides as its chords at `ARC_FIT_TOL`.
pub(super) fn clip(base: &Toolpath, r: f64, sh: &Shadow) -> Toolpath {
    let mut out: Vec<Move> = Vec::with_capacity(base.moves.len());
    // Where the last kept cut piece ended; None after a link.
    let mut cut_end: Option<Point3> = None;
    for m in &base.moves {
        if m.kind != MoveKind::Feed {
            out.push(*m);
            cut_end = None;
            continue;
        }
        let chords: Vec<(Point3, Point3)> = match m.arc {
            None => vec![(m.from, m.to)],
            Some(_) => m.segments3(ARC_FIT_TOL),
        };
        let kept: Vec<Vec<(f64, f64)>> = chords.iter().map(|(a, b)| spans(*a, *b, r, sh)).collect();
        let whole = kept.iter().all(|s| matches!(s.as_slice(), [(s0, s1)] if *s0 <= 0.0 && *s1 >= 1.0));
        let pieces: Vec<(Point3, Point3)> = if whole {
            vec![(m.from, m.to)]
        } else {
            chords
                .iter()
                .zip(&kept)
                .flat_map(|((a, b), ss)| ss.iter().map(|(s, e)| (lerp(*a, *b, *s), lerp(*a, *b, *e))))
                .filter(|(p, q)| p != q)
                .collect()
        };
        for (a, b) in pieces {
            if let Some(e) = cut_end
                && e != a
            {
                out.push(Move::new(MoveKind::Plunge, e, a, m.feed));
            }
            out.push(if whole { *m } else { Move { from: a, to: b, arc: None, ..*m } });
            cut_end = Some(b);
        }
    }
    let mut path = Toolpath::new(out);
    path.notes = base.notes.clone();
    path
}

/// Whether a radially posed tool can stand over the face `facets`
/// (world, wound outward): some facet's outward normal leans away from
/// the axis `ax` by more than one degree. An end face is edge-on to
/// every radial, and the wall of a bore looks at the axis: neither is
/// reached.
#[must_use]
pub fn faces_outward(ax: &Axis, facets: &[Triangle]) -> bool {
    facets.iter().any(|t| {
        let n = (t.b - t.a).cross(t.c - t.a);
        let g = Point3::new((t.a.x + t.b.x + t.c.x) / 3.0, (t.a.y + t.b.y + t.c.y) / 3.0, (t.a.z + t.b.z + t.c.z) / 3.0);
        let w = g - ax.origin;
        let along = w.dot(ax.dir);
        let rad = Point3::new(w.x - ax.dir.x * along, w.y - ax.dir.y * along, w.z - ax.dir.z * along);
        let (nl, rl) = (n.dot(n).sqrt(), rad.dot(rad).sqrt());
        nl.is_finite() && rl.is_finite() && nl > 0.0 && rl > 1e-9 && n.dot(rad) > EDGE_ON * nl * rl
    })
}

/// How many separate groups `facets` form: two facets that share a
/// vertex (welded at a thousandth of a micron) are in one group. 0 for
/// no facets.
#[must_use]
pub fn groups(facets: &[Triangle]) -> usize {
    fn root(parent: &mut [usize], mut i: usize) -> usize {
        // Path halving: every index is one of `0..facets.len()`.
        while parent[i] != i {
            parent[i] = parent[parent[i]];
            i = parent[i];
        }
        i
    }
    let key = |p: Point3| ((p.x / WELD).round() as i64, (p.y / WELD).round() as i64, (p.z / WELD).round() as i64);
    let mut parent: Vec<usize> = (0..facets.len()).collect();
    let mut seen: HashMap<(i64, i64, i64), usize> = HashMap::new();
    for (i, t) in facets.iter().enumerate() {
        for p in [t.a, t.b, t.c] {
            let j = *seen.entry(key(p)).or_insert(i);
            let (a, b) = (root(&mut parent, i), root(&mut parent, j));
            parent[a] = b;
        }
    }
    (0..facets.len()).filter(|&i| root(&mut parent, i) == i).count()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(x: f64, y: f64, z: f64) -> Point3 {
        Point3::new(x, y, z)
    }

    /// A flat roof over stations `x0..x1`, 5 mm above the axis line,
    /// 4 mm wide, wound to look up (away from the axis).
    fn roof(x0: f64, x1: f64) -> Vec<Triangle> {
        vec![
            Triangle::new(p(x0, -2.0, 5.0), p(x1, -2.0, 5.0), p(x1, 2.0, 5.0)),
            Triangle::new(p(x0, -2.0, 5.0), p(x1, 2.0, 5.0), p(x0, 2.0, 5.0)),
        ]
    }

    #[test]
    fn the_clip_keeps_the_cut_over_the_picked_facets_and_breaks_it_where_it_left() {
        let sh = Shadow::of(&roof(10.0, 20.0)).expect("a shadow");
        let feed = |a, b| Move::new(MoveKind::Feed, a, b, Some(100.0));
        // One pass over the roof's stations and far past both ends, then
        // a pass on the far side of the bar (the roof's shadow is one-sided).
        let base = Toolpath::new(vec![
            Move::new(MoveKind::Rapid, p(0.0, 0.0, 9.0), p(0.0, 0.0, 9.0), None),
            feed(p(0.0, 0.0, 4.0), p(30.0, 0.0, 4.0)),
            feed(p(30.0, 0.0, 4.0), p(30.0, 0.0, -8.0)),
            feed(p(30.0, 0.0, -8.0), p(0.0, 0.0, -8.0)),
        ]);
        let got = clip(&base, 1.0, &sh);
        let cuts: Vec<&Move> = got.moves.iter().filter(|m| m.kind == MoveKind::Feed).collect();
        assert_eq!(cuts.len(), 1, "{:?}", got.moves);
        assert!((cuts[0].from.x - 10.0).abs() < 1e-6 && (cuts[0].to.x - 20.0).abs() < 1e-6, "{:?}", cuts[0]);
        assert!(sh.holds(p(cuts[0].from.x, 0.0, 5.0)) && sh.holds(p(cuts[0].to.x, 0.0, 5.0)), "a kept end is a held point");
        // Two roofs with a gap: two pieces of one pass, a plunge marker between.
        let mut two = roof(2.0, 8.0);
        two.extend(roof(12.0, 18.0));
        let got = clip(&base, 1.0, &Shadow::of(&two).expect("a shadow"));
        let kinds: Vec<MoveKind> = got.moves.iter().map(|m| m.kind).collect();
        assert_eq!(kinds, [MoveKind::Rapid, MoveKind::Feed, MoveKind::Plunge, MoveKind::Feed]);
        // Every cut over the shadow: the path is the planned one, move for move.
        let all = Shadow::of(&roof(-5.0, 35.0)).expect("a shadow");
        let top = Toolpath::new(base.moves[..2].to_vec());
        assert_eq!(clip(&top, 1.0, &all).moves, top.moves);
        assert!(Shadow::of(&[]).is_none());
    }

    #[test]
    fn a_face_the_radial_tool_cannot_reach_is_told_from_one_it_can() {
        let ax = Axis { origin: Point3::ZERO, dir: p(1.0, 0.0, 0.0) };
        assert!(faces_outward(&ax, &roof(0.0, 10.0)));
        // The same roof wound to look at the axis: a bore's wall.
        let inward: Vec<Triangle> = roof(0.0, 10.0).iter().map(|t| Triangle::new(t.a, t.c, t.b)).collect();
        assert!(!faces_outward(&ax, &inward));
        // An end face: edge-on to every radial.
        let end = vec![Triangle::new(p(10.0, 0.0, 1.0), p(10.0, 3.0, 1.0), p(10.0, 0.0, 4.0))];
        assert!(!faces_outward(&ax, &end));
    }

    #[test]
    fn facets_that_share_no_vertex_are_separate_groups() {
        assert_eq!(groups(&[]), 0);
        assert_eq!(groups(&roof(0.0, 10.0)), 1);
        let mut touching = roof(0.0, 10.0);
        touching.extend(roof(10.0, 20.0));
        assert_eq!(groups(&touching), 1);
        let mut apart = roof(0.0, 10.0);
        apart.extend(roof(12.0, 20.0));
        assert_eq!(groups(&apart), 2);
    }
}
