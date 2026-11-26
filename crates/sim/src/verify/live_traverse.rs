//! #3077: the first traverse of a live-tool cross drill on a lathe or a mill-turn, from the turret's index position at the top of the X and Z travel, must pass outside the work and the steel - our own reading of the RS-274 / ISO 6983 lathe rapid (G00: each axis at its own rate, so the path stays in the box of its two ends) and of the shop rule "never traverse through uncut stock or steel"; no vendor material consulted.
//!
//! The lathe post positions C first (with the tool at the index
//! position), then sends X and Z in one G00 to the first point: the tip
//! at radius `r` about the spindle, at height `z`. Each axis runs at its
//! own rate, so the tip can pass any point of the box between the index
//! position (X and Z at the top of their travel) and the first point:
//! every radius from `r` out, at every height from `z` up, in the
//! station's half-plane. The tool lies along the station's radius, its
//! tip inward: at a distance `s` behind the tip its body is the band of
//! the tool stack there (flute, shank, holder; `ToolGeom`). Over every
//! tip position the swept solid is, in the part frame (`u` the first
//! point's radial direction, `v` across it, `s = u - r`):
//! `u >= r`, `|v| <= W(s)`, `z >= z - W(s)`, with `W(s)` the widest band
//! of the stack within `s` of the tip. Past the stack's end the widest
//! band runs on (the turret is not modelled; reading it wide is stricter).
//!
//! C stands at its angle before X and Z move, so the work and the steel
//! that turns with it (a chuck) stand in the part frame as modelled: the
//! swept solid is read against them exactly (each facet clipped by each
//! band's planes). The fixed steel (a tailstock, a table) stands in the
//! machine frame at an angle the part frame does not state: it is read
//! revolved about the spindle. A face that only touches the swept solid
//! (within `tol`) passes, as in the other traverse laws; the work's
//! model excess (a coarse model, #1053) tightens that tolerance while a
//! positive one is left, and past that the law reads untightened.
//! NOT covered: a program that starts with the turret low near the work
//! (after a manual jog, for example) - decision #3077.

use crate::fixture::FixtureBox;
use crate::geometry::{Point3, Triangle};
use crate::tools::ToolGeom;

/// The first point of a live-tool cross drill as the law reads it: the
/// tip `start` in the part frame, the spindle line through `spindle`
/// (X, Y) along Z, and the tool stack (`ToolGeom`: flute, shank, holder).
#[derive(Clone, Copy, Debug)]
pub struct LiveStart {
    pub start: Point3,
    pub spindle: [f64; 2],
    pub tool: ToolGeom,
}

/// What stands in the way of the first traverse. `along_mm` is the
/// farthest the solid reaches out from the spindle along the station's
/// line inside the swept solid, `z_mm` the height of that point.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum LiveHit {
    /// The stock or a part (`index` into the work list, then the work
    /// boxes) meets the swept solid.
    Work { index: usize, along_mm: f64, z_mm: f64 },
    /// Tagged steel: `turning` (a chuck, read at its angle) or fixed (a
    /// tailstock, a table, read revolved), `index` into its list.
    Steel { index: usize, turning: bool, along_mm: f64, z_mm: f64 },
}

/// The start, the spindle, the tool stack or the tolerances of a
/// live-tool first traverse are not finite (or negative), or the start
/// stands on the spindle line: the traverse cannot be judged.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LiveUnjudged;

/// What the live-tool first traverse is judged against: `work`, the
/// stock and the parts (each a facet list; `work_boxes` a stock that has
/// only its box), and the tagged steel (`fixture::workholding_for`),
/// `turning` (a chuck) and `fixed` (a tailstock, a table).
#[derive(Clone, Copy, Debug, Default)]
pub struct LiveSolids<'a> {
    pub work: &'a [&'a [Triangle]],
    pub work_boxes: &'a [FixtureBox],
    pub turning: &'a [FixtureBox],
    pub fixed: &'a [FixtureBox],
}

/// One band of the swept solid: `s` from `s0` to `s1` behind the first
/// point's radius, `w` wide on each side and under the tip's height.
#[derive(Clone, Copy, Debug)]
struct Band {
    s0: f64,
    s1: f64,
    w: f64,
}

/// The swept solid in the part frame: the union of its bands.
struct Swept {
    o: [f64; 2],
    u: [f64; 2],
    r: f64,
    z: f64,
    bands: Vec<Band>,
    /// Added to every band's width and taken off the first band's inner
    /// radius: less the touch tolerance (tightened by the work's excess).
    grow: f64,
}

/// The widest-so-far bands of the tool stack from the tip; `None` when a
/// field cannot be read.
fn bands_of(g: &ToolGeom) -> Option<Vec<Band>> {
    let stack = [(g.flute_len, g.radius), (g.shank_len, g.shank_radius), (g.holder_len, g.holder_radius)];
    if stack.iter().any(|(l, w)| !(l.is_finite() && w.is_finite()) || *l < 0.0 || *w < 0.0) {
        return None;
    }
    let (mut s, mut w, mut out) = (0.0_f64, 0.0_f64, Vec::new());
    for (len, half) in stack {
        if len <= 0.0 {
            continue;
        }
        w = w.max(half);
        out.push(Band { s0: s, s1: s + len, w });
        s += len;
    }
    // Past the stack: the widest band runs on (the turret is unmodelled).
    out.push(Band { s0: s, s1: f64::INFINITY, w: w.max(g.radius) });
    Some(out)
}

impl Swept {
    fn of(s: &LiveStart, tol: f64, excess: f64) -> Option<Swept> {
        let LiveStart { start, spindle, tool } = *s;
        let finite = [start.x, start.y, start.z, spindle[0], spindle[1], tol, excess].iter().all(|v| v.is_finite());
        if !finite || tol < 0.0 || excess < 0.0 {
            return None;
        }
        let (dx, dy) = (start.x - spindle[0], start.y - spindle[1]);
        let r = dx.hypot(dy);
        if r <= 1e-9 {
            return None;
        }
        let bands = bands_of(&tool)?;
        // #1053: the excess tightens the tolerance while a positive one is
        // left; past that the law reads untightened (the program gate
        // refuses the coarse model by name).
        let t = if tol > excess { tol - excess } else { tol };
        Some(Swept { o: spindle, u: [dx / r, dy / r], r, z: start.z, bands, grow: -t })
    }

    /// `(along, across)` of `p` about the spindle line.
    fn frame(&self, p: Point3) -> (f64, f64) {
        let (dx, dy) = (p.x - self.o[0], p.y - self.o[1]);
        (dx * self.u[0] + dy * self.u[1], -dx * self.u[1] + dy * self.u[0])
    }

    /// The half-spaces of band `b`, each as `plane(k, p) >= 0`.
    fn plane(&self, b: &Band, first: bool, k: usize, p: Point3) -> f64 {
        let (along, across) = self.frame(p);
        let w = b.w + self.grow;
        match k {
            0 => along - (self.r + b.s0 - if first { self.grow } else { 0.0 }),
            1 => self.r + b.s1 - along,
            2 => w - across,
            3 => w + across,
            _ => p.z - (self.z - w),
        }
    }

    /// The farthest-out point (along, z) of the part of `t` inside the
    /// swept solid, or `None` when no part of it is there.
    fn meets(&self, t: &Triangle) -> Option<(f64, f64)> {
        let pts = [t.a, t.b, t.c];
        if pts.iter().any(|p| !(p.x.is_finite() && p.y.is_finite() && p.z.is_finite())) {
            // A facet the law cannot read is never passed.
            return Some((f64::INFINITY, f64::NAN));
        }
        let mut best: Option<(f64, f64)> = None;
        for (i, b) in self.bands.iter().enumerate() {
            let planes = if b.s1.is_finite() { 5 } else { 4 };
            let mut poly: Vec<Point3> = pts.to_vec();
            for k in (0..5).filter(|&k| k != 1 || planes == 5) {
                poly = clip(&poly, |p| self.plane(b, i == 0, k, p));
                if poly.is_empty() {
                    break;
                }
            }
            for p in &poly {
                let along = self.frame(*p).0;
                if best.is_none_or(|(a, _)| along > a) {
                    best = Some((along, p.z));
                }
            }
        }
        best
    }

    fn meets_all(&self, facets: &[Triangle]) -> Option<(f64, f64)> {
        facets.iter().filter_map(|t| self.meets(t)).reduce(|a, b| if b.0 > a.0 { b } else { a })
    }

    /// A box read revolved about the spindle: it meets a band when its
    /// radii (nearest and farthest X/Y point) overlap the band's and its
    /// top reaches the band's floor.
    fn reaches_revolved(&self, b: &FixtureBox) -> Option<(f64, f64)> {
        if !box_finite(b) {
            return Some((f64::INFINITY, f64::NAN));
        }
        let corners = [b.min.x, b.max.x].map(|x| [b.min.y, b.max.y].map(|y| (x - self.o[0]).hypot(y - self.o[1])));
        let outer = corners.iter().flatten().fold(f64::NEG_INFINITY, |a, &c| a.max(c));
        let near = |lo: f64, hi: f64, c: f64| c.clamp(lo, hi) - c;
        let inner = near(b.min.x, b.max.x, self.o[0]).hypot(near(b.min.y, b.max.y, self.o[1]));
        for (i, band) in self.bands.iter().enumerate() {
            let lo = self.r + band.s0 - if i == 0 { self.grow } else { 0.0 };
            let floor = self.z - (band.w + self.grow);
            if outer > lo && inner < self.r + band.s1 && b.max.z > floor {
                return Some((outer, b.max.z));
            }
        }
        None
    }
}

/// One Sutherland-Hodgman step: the part of `poly` where `f >= 0`.
fn clip(poly: &[Point3], f: impl Fn(Point3) -> f64) -> Vec<Point3> {
    let mut out = Vec::with_capacity(poly.len() + 1);
    for i in 0..poly.len() {
        let (p, q) = (poly[i], poly[(i + 1) % poly.len()]);
        let (fp, fq) = (f(p), f(q));
        if fp >= 0.0 {
            out.push(p);
        }
        if (fp >= 0.0) != (fq >= 0.0) {
            let s = (fp / (fp - fq)).clamp(0.0, 1.0);
            out.push(Point3::new(p.x + s * (q.x - p.x), p.y + s * (q.y - p.y), p.z + s * (q.z - p.z)));
        }
    }
    out
}

fn box_finite(b: &FixtureBox) -> bool {
    [b.min.x, b.min.y, b.min.z, b.max.x, b.max.y, b.max.z].iter().all(|v| v.is_finite())
}

/// The twelve facets of `b`.
fn box_facets(b: &FixtureBox) -> Vec<Triangle> {
    let c = |i: usize| {
        Point3::new(if i & 1 == 0 { b.min.x } else { b.max.x }, if i & 2 == 0 { b.min.y } else { b.max.y }, if i & 4 == 0 { b.min.z } else { b.max.z })
    };
    let quads = [[0, 1, 3, 2], [4, 6, 7, 5], [0, 4, 5, 1], [2, 3, 7, 6], [0, 2, 6, 4], [1, 5, 7, 3]];
    quads.iter().flat_map(|q| [Triangle::new(c(q[0]), c(q[1]), c(q[2])), Triangle::new(c(q[0]), c(q[2]), c(q[3]))]).collect()
}

/// The live-tool first-traverse law (#3077). `Ok(Some(hit))` names the
/// first solid in the way: the work or the turning steel that meets the
/// swept solid, or the fixed steel that reaches it read revolved. A face
/// within `tol` of it passes; on the work `tol` less `excess` while that
/// is positive (#1053). `Ok(None)` when the
/// traverse is clear.
///
/// # Errors
/// [`LiveUnjudged`] when the start cannot be read: the caller refuses by
/// name.
pub fn live_first_traverse(s: &LiveStart, solids: &LiveSolids<'_>, tol: f64, excess: f64) -> Result<Option<LiveHit>, LiveUnjudged> {
    let work = Swept::of(s, tol, excess).ok_or(LiveUnjudged)?;
    let steel = Swept::of(s, tol, 0.0).ok_or(LiveUnjudged)?;
    for (index, facets) in solids.work.iter().enumerate() {
        if let Some((along_mm, z_mm)) = work.meets_all(facets) {
            return Ok(Some(LiveHit::Work { index, along_mm, z_mm }));
        }
    }
    let boxed = |sw: &Swept, b: &FixtureBox| if box_finite(b) { sw.meets_all(&box_facets(b)) } else { Some((f64::INFINITY, f64::NAN)) };
    for (i, b) in solids.work_boxes.iter().enumerate() {
        if let Some((along_mm, z_mm)) = boxed(&work, b) {
            return Ok(Some(LiveHit::Work { index: solids.work.len() + i, along_mm, z_mm }));
        }
    }
    for (index, b) in solids.turning.iter().enumerate() {
        if let Some((along_mm, z_mm)) = boxed(&steel, b) {
            return Ok(Some(LiveHit::Steel { index, turning: true, along_mm, z_mm }));
        }
    }
    for (index, b) in solids.fixed.iter().enumerate() {
        if let Some((along_mm, z_mm)) = steel.reaches_revolved(b) {
            return Ok(Some(LiveHit::Steel { index, turning: false, along_mm, z_mm }));
        }
    }
    Ok(None)
}

#[cfg(test)]
#[path = "live_traverse_tests.rs"]
mod tests;
