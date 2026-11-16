//! The plasma cut's certification laws (#670, #672), run over the moves
//! just laid down, before the path leaves the generator:
//!
//! - **Kerf law.** Every lit move of a closed contour keeps its kerf off
//!   the part: no chord point lies inside the part grown by kerf / 2
//!   (less [`super::KERF_TOL`], itself less a coarse model's excess while a
//!   positive tolerance is left - #1053). A cut that would reach the part is refused by
//!   name, never trimmed.
//! - **Cover law** (#676 decision 12, #672). Every edge of a closed
//!   contour is reached by its own cut's kerf: no point of the nominal
//!   contour stands farther than one kerf width from the lit centre line.
//!   An outward offset bridges a slot or notch narrower than the kerf
//!   (and an inward offset drops a keyway narrower than the kerf from a
//!   hole) without splitting or closing a loop, so the ring law in
//!   [`super::ring::kerf_ring`] accepts it and the feature is silently
//!   left in the material. Here it is refused by name. The allowance is
//!   one kerf width (2 r) plus the tolerance, outside the tip zone of
//!   each inside corner (#791 review): the torch, radius r, can get no
//!   nearer an inside corner of opening θ than r / sin(θ/2) along its
//!   bisector, where its circle touches each wall r / tan(θ/2) from the
//!   vertex - so the stretch of wall the best possible path leaves more
//!   than 2 r away, r (tan(δ/2) − √3) from the vertex (δ = π − θ, the
//!   turn), is the uncut tip every plasma inside corner has, not a
//!   bridged feature, and is not judged. It is empty for openings of
//!   60° and more; a 30° V-notch keeps 1.5 mm of each wall. Uncovered
//!   material beyond it is what a bridged slot or keyway produces.
//!
//! Provenance: our own geometry over Tormach's public 1300PL manual
//! (UM10720: kerf ≈ 1.5x the orifice; a feature under the kerf cannot be
//! cut) and the LinuxCNC Plasma Primer; no vendor implementation read.

use crate::cl::{ContourRole, Toolpath, TorchSpan};
use crate::geo2d::{Point2, Region};
use crate::machining2d::GenError;

use super::emit::FIT_TOL;
use super::nest::Contour;
use super::ring::{dist, unit};

/// How far apart (mm) the contour and the chords are sampled.
const STEP: f64 = 0.25;

fn invalid(why: String) -> GenError {
    GenError::Invalid(format!("plasma profile: {why}"))
}

/// Every lit move of every closed contour keeps its kerf off the part.
pub(super) fn kerf_law(path: &Toolpath, spans: &[TorchSpan], forbidden: &Region) -> Result<(), GenError> {
    for sp in spans.iter().filter(|s| s.role != ContourRole::Open) {
        for i in sp.on..=sp.off {
            let Some(m) = path.moves.get(i) else { continue };
            for (a, b) in m.chords(FIT_TOL) {
                let k = (dist(a, b) / STEP).ceil().clamp(1.0, 4096.0) as u32;
                for j in 0..=k {
                    let f = f64::from(j) / f64::from(k);
                    let q = Point2::new(a.x + (b.x - a.x) * f, a.y + (b.y - a.y) * f);
                    if forbidden.contains(q) {
                        return Err(invalid(format!(
                            "contour C{}: move {i} would carry the kerf into the part near X{:.3} Y{:.3} - refused",
                            sp.contour, q.x, q.y
                        )));
                    }
                }
            }
        }
    }
    Ok(())
}

/// Every edge of every closed contour is reached by its own cut's kerf,
/// outside the tip zone of its inside corners: `spans[pos]` is the cut
/// of `cs[order[pos]]`.
pub(super) fn cover_law(
    path: &Toolpath,
    spans: &[TorchSpan],
    cs: &[Contour],
    order: &[usize],
    (kerf, tol): (f64, f64),
) -> Result<(), GenError> {
    // `tol` is [`super::KERF_TOL`], less a coarse model's excess (#1053).
    let reach = kerf + tol;
    for (sp, &k) in spans.iter().zip(order) {
        let Some(c) = cs.get(k) else { continue };
        if c.role == ContourRole::Open || sp.role == ContourRole::Open {
            continue;
        }
        let chords: Vec<(Point2, Point2)> =
            path.moves.get(sp.on..=sp.off).into_iter().flatten().flat_map(|m| m.chords(FIT_TOL)).collect();
        let n = c.pts.len();
        let tips: Vec<f64> = (0..n).map(|i| tip_zone(c, i, kerf * 0.5)).collect();
        for i in 0..n {
            let (a, b) = (c.pts[i], c.pts[(i + 1) % n]);
            let len = dist(a, b);
            let steps = (len / STEP).ceil().clamp(1.0, 65536.0) as u32;
            for j in 0..steps {
                let f = f64::from(j) / f64::from(steps);
                let s = f * len;
                if s < tips[i] || len - s < tips[(i + 1) % n] {
                    continue;
                }
                let q = Point2::new(a.x + (b.x - a.x) * f, a.y + (b.y - a.y) * f);
                let nearest = chords.iter().map(|(s, t)| pt_seg(q, *s, *t)).fold(f64::INFINITY, f64::min);
                if nearest.is_nan() || nearest > reach {
                    return Err(invalid(uncovered(sp.contour, c, i, q, kerf)));
                }
            }
        }
    }
    Ok(())
}

/// How far (mm) along each wall from vertex `i` the cover law does not
/// judge: at an inside corner of the material - a right turn on an outer
/// ring, a left turn on a hole, both rings running counter-clockwise -
/// the torch's closest circle touches the walls r · tan(δ/2) from the
/// vertex (δ the turn angle), and the stretch before that which even
/// this best path leaves more than 2 r away, r (tan(δ/2) − √3), is the
/// corner's uncut tip. Zero at a convex corner, a straight run, or any
/// opening of 60° and more; capped at one kerf width (2 r, reached at a
/// 30° opening) - the tip grows without bound as the opening closes, and
/// uncapped a deep narrow V-slot or a zero-width slit would be exempted
/// whole and generate silently uncut (#791 review). A non-finite tan
/// (a turn of π) takes the cap too.
fn tip_zone(c: &Contour, i: usize, r: f64) -> f64 {
    let n = c.pts.len();
    let (u, v, w) = (c.pts[(i + n - 1) % n], c.pts[i], c.pts[(i + 1) % n]);
    let (e1, e2) = (unit(u, v), unit(v, w));
    if !inside_corner(c.role, e1, e2) {
        return 0.0;
    }
    let cap = 2.0 * r;
    let turn = (e1.x * e2.x + e1.y * e2.y).clamp(-1.0, 1.0).acos();
    let tip = r * ((turn * 0.5).tan() - 3.0_f64.sqrt());
    if tip.is_finite() { tip.clamp(0.0, cap) } else { cap }
}

/// Is the turn from direction `e1` to `e2` an inside corner of the
/// material - a right turn on an outer ring, a left turn on a hole?
fn inside_corner(role: ContourRole, e1: Point2, e2: Point2) -> bool {
    let turn_sign = e1.x * e2.y - e1.y * e2.x;
    if role == ContourRole::Inner { turn_sign > 0.0 } else { turn_sign < 0.0 }
}

/// The refusal at an uncovered edge point `q` of edge `i`, naming the
/// gap across the feature - the nearest edge of the same contour that
/// is not `q`'s own or its neighbours - when that gap is under the kerf.
/// On a slot's floor (an edge with inside corners at both ends) that
/// nearest edge is beyond the walls, about the slot's depth away, so the
/// floor's own length - the slot's width - is taken when it is smaller.
fn uncovered(contour: u32, c: &Contour, i: usize, q: Point2, kerf: f64) -> String {
    let n = c.pts.len();
    let (u, a, b, w) = (c.pts[(i + n - 1) % n], c.pts[i], c.pts[(i + 1) % n], c.pts[(i + 2) % n]);
    let floor = inside_corner(c.role, unit(u, a), unit(a, b)) && inside_corner(c.role, unit(a, b), unit(b, w));
    let across = (0..n)
        .filter(|&k| k != i && k != (i + 1) % n && k != (i + n - 1) % n)
        .map(|k| pt_seg(q, c.pts[k], c.pts[(k + 1) % n]))
        .fold(f64::INFINITY, f64::min);
    let gap = if floor { across.min(dist(a, b)) } else { across };
    let what = if gap < kerf {
        format!("a slot, notch or keyway about {gap:.3} mm across, narrower than the kerf ({kerf:.3} mm),")
    } else {
        format!("a slot, notch or keyway narrower than the kerf ({kerf:.3} mm)")
    };
    format!(
        "contour C{contour}: the torch never comes within a kerf of the part edge near X{:.3} Y{:.3} - {what} is bridged by the offset, not cut; widen it past the kerf or use a narrower kerf",
        q.x, q.y
    )
}

/// Distance from `p` to segment a-b.
pub(super) fn pt_seg(p: Point2, a: Point2, b: Point2) -> f64 {
    let (dx, dy) = (b.x - a.x, b.y - a.y);
    let l2 = dx * dx + dy * dy;
    let t = if l2 > 0.0 { (((p.x - a.x) * dx + (p.y - a.y) * dy) / l2).clamp(0.0, 1.0) } else { 0.0 };
    (p.x - (a.x + t * dx)).hypot(p.y - (a.y + t * dy))
}
