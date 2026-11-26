//! The contour's lead geometry beyond the lead shape (M11.3 C): straight
//! extensions before the lead-in and after the lead-out, the vertical
//! lead-in that descends along itself, and the axis-aligned ring start
//! that lets an arc entry post as a true G18/G19. Every extension rides
//! `lead_fit`'s room law (halved, then refused by name); the start
//! choice only picks which point of the same ring starts it.
//!
//! Provenance: lead-in / lead-out extensions and ramped (vertical)
//! lead-ins are the standard contour lead vocabulary of published CAM
//! practice and machining handbooks; RS-274 / ISO 6983 G17/G18/G19 plane
//! semantics for why an axis-aligned start matters. The laws are ours; no
//! vendor material consulted.

use crate::cl::{MoveKind, PathBuilder};
use crate::entry_arc::seg_dir;
use crate::geo2d::Point2;
use crate::geometry::Point3;

use super::arcs::{Seg, walk_tangent};
use super::contour::ContourParams;
use super::contour_entry::{crosses, say};
use super::fillet::Legal;
use super::lead_fit::{segs_hold, slack};
use super::ring::dist;

/// Halvings an extension gets before it is refused.
const EXT_SHRINKS: usize = 4;

/// The ring `pts` restarted at the midpoint of its edge nearest an axis
/// direction (within `tol_deg`; ties to the longer edge), so the first
/// cut - and the arc entry tangent to it - runs along X or Y. `None` keeps
/// the start: its own first edge is exactly on an axis, no edge is within
/// the tolerance, or no edge is nearer an axis than the first. Only an
/// exact edge posts a G18/G19 (`entry_arc::true_arc`), so a first edge
/// that is merely within the tolerance does not hold the start (#2849).
pub(super) fn axis_reseat(pts: &[Point2], tol_deg: f64) -> Option<Vec<Point2>> {
    let n = pts.len();
    if n < 3 || !(tol_deg.is_finite() && tol_deg >= 0.0) {
        return None;
    }
    let dev = |i: usize| {
        let (a, b) = (pts[i], pts[(i + 1) % n]);
        let (dx, dy) = ((b.x - a.x).abs(), (b.y - a.y).abs());
        let ang = dy.atan2(dx).to_degrees();
        (ang.min(90.0 - ang), dist(a, b))
    };
    let (d0, l0) = dev(0);
    if d0 == 0.0 && l0 > 0.0 {
        return None;
    }
    let mut best: Option<(usize, f64, f64)> = None;
    for i in 0..n {
        let (d, l) = dev(i);
        if !(d.is_finite() && l.is_finite() && l > 0.0 && d <= tol_deg) {
            continue;
        }
        let better = match best {
            None => true,
            Some((_, bd, bl)) => d < bd || (d == bd && l > bl),
        };
        if better {
            best = Some((i, d, l));
        }
    }
    let (i, _, _) = best?;
    if i == 0 {
        return None;
    }
    let (a, b) = (pts[i], pts[(i + 1) % n]);
    // The midpoint keeps the aligned coordinate exact: (y + y) / 2 == y.
    let mut v = Vec::with_capacity(n + 1);
    v.push(Point2::new((a.x + b.x) * 0.5, (a.y + b.y) * 0.5));
    v.extend((1..=n).map(|k| pts[(i + k) % n]));
    Some(v)
}

/// The direction the segments leave `from` in.
fn start_dir(from: Point2, segs: &[Seg]) -> Option<Point2> {
    segs.first().and_then(|s| seg_dir(from, s))
}

/// Where `segs` walked from `from` end, and their travel direction there.
fn end_dir(from: Point2, segs: &[Seg]) -> Option<(Point2, Point2)> {
    let mut at = from;
    let mut dir = None;
    for seg in segs {
        match seg {
            Seg::Line(qs) => {
                for &q in qs {
                    if q.x != at.x || q.y != at.y {
                        dir = Some(Point2::new(q.x - at.x, q.y - at.y));
                    }
                    at = q;
                }
            }
            Seg::Arc { center, ccw, to } => {
                let (rx, ry) = (to.x - center.x, to.y - center.y);
                dir = Some(if *ccw { Point2::new(-ry, rx) } else { Point2::new(ry, -rx) });
                at = *to;
            }
        }
    }
    let d = dir?;
    let l = d.x.hypot(d.y);
    (l.is_finite() && l > 0.0).then(|| (at, Point2::new(d.x / l, d.y / l)))
}

fn unit(d: Point2) -> Option<Point2> {
    let l = d.x.hypot(d.y);
    (l.is_finite() && l > 0.0).then(|| Point2::new(d.x / l, d.y / l))
}

/// Lay the approach's extensions onto one window's leads: `ext_in`
/// straight back along the lead-in's start direction (the walk's, with
/// no lead-in), `ext_out` straight on along the lead-out's end. Each is
/// held to the legal room and kept off the other lead (no crossing),
/// halved where it does not fit and refused by name where nothing does.
#[allow(clippy::too_many_arguments)]
pub(super) fn extend_leads(
    entry: Point2,
    lead: Vec<Seg>,
    lead_off: Vec<Seg>,
    walk: &[Seg],
    poly: &[Point2],
    p: &ContourParams,
    legal: &Legal,
    notes: &mut Vec<String>,
) -> (Point2, Vec<Seg>, Vec<Seg>) {
    let a = &p.links.approach;
    let Some(&e) = poly.last() else {
        return (entry, lead, lead_off);
    };
    let tol = slack(p);
    let (mut entry, mut lead, mut lead_off) = (entry, lead, lead_off);
    if a.ext_in > 0.0 {
        let u = start_dir(entry, &lead).and_then(unit).unwrap_or_else(|| walk_tangent(walk, poly, true));
        let mut done = false;
        for k in 0..=EXT_SHRINKS {
            let len = a.ext_in * 0.5f64.powi(k as i32);
            let q = Point2::new(entry.x - u.x * len, entry.y - u.y * len);
            let ext = [Seg::Line(vec![entry])];
            let with: Vec<Seg> = ext.iter().cloned().chain(lead.iter().cloned()).collect();
            if segs_hold(legal, q, &ext, tol) && !crosses(q, &with, e, &lead_off) {
                if k > 0 {
                    say(notes, format!("lead: the {:.3} mm extension before the lead-in at ({:.3}, {:.3}) leaves the legal cutter region - shrunk to {len:.3} mm", a.ext_in, entry.x, entry.y));
                }
                entry = q;
                lead = with;
                done = true;
                break;
            }
        }
        if !done {
            say(notes, format!("lead: no extension before the lead-in fits the legal cutter region at ({:.3}, {:.3}) - refused", entry.x, entry.y));
        }
    }
    if a.ext_out > 0.0 {
        let (end, u) = end_dir(e, &lead_off).unwrap_or_else(|| (e, walk_tangent(walk, poly, false)));
        let mut done = false;
        for k in 0..=EXT_SHRINKS {
            let len = a.ext_out * 0.5f64.powi(k as i32);
            let q = Point2::new(end.x + u.x * len, end.y + u.y * len);
            let ext = [Seg::Line(vec![q])];
            let with: Vec<Seg> = lead_off.iter().cloned().chain(ext.iter().cloned()).collect();
            if segs_hold(legal, end, &ext, tol) && !crosses(entry, &lead, e, &with) {
                if k > 0 {
                    say(notes, format!("lead: the {:.3} mm extension after the lead-out at ({:.3}, {:.3}) leaves the legal cutter region - shrunk to {len:.3} mm", a.ext_out, end.x, end.y));
                }
                lead_off = with;
                done = true;
                break;
            }
        }
        if !done {
            say(notes, format!("lead: no extension after the lead-out fits the legal cutter region at ({:.3}, {:.3}) - refused", end.x, end.y));
        }
    }
    (entry, lead, lead_off)
}

/// The approach's extensions on an open run's leads (M11.5 owed): the
/// same straight pieces [`extend_leads`] lays - `ext_in` back along the
/// lead-in's start (the walk's, with no lead-in), `ext_out` on along
/// the lead-out's end - with no legal region to hold them to: an open
/// run bounds no area, and its ends are air by the chain contour's own
/// law. Both 0 (the default): everything untouched.
pub(super) fn extend_open(
    entry: Point2,
    lead: Vec<Seg>,
    lead_off: Vec<Seg>,
    walk: &[Seg],
    poly: &[Point2],
    a: &crate::approach::Approach,
) -> (Point2, Vec<Seg>, Vec<Seg>) {
    let Some(&e) = poly.last() else {
        return (entry, lead, lead_off);
    };
    let (mut entry, mut lead, mut lead_off) = (entry, lead, lead_off);
    if a.ext_in > 0.0 && a.ext_in.is_finite() {
        let u = start_dir(entry, &lead).and_then(unit).unwrap_or_else(|| walk_tangent(walk, poly, true));
        let q = Point2::new(entry.x - u.x * a.ext_in, entry.y - u.y * a.ext_in);
        lead.insert(0, Seg::Line(vec![entry]));
        entry = q;
    }
    if a.ext_out > 0.0 && a.ext_out.is_finite() {
        let (end, u) = end_dir(e, &lead_off).unwrap_or_else(|| (e, walk_tangent(walk, poly, false)));
        lead_off.push(Seg::Line(vec![Point2::new(end.x + u.x * a.ext_out, end.y + u.y * a.ext_out)]));
    }
    (entry, lead, lead_off)
}

/// The length of `segs` walked from `from`.
fn length(from: Point2, segs: &[Seg]) -> f64 {
    let mut at = from;
    let mut l = 0.0;
    for seg in segs {
        match seg {
            Seg::Line(qs) => {
                for &q in qs {
                    l += dist(at, q);
                    at = q;
                }
            }
            Seg::Arc { center, ccw, to } => {
                let r = dist(at, *center);
                let a0 = (at.y - center.y).atan2(at.x - center.x);
                let a1 = (to.y - center.y).atan2(to.x - center.x);
                let sw = if *ccw { a1 - a0 } else { a0 - a1 }.rem_euclid(std::f64::consts::TAU);
                l += r * sw;
                at = *to;
            }
        }
    }
    l
}

/// The vertical lead-in (M11.3 C): the lead from the builder's position
/// (standing at the lead's start on `z_top`) onto the cut at `z`, Z
/// falling linearly with length - its XY is the lead the room law proved
/// at the plane, and every plane above holds the same section. Arcs carry
/// their Z change (a helical piece).
pub(super) fn push_lead_down(b: &mut PathBuilder, segs: &[Seg], z_top: f64, z: f64, feed: f64) {
    let at = b.at();
    let total = length(Point2::new(at.x, at.y), segs);
    if !(total.is_finite() && total > 0.0) {
        super::arcs::push_segs(b, segs, z, feed);
        return;
    }
    let mut run = 0.0;
    let mut from = Point2::new(at.x, at.y);
    let n = segs.len();
    for (i, seg) in segs.iter().enumerate() {
        match seg {
            Seg::Line(qs) => {
                for (j, &q) in qs.iter().enumerate() {
                    run += dist(from, q);
                    from = q;
                    let last = i + 1 == n && j + 1 == qs.len();
                    let zq = if last { z } else { z_top - (z_top - z) * (run / total) };
                    b.push(MoveKind::Feed, Point3::new(q.x, q.y, zq), Some(feed));
                }
            }
            Seg::Arc { center, ccw, to } => {
                run += length(from, std::slice::from_ref(seg));
                from = *to;
                let zq = if i + 1 == n { z } else { z_top - (z_top - z) * (run / total) };
                b.push_arc(MoveKind::Feed, Point3::new(to.x, to.y, zq), Some(feed), *center, *ccw);
            }
        }
    }
}

/// The closed walk `walk` (from `poly[0]`) carried on past its start by
/// `ov` mm over its OWN first segments - whole segments (a line's points
/// one by one), at least one - with the polyline extended to match: the
/// overlap re-cuts exactly what the start cut, chord for chord.
pub(super) fn retrace(poly: &[Point2], walk: &[Seg], ov: f64) -> (Vec<Point2>, Vec<Seg>) {
    let (mut p2, mut w2) = (poly.to_vec(), walk.to_vec());
    let Some(&s) = poly.first() else {
        return (p2, w2);
    };
    let mut at = s;
    let mut run = 0.0;
    'outer: for seg in walk {
        match seg {
            Seg::Line(qs) => {
                let mut take = Vec::new();
                for &q in qs {
                    run += dist(at, q);
                    at = q;
                    take.push(q);
                    p2.push(q);
                    if run >= ov {
                        w2.push(Seg::Line(take));
                        break 'outer;
                    }
                }
                w2.push(Seg::Line(take));
            }
            Seg::Arc { to, .. } => {
                run += length(at, std::slice::from_ref(seg));
                at = *to;
                w2.push(seg.clone());
                p2.push(*to);
                if run >= ov {
                    break;
                }
            }
        }
    }
    (p2, w2)
}

#[cfg(test)]
mod reseat_tests {
    use super::*;

    #[test]
    fn a_first_edge_inside_the_tolerance_but_off_the_axis_reseats_to_the_exact_edge() {
        // #2849: a fine chord 0.93 degrees off X held the start, and the
        // arc entry on it had no standard plane. The exact edge wins.
        let off = 0.93_f64.to_radians().tan();
        let pts = [Point2::new(10.0, -10.0), Point2::new(9.0, -10.0 + off), Point2::new(-10.0, 0.0), Point2::new(-10.0, -12.0), Point2::new(4.0, -13.0)];
        let v = axis_reseat(&pts, 1.0).expect("reseated");
        assert_eq!(v[0], Point2::new(-10.0, -6.0));
        assert_eq!(v.len(), pts.len() + 1);
    }

    #[test]
    fn an_exact_first_edge_and_a_first_edge_that_is_the_best_keep_the_start() {
        let exact = [Point2::new(0.0, 0.0), Point2::new(5.0, 0.0), Point2::new(5.0, 9.0), Point2::new(0.0, 4.0)];
        assert!(axis_reseat(&exact, 1.0).is_none());
        let off = 0.5_f64.to_radians().tan();
        let best = [Point2::new(0.0, 0.0), Point2::new(10.0, 10.0 * off), Point2::new(3.0, 9.0)];
        assert!(axis_reseat(&best, 1.0).is_none());
    }
}
