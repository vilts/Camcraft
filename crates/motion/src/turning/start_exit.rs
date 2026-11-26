//! Turning start and exit points (#3331, design #3324): a user pick read
//! in the lathe's (Z, X) plane - (station, radius) about the fitted axis -
//! and laid onto a kernel's finished path. The first pass starts at the
//! start pick: one rapid brings the nose there from the path's opening
//! point, and the first feed of the path leaves from it. The last pass
//! leaves at the exit pick: the nose feeds from the end of the last cut to
//! it, then rapids home. A pick is a wish: one that the laws below cannot
//! hold is refused by name and never moved.
//!
//! The laws, each in the meridian plane:
//! - the pick stands in air: outside the bar's radius or past its free
//!   end, by the nose radius; a point inside the bar's radius behind the
//!   bar is on the chuck side;
//! - a rapid to or from the pick does not cross that bar;
//! - a feed between the pick and the cut keeps the nose `nose + leave`
//!   off the part, less the tolerance - the finished surface and the stock
//!   to leave stay as the kernel left them;
//! - a feed away from the cut lifts off the surface it leaves, so the nose
//!   is not dragged along a fresh face.
//!
//! Provenance: the lathe cycle start point and tool-change position of
//! standard turning practice (machine-tool handbooks); the envelope and
//! clearance tests are our own, on this crate's silhouette. No vendor
//! material consulted.

use crate::cl::{Move, MoveKind, Toolpath};
use crate::geometry::Triangle;
use crate::machining2d::GenError;
use crate::path_picks::{PathPick, PathPicks, PickRole};

use super::profile::{self, Frame};

/// What the pick law reads of the operation.
#[derive(Clone, Copy, Debug)]
pub struct PickLaw<'a> {
    /// The kind's label: each refusal starts with it.
    pub kind: &'a str,
    /// The insert's nose radius (mm): the moves carry its centre.
    pub nose: f64,
    /// The stock that the kind leaves on the part (mm); 0 for a finish.
    pub leave: f64,
    /// The kind's tolerance (mm): the slack of the clearance test.
    pub tolerance: f64,
}

/// How far along a feed away from the cut the lift is measured (mm).
const LIFT_RUN: f64 = 1.0;
/// The clearance that the nose must gain over [`LIFT_RUN`] (mm), in
/// proportion for a shorter move: a climb of about 6 degrees or more.
const LIFT_MIN: f64 = 0.1;
/// Float slack of the comparisons (mm).
const EPS: f64 = 1e-9;

/// A point of the meridian plane: (station, radius).
type Sr = (f64, f64);

/// `path` with the start and exit picks of `picks` laid on. No pick gives
/// the path back unchanged. `stock` is the setup's stock, read in the
/// part's frame; without it the part is the bar.
pub fn with_picks(
    mut path: Toolpath,
    facets: &[Triangle],
    stock: Option<&[Triangle]>,
    picks: &PathPicks,
    law: &PickLaw,
) -> Result<Toolpath, GenError> {
    if picks.is_empty() {
        return Ok(path);
    }
    let refuse = |why: String| GenError::Invalid(format!("{}: {why}", law.kind));
    if !(law.nose.is_finite() && law.leave.is_finite() && law.tolerance.is_finite()) || law.nose <= 0.0 || law.tolerance <= 0.0 {
        return Err(refuse("a start or exit point needs a nose radius and a tolerance that are positive sizes".into()));
    }
    if !(path.cycles.is_empty() && path.comps.is_empty() && path.threads.is_empty()) {
        return Err(refuse("a start or exit point is not taken on a path with canned cycles or control compensation - clear it".into()));
    }
    let f = Frame::about(facets)?;
    let part = Part::of(facets, &f, law.tolerance)?;
    let bar = Bar::of(facets, stock, &f, law.nose)?;
    let keep = law.nose + law.leave.max(0.0) - law.tolerance;

    if let Some(pick) = picks.first(PickRole::Start) {
        let what = "start point";
        let p = bar.in_air(&f, pick, what).map_err(refuse)?;
        let (Some(first), Some(open)) = (path.moves.iter().position(|m| m.kind == MoveKind::Feed), path.moves.first().copied()) else {
            return Err(refuse(format!("the path has no pass to start at the {what} - clear it")));
        };
        let cut = path.moves[first];
        // An arc keeps its own start point: the pick feeds to it in a line.
        let end = if cut.arc.is_some() { cut.from } else { cut.to };
        let (a, t) = (f.sr(open.from), f.sr(end));
        if bar.crossed(a, p) {
            return Err(refuse(format!("the rapid to the {what} {} crosses the stock - move the point or clear it", said(&f, p))));
        }
        if part.clearance(p, t) < keep {
            return Err(refuse(format!(
                "the move from the {what} {} to the first pass crosses the part{} - the nose must stay {:.3} mm off it; move the point or clear it",
                said(&f, p),
                leave_word(law),
                law.nose + law.leave.max(0.0)
            )));
        }
        let at = f.point(p.1, p.0);
        let mut lead = vec![Move { kind: MoveKind::Rapid, to: at, feed: None, arc: None, ..open }, Move { from: at, to: end, arc: None, ..cut }];
        if cut.arc.is_some() {
            lead.push(cut);
        }
        path.moves.splice(..=first, lead);
    }

    if let Some(pick) = picks.first(PickRole::Exit) {
        let what = "exit point";
        let p = bar.in_air(&f, pick, what).map_err(refuse)?;
        let (Some(last), Some(home)) = (path.moves.iter().rposition(|m| m.kind == MoveKind::Feed), path.moves.last().copied()) else {
            return Err(refuse(format!("the path has no pass to leave at the {what} - clear it")));
        };
        let cut = path.moves[last];
        let (e, h) = (f.sr(cut.to), f.sr(home.to));
        if part.clearance(e, p) < keep.min(part.clearance(e, e)) - EPS {
            return Err(refuse(format!(
                "the move from the last pass to the {what} {} crosses the part{} - the nose must stay {:.3} mm off it; move the point or clear it",
                said(&f, p),
                leave_word(law),
                law.nose + law.leave.max(0.0)
            )));
        }
        if !part.lifts(e, p, law) {
            return Err(refuse(format!(
                "the move from the last pass to the {what} {} runs along the surface that the pass cut - move the point away from that surface or clear it",
                said(&f, p)
            )));
        }
        if bar.crossed(p, h) {
            return Err(refuse(format!("the rapid from the {what} {} crosses the stock - move the point or clear it", said(&f, p))));
        }
        let at = f.point(p.1, p.0);
        let feed = cut.feed.or(home.feed);
        let tail = [Move { kind: MoveKind::Feed, from: cut.to, to: at, feed, arc: None, ..cut }, Move { kind: MoveKind::Rapid, from: at, feed: None, arc: None, ..home }];
        path.moves.splice(last + 1.., tail);
    }
    Ok(path)
}

fn leave_word(law: &PickLaw) -> &'static str {
    if law.leave > 0.0 { " or its stock to leave" } else { "" }
}

/// The point as the refusals say it: world Z and the radius.
fn said(f: &Frame, p: Sr) -> String {
    format!("(Z {:.3}, radius {:.3})", f.height_of(p.0), p.1)
}

/// The bar that a rapid must not enter: every point inside the stock's
/// radius and below its free end, each grown by the nose radius. It has
/// no lower end: behind the bar stands the chuck.
struct Bar {
    /// The radius a nose centre must reach to be outside (mm).
    r: f64,
    /// The station a nose centre must reach to be past the free end.
    s_top: f64,
    /// The bar's chuck end: the lowest station of the part or the stock.
    s_bot: f64,
}

impl Bar {
    fn of(facets: &[Triangle], stock: Option<&[Triangle]>, f: &Frame, nose: f64) -> Result<Bar, GenError> {
        let all = || facets.iter().chain(stock.unwrap_or(&[]));
        let mut prof = profile::profile_of(facets, f)?;
        if let Some(s) = stock.filter(|s| !s.is_empty()) {
            let sp = profile::profile_of(s, f)?;
            prof.r_max = prof.r_max.max(sp.r_max);
            prof.z_top = prof.z_top.max(sp.z_top);
        }
        let s_bot = all().flat_map(|t| [t.a, t.b, t.c]).map(|q| f.sr(q).0).fold(f64::INFINITY, f64::min);
        Ok(Bar { r: prof.r_max + nose, s_top: prof.z_top + nose, s_bot })
    }

    fn holds(&self, p: Sr) -> bool {
        p.1 < self.r - EPS && p.0 < self.s_top - EPS
    }

    /// The pick in the meridian plane, or its refusal: no Z, or not in air.
    fn in_air(&self, f: &Frame, pick: PathPick, what: &str) -> Result<Sr, String> {
        let Some(z) = pick.z else {
            return Err(format!("the {what} needs a Z - a turning point is a point in the lathe (Z, X) plane; state its Z or clear it"));
        };
        let p = f.sr(crate::geometry::Point3::new(pick.xy.x, pick.xy.y, z));
        if !(p.0.is_finite() && p.1.is_finite()) {
            return Err(format!("the {what} is not a finite point - clear it"));
        }
        if !self.holds(p) {
            return Ok(p);
        }
        let where_ = if p.0 < self.s_bot { "on the chuck side of the bar, inside its radius" } else { "inside the stock" };
        Err(format!(
            "the {what} {} is {where_} - a turning {what} stands in air, outside the bar or past its free end by the nose radius; move it or clear it",
            said(f, p)
        ))
    }

    /// Does the straight move `a -> b` enter the bar? The bar is the
    /// meet of two half-planes, so the move is clipped against each.
    fn crossed(&self, a: Sr, b: Sr) -> bool {
        let (mut lo, mut hi) = (0.0f64, 1.0f64);
        for (v0, v1, cap) in [(a.1, b.1, self.r - EPS), (a.0, b.0, self.s_top - EPS)] {
            // The stretch of t in [0, 1] where v0 + t (v1 - v0) < cap.
            let d = v1 - v0;
            if d.abs() <= f64::EPSILON {
                if v0 >= cap {
                    return false;
                }
            } else if d > 0.0 {
                hi = hi.min((cap - v0) / d);
            } else {
                lo = lo.max((cap - v0) / d);
            }
        }
        lo < hi
    }
}

/// The part's meridian section: the closed outline of the silhouette down
/// to the axis, in (station, radius).
struct Part {
    outline: Vec<Sr>,
}

impl Part {
    fn of(facets: &[Triangle], f: &Frame, tol: f64) -> Result<Part, GenError> {
        let pts = profile::silhouette(facets, f, tol * 0.25)?;
        let (Some(first), Some(last)) = (pts.first().copied(), pts.last().copied()) else {
            return Err(GenError::Empty("the model has no silhouette to turn".into()));
        };
        let mut outline = Vec::with_capacity(pts.len() + 2);
        outline.push((first.0, 0.0));
        outline.extend(pts);
        outline.push((last.0, 0.0));
        Ok(Part { outline })
    }

    /// Is `p` in the part? The silhouette ascends in station, so the part
    /// at a station is everything under the outline's highest radius there.
    fn holds(&self, p: Sr) -> bool {
        self.outline.windows(2).any(|w| {
            let (a, b) = (w[0], w[1]);
            if p.0 < a.0.min(b.0) || p.0 > a.0.max(b.0) {
                return false;
            }
            let ds = b.0 - a.0;
            let r = if ds.abs() <= f64::EPSILON { a.1.max(b.1) } else { a.1 + (b.1 - a.1) * (p.0 - a.0) / ds };
            p.1 < r
        })
    }

    /// The least distance from the move `a -> b` to the part; 0 when the
    /// move touches or enters it.
    fn clearance(&self, a: Sr, b: Sr) -> f64 {
        if self.holds(a) || self.holds(b) {
            return 0.0;
        }
        self.outline.windows(2).map(|w| seg_seg(a, b, w[0], w[1])).fold(f64::INFINITY, f64::min)
    }

    /// Does a feed from the cut's end `e` to `p` lift off the surface? Only
    /// asked where `e` stands on the cut (at the kind's own distance).
    fn lifts(&self, e: Sr, p: Sr, law: &PickLaw) -> bool {
        let at_e = self.clearance(e, e);
        if at_e > law.nose + law.leave.max(0.0) + law.tolerance {
            return true; // the pass ended in air
        }
        let len = ((p.0 - e.0).powi(2) + (p.1 - e.1).powi(2)).sqrt();
        if len <= EPS {
            return false;
        }
        let run = len.min(LIFT_RUN);
        let q = (e.0 + (p.0 - e.0) * run / len, e.1 + (p.1 - e.1) * run / len);
        self.clearance(q, q) >= at_e + LIFT_MIN * run / LIFT_RUN
    }
}

fn sub(a: Sr, b: Sr) -> Sr {
    (a.0 - b.0, a.1 - b.1)
}

fn cross(a: Sr, b: Sr) -> f64 {
    a.0 * b.1 - a.1 * b.0
}

/// The distance from `p` to the segment `a -> b`.
fn point_seg(p: Sr, a: Sr, b: Sr) -> f64 {
    let (d, w) = (sub(b, a), sub(p, a));
    let len2 = d.0 * d.0 + d.1 * d.1;
    let t = if len2 <= 0.0 { 0.0 } else { ((w.0 * d.0 + w.1 * d.1) / len2).clamp(0.0, 1.0) };
    let q = (a.0 + d.0 * t - p.0, a.1 + d.1 * t - p.1);
    (q.0 * q.0 + q.1 * q.1).sqrt()
}

/// The distance between the segments `a -> b` and `c -> d`: 0 when they
/// cross, else the least of the four end-to-segment distances.
fn seg_seg(a: Sr, b: Sr, c: Sr, d: Sr) -> f64 {
    let (o1, o2) = (cross(sub(b, a), sub(c, a)), cross(sub(b, a), sub(d, a)));
    let (o3, o4) = (cross(sub(d, c), sub(a, c)), cross(sub(d, c), sub(b, c)));
    if o1 * o2 < 0.0 && o3 * o4 < 0.0 {
        return 0.0;
    }
    point_seg(a, c, d).min(point_seg(b, c, d)).min(point_seg(c, a, b)).min(point_seg(d, a, b))
}

#[cfg(test)]
#[path = "start_exit_tests.rs"]
mod tests;
