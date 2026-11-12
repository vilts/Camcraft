//! Issue #1091: law (d)'s vertical dives re-read against the stock's top under the tool, not the stock's highest point - our own exact upper bound on a closed mesh's column (the topmost surface over a point lies on a facet whose plan covers it), specced from the shop rule "never rapid into uncut stock"; no vendor material consulted.
//!
//! The column law caps every column at `VerifyContext::stock_top_z`, the
//! highest point of the whole stock: on a stepped stock (a casting, a
//! carried body with a boss) a dive to just above the low part is
//! convicted as if the boss stood there too. [`acquit_by_stock_top`]
//! re-reads such a dive the way [`super::acquit_by_prior`] re-reads one
//! against the program ahead: it only removes a conviction, and only
//! where the solids prove the whole tool stack ends above all material.
//!
//! Issue #1095: an upright LATERAL rapid (a link, a traverse under the
//! clearance plane) is re-read the same way, each band over the capsule
//! it sweeps along the move rather than the disc of a dive, and the tip
//! taken at the rapid's lowest end for the whole move - a climbing or
//! falling traverse is judged as if it ran level at its low end, which
//! can only over-report. It takes the dive's rules whole: the bands of
//! [`bands`], grown and lifted by the excess. Upright only by
//! construction - a posed rapid keeps its conviction.
//!
//! The bound: for a closed mesh, the material over any plan point p
//! stands no higher than the highest vertex of a facet whose plan
//! (triangle, or the segment of a vertical facet) comes within the disc
//! around the dive, or the capsule along the rapid - the topmost surface
//! over p is on a facet covering p.
//! A facet that only grazes the disc still lends its highest vertex, so
//! a slope reads high (an over-report, never a hide). Each band of the
//! stack is judged at its own disc and its own height over the tip: the
//! flute's radius at the tip, the shank's from the flute's top, the
//! holder's from the shank's top (and on up - the cap is a height, so a
//! band that is taller changes nothing).
//!
//! Never narrower than the laws read the stack (#957/#993 merge): the
//! shank at the STATED shank radius, never a drawn neck's narrower bands
//! (`NeckBands` are never wider than it), and above the shank the
//! wider of holder and shank radius - what `BodyBands` bounds a holder
//! body by there. A coarse model (#864/#904, `model_excess` `e`) may
//! stand up to `e` outside its facets, sideways and up: every disc
//! grows by `e` and every top rises by `e`, so a dive that clears the
//! meshed material by less than the excess stays convicted. A
//! non-finite excess keeps every conviction. Fails closed: a solid that is
//! not one closed, outward shell ([`super::stock_top_shell`] - an open
//! shell, a non-manifold edge, a non-finite vertex, no facets), a band
//! whose radius or offset is not a finite non-negative number, or a
//! flute disc that meets no facet at all leaves the conviction standing.

use super::laws::{dist2_seg, dist2_segs};
use super::stock_top_shell::closed_shell;
use super::types::{Mode, Warning};

use crate::cl::{MoveKind, Toolpath};
use crate::geo2d::Point2;
use crate::geometry::Triangle;
use crate::tools::ToolGeom;

/// `warns` without the convictions of upright rapids - vertical dives
/// DOWN, and lateral rapids (#1095) - whose whole tool stack ends above
/// every facet of `solids` within reach (see the module doc). `solids`
/// are the closed meshes that bound the material the op can meet - the
/// stock and the part - in the path's own frame. Posed rapids, vertical
/// retracts, and every other warning keep their verdict; so does every
/// rapid of a mode whose rapids are not judged by the vertical column
/// (radial, revolved, rotary).
pub fn acquit_by_stock_top(
    path: &Toolpath,
    mode: Mode,
    solids: &[&[Triangle]],
    tool: &ToolGeom,
    z_tol: f64,
    excess: f64,
    warns: &mut Vec<Warning>,
) {
    if matches!(mode, Mode::Radial | Mode::Revolve | Mode::Rotary)
        || solids.is_empty()
        || !z_tol.is_finite()
        || !excess.is_finite()
        || !solids.iter().all(|s| closed_shell(s))
    {
        return;
    }
    let e = excess.max(0.0);
    let Some(bands) = bands(tool, e) else { return };
    warns.retain(|w| {
        let Warning::RapidInStock { at } = *w else { return true };
        let Some(m) = path.moves.get(at) else { return true };
        let (dx, dy) = (m.to.x - m.from.x, m.to.y - m.from.y);
        let upright = m.axis.is_none_or(|a| a[0].abs() <= 1e-12 && a[1].abs() <= 1e-12 && a[2] > 0.0);
        if m.kind != MoveKind::Rapid || !upright {
            return true;
        }
        if dx * dx + dy * dy > z_tol * z_tol {
            // #1095: a lateral rapid, over its whole trace at its low end.
            let low = m.from.z.min(m.to.z);
            if !(low.is_finite() && m.from.is_finite() && m.to.is_finite()) {
                return true;
            }
            let (a, b) = (Point2::new(m.from.x, m.from.y), Point2::new(m.to.x, m.to.y));
            // The excess lifts every top by `e`, as for a dive.
            return !clear_of(solids, (a, b), low - e, &bands, z_tol);
        }
        if !(m.to.z.is_finite() && m.from.z.is_finite()) || m.to.z >= m.from.z {
            return true;
        }
        let c = Point2::new(m.to.x, m.to.y);
        !clear_of(solids, (c, c), m.to.z - e, &bands, z_tol)
    });
}

/// The stack's bands as `(radius, height of its bottom over the tip)`,
/// each radius grown by the excess `e` (see the module doc); `None` when
/// any number is not a finite non-negative value.
fn bands(t: &ToolGeom, e: f64) -> Option<[(f64, f64); 3]> {
    let shank_at = t.flute_len;
    let holder_at = t.flute_len + t.shank_len;
    // `f64::max` drops a NaN: every raw number is checked before it.
    let raw = [t.radius, t.shank_radius, t.holder_radius, t.flute_len, t.shank_len];
    if !raw.iter().all(|v| v.is_finite() && *v >= 0.0) {
        return None;
    }
    let above_shank = t.holder_radius.max(t.shank_radius);
    let b = [(t.radius + e, 0.0), (t.shank_radius + e, shank_at), (above_shank + e, holder_at)];
    b.iter().all(|&(r, h)| r.is_finite() && r >= 0.0 && h.is_finite() && h >= 0.0).then_some(b)
}

/// Whether every band of the stack, its tip at `tip` along the plan
/// segment `trace` (one point for a dive), stands above the highest
/// facet within its reach of it (less `z_tol`). The flute's reach must
/// meet some facet - an empty reading is no proof.
fn clear_of(solids: &[&[Triangle]], trace: (Point2, Point2), tip: f64, bands: &[(f64, f64); 3], z_tol: f64) -> bool {
    bands.iter().enumerate().all(|(k, &(r, h))| match top_within(solids, trace, r) {
        Some(top) => tip + h >= top - z_tol,
        None => k > 0, // nothing under a stack band is air; under the flute it is unknown
    })
}

/// The highest vertex of any facet whose plan comes within `r` of the
/// plan segment `trace` (a disc about a point, a capsule along a move).
fn top_within(solids: &[&[Triangle]], (a, b): (Point2, Point2), r: f64) -> Option<f64> {
    let r2 = r * r;
    let mut top: Option<f64> = None;
    for t in solids.iter().flat_map(|s| s.iter()) {
        if dist2_seg_tri(a, b, t) <= r2 {
            let z = t.a.z.max(t.b.z).max(t.c.z);
            top = Some(top.map_or(z, |m| m.max(z)));
        }
    }
    top
}

/// The squared plan distance from segment `a`-`b` to triangle `t`: 0 when
/// they meet, else the least of the ends' distances to the triangle and
/// the segment's to its edges (a segment that does not meet a triangle
/// comes nearest it at one of its ends or at an edge). A point segment
/// reads exactly [`dist2_tri`] of the point: its distance to an edge is
/// that edge's term of `dist2_tri` already.
fn dist2_seg_tri(a: Point2, b: Point2, t: &Triangle) -> f64 {
    let ends = dist2_tri(a, t).min(dist2_tri(b, t));
    if a == b || ends == 0.0 {
        return ends;
    }
    let (p, q, r) = (Point2::new(t.a.x, t.a.y), Point2::new(t.b.x, t.b.y), Point2::new(t.c.x, t.c.y));
    ends.min(dist2_segs(a, b, p, q)).min(dist2_segs(a, b, q, r)).min(dist2_segs(a, b, r, p))
}

/// The squared plan distance from `p` to triangle `t` (0 inside it). A
/// triangle whose plan is degenerate is read as its three edges.
fn dist2_tri(p: Point2, t: &Triangle) -> f64 {
    let (a, b, c) = (Point2::new(t.a.x, t.a.y), Point2::new(t.b.x, t.b.y), Point2::new(t.c.x, t.c.y));
    let cross = |u: Point2, v: Point2| (v.x - u.x) * (p.y - u.y) - (v.y - u.y) * (p.x - u.x);
    let (d0, d1, d2) = (cross(a, b), cross(b, c), cross(c, a));
    let neg = d0 < 0.0 || d1 < 0.0 || d2 < 0.0;
    let pos = d0 > 0.0 || d1 > 0.0 || d2 > 0.0;
    let area = (b.x - a.x) * (c.y - a.y) - (b.y - a.y) * (c.x - a.x);
    if area != 0.0 && !(neg && pos) {
        return 0.0;
    }
    dist2_seg(p, a, b).min(dist2_seg(p, b, c)).min(dist2_seg(p, c, a))
}


#[cfg(test)]
#[path = "stock_top_tests.rs"]
mod tests;
