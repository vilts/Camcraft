//! Issue #2121: law (d)'s traverses at the clearance plane re-read against the solids under the tool - our own upper bound on a closed mesh's column over a swept strip (the topmost surface over a point lies on a facet whose plan covers it, so it stands no higher than that facet's plane over the strip), specced from the shop rule "never traverse through uncut stock"; no vendor material consulted.
//!
//! Since #2121 the column law reads a lateral rapid AT the clearance
//! plane too, and it caps every column at `VerifyContext::stock_top_z`:
//! the stock's highest point, or - in a 3+2 plane's frame - the highest
//! corner of the tilted stock box. Under the old clearance gate that
//! over-report never met a traverse; without it, a traverse over the
//! low side of a tilted block would be convicted for stock that stands
//! only at the far corner. [`acquit_traverses_by_solids`] re-reads such
//! a traverse against the solids themselves.
//!
//! It is [`super::acquit_by_stock_top`]'s re-read (#1091, #1095) with a
//! tighter height: that one takes the highest VERTEX of every facet in
//! reach (a slope reads as its far end), this one the highest point of
//! each facet's plane over the strip the band sweeps - the facet's plan
//! triangle clipped to the rectangle that encloses the capsule of the
//! band's radius about the rapid's trace. The rectangle holds the
//! capsule, and a plane's height over a convex polygon peaks at a
//! vertex of it, so the reading is still an upper bound on the material
//! under the band. The cut is made on the facet in space (the four
//! half-planes read x and y only), so a vertical wall, whose plan has no
//! area, is cut to the part of it over the strip like any other facet.
//!
//! Everything else is that re-read's rule, whole: only upright rapids,
//! every band of the stack at its own radius and height
//! ([`super::stock_top`]'s bands), each grown and lifted by the model
//! excess, only closed outward shells, and a flute that meets no facet
//! at all is unknown. And it is narrower: only a lateral rapid riding at
//! or above the clearance plane is re-read - the convictions #2121
//! added. Every conviction the law gave before keeps its own re-reads.

use super::stock_top::bands;
use super::stock_top_shell::closed_shell;
use super::types::{Mode, Warning};
use super::upright::is_upright;

use crate::cl::{MoveKind, Toolpath};
use crate::geo2d::Point2;
use crate::geometry::{Point3, Triangle};
use crate::tools::ToolGeom;

/// `warns` without the convictions of upright lateral rapids at or above
/// `clearance_z` whose whole tool stack rides above every facet of
/// `solids` under the strip it sweeps (see the module doc). `solids` are
/// the closed meshes that bound the material the op can meet, in the
/// path's own frame.
pub fn acquit_traverses_by_solids(
    path: &Toolpath,
    mode: Mode,
    solids: &[&[Triangle]],
    tool: &ToolGeom,
    (clearance_z, z_tol, excess): (f64, f64, f64),
    warns: &mut Vec<Warning>,
) {
    if matches!(mode, Mode::Radial | Mode::Revolve | Mode::Rotary)
        || solids.is_empty()
        || !(clearance_z.is_finite() && z_tol.is_finite() && excess.is_finite())
        || !solids.iter().all(|s| closed_shell(s))
    {
        return;
    }
    let e = excess.max(0.0);
    let Some(bands) = bands(tool, e) else { return };
    warns.retain(|w| {
        let Warning::RapidInStock { at } = *w else { return true };
        let Some(m) = path.moves.get(at) else { return true };
        if m.kind != MoveKind::Rapid || !m.axis.is_none_or(|a| is_upright(&a)) || !(m.from.is_finite() && m.to.is_finite()) {
            return true;
        }
        let (a, b) = (Point2::new(m.from.x, m.from.y), Point2::new(m.to.x, m.to.y));
        let (dx, dy) = (b.x - a.x, b.y - a.y);
        let low = m.from.z.min(m.to.z);
        if dx * dx + dy * dy <= z_tol * z_tol || low < clearance_z - z_tol {
            return true;
        }
        // The excess lifts every top by `e`, as in the stock-top re-read.
        let tip = low - e;
        let clear = bands.iter().enumerate().all(|(k, &(r, h))| match top_under(solids, a, b, r) {
            Some(top) => tip + h >= top - z_tol,
            None => k > 0, // nothing under a stack band is air; under the flute it is unknown
        });
        !clear
    });
}

/// The highest any facet of `solids` stands over the rectangle enclosing
/// the capsule of radius `r` about the plan segment `a`-`b`; `None` when
/// no facet's plan meets it.
fn top_under(solids: &[&[Triangle]], a: Point2, b: Point2, r: f64) -> Option<f64> {
    let (dx, dy) = (b.x - a.x, b.y - a.y);
    let len = dx.hypot(dy);
    if !(len.is_finite() && len > 0.0 && r.is_finite() && r >= 0.0) {
        return None;
    }
    // The strip's frame: `u` along the trace, `v` across it.
    let (ux, uy) = (dx / len, dy / len);
    let along = |p: &Point3| (p.x - a.x) * ux + (p.y - a.y) * uy;
    let across = |p: &Point3| (p.y - a.y) * ux - (p.x - a.x) * uy;
    let mut top: Option<f64> = None;
    let mut raise = |z: f64| top = Some(top.map_or(z, |t: f64| t.max(z)));
    for t in solids.iter().flat_map(|s| s.iter()) {
        let mut poly = vec![t.a, t.b, t.c];
        // Four half-planes: -r <= along <= len + r, -r <= across <= r.
        poly = clip(&poly, |p| along(p) + r);
        poly = clip(&poly, |p| len + r - along(p));
        poly = clip(&poly, |p| across(p) + r);
        poly = clip(&poly, |p| r - across(p));
        for p in &poly {
            raise(p.z);
        }
    }
    top.filter(|t| !t.is_nan())
}

/// `poly` (planar, convex, in space) cut to the half-space `keep(p) >= 0`
/// (Sutherland-Hodgman): the crossings are linear in the edge, so they
/// stay on the facet's plane. A non-finite `keep` drops the point.
fn clip(poly: &[Point3], keep: impl Fn(&Point3) -> f64) -> Vec<Point3> {
    let mut out = Vec::with_capacity(poly.len() + 1);
    for (i, p) in poly.iter().enumerate() {
        let Some(q) = poly.get((i + 1) % poly.len()) else { continue };
        let (kp, kq) = (keep(p), keep(q));
        if kp >= 0.0 {
            out.push(*p);
        }
        if (kp >= 0.0) != (kq >= 0.0) && kp.is_finite() && kq.is_finite() {
            let s = kp / (kp - kq);
            out.push(Point3::new(p.x + (q.x - p.x) * s, p.y + (q.y - p.y) * s, p.z + (q.z - p.z) * s));
        }
    }
    out
}

#[cfg(test)]
#[path = "traverse_solids_tests.rs"]
mod tests;
