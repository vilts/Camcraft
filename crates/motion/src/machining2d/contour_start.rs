//! A user-picked start point on a closed 2D contour (#1448): the ring
//! nearest the pick is re-sequenced to begin at the pick's projection onto
//! it, the lead-in is planned there by the lead law, and anything that law
//! cannot hold at the pick is refused by name - a pick is a wish, it never
//! overrides a check, and the entry never moves away from it silently.
//! #1473 adds the exit point: the ring is cut all the way round, then on
//! along its own polyline to the pick's projection, and the lead-out
//! leaves there - or it is refused by name, never moved. Also the automatic seam (axis start, control seam, lead-room reseat)
//! that every ring without a pick keeps. Provenance: first principles -
//! the closest point on a segment (orthogonal projection clamped to the
//! segment) and the rotation of a closed polyline's vertex list; the
//! start-point parameter is the common contour-entry vocabulary of
//! published CAM practice. No vendor material consulted.

use crate::geo2d::{Loop, Point2};

use super::arcs::Seg;
use super::contour_comp;
use super::contour_entry::{crosses, seam_leads};
use super::contour_params::{ContourParams, Lead};
use super::face::GenError;
use super::fillet::Legal;
use super::lead_ext::{axis_reseat, extend_leads};
use super::lead_fit::reseat;

/// Squared Euclidean distance between `a` and `b`.
fn dist2(a: Point2, b: Point2) -> f64 {
    (a.x - b.x) * (a.x - b.x) + (a.y - b.y) * (a.y - b.y)
}

/// The closest point of segment `a`-`b` to `q`, and its squared distance.
fn foot(a: Point2, b: Point2, q: Point2) -> (Point2, f64) {
    let (dx, dy) = (b.x - a.x, b.y - a.y);
    let dd = dx * dx + dy * dy;
    let t = if dd > 0.0 { (((q.x - a.x) * dx + (q.y - a.y) * dy) / dd).clamp(0.0, 1.0) } else { 0.0 };
    let f = Point2::new(a.x + dx * t, a.y + dy * t);
    (f, dist2(q, f))
}

/// The closest point to `q` on the closed ring `pts` (its last vertex
/// wraps to the first): `(segment index, point, squared distance)`, the
/// first segment winning a tie. `None` for an empty ring or a non-finite
/// answer.
fn nearest_on_ring(pts: &[Point2], q: Point2) -> Option<(usize, Point2, f64)> {
    let n = pts.len();
    let mut best: Option<(usize, Point2, f64)> = None;
    for i in 0..n {
        let (f, d2) = foot(pts[i], pts[(i + 1) % n], q);
        if d2.is_finite() && best.is_none_or(|b| d2 < b.2) {
            best = Some((i, f, d2));
        }
    }
    best
}

/// The farthest a pick may sit from the cutter-centre ring it starts:
/// the wall's own offset (radius plus stock to leave) and one tool
/// diameter of slack for a click beside the wall.
fn reach(p: &ContourParams) -> f64 {
    p.tool_diameter * 1.5 + p.stock_to_leave
}

/// Which ring (index into `loops`) takes the user's start point, with the
/// pick; `Ok(None)` when no start point is set. Refused by name: a pick
/// farther than [`reach`] from every ring, and compensation in the
/// control (its seam is the comp law's), which this slice does not
/// certify. Tabs take the pick (#1473): [`seat`] starts the picked ring
/// at the pick on every level, tabbed ones too, so the tab windows are
/// measured from the picked start and the first window begins there.
pub(super) fn pick_ring(loops: &[&Loop], p: &ContourParams) -> Result<Option<(usize, Point2)>, GenError> {
    let ap = &p.links.approach;
    let Some(q) = ap.start_point else {
        return Ok(None);
    };
    let at = format!("start point ({:.3}, {:.3})", q.x, q.y);
    if ap.compensation.in_control() {
        return Err(GenError::Invalid(format!(
            "{at}: a picked start is not taken with {} compensation yet - its seam is the \
             compensation law's; clear the start point or compensate in the computer",
            ap.compensation.label()
        )));
    }
    // The part-boundary room law does not include the standing tabs.
    // Only normal departures are certified here; tangential motion can
    // cut the tab ending at the pick, including an extension with no lead.
    if p.tab_count > 0 {
        let normal = |lead| matches!(lead, Lead::None | Lead::Perpendicular { .. });
        let unsafe_part = if !normal(p.lead) {
            Some("lead-in")
        } else if !normal(p.lead_out_shape()) {
            Some("lead-out")
        } else if ap.ext_in > 0.0 || ap.ext_out > 0.0 {
            Some("extension")
        } else {
            None
        };
        if let Some(part) = unsafe_part {
            return Err(GenError::Invalid(format!(
                "{at}: the {part} is not certified with standing tabs at a picked start - \
                 use no lead or perpendicular leads without extensions, or clear the start point"
            )));
        }
    }
    nearest_ring(loops, q, &at, "start", p).map(Some)
}

/// The ring (index into `loops`) nearest the pick `q`, refused by name
/// when it lies farther than [`reach`] from every ring. `at` names the
/// pick, `what` is "start" or "exit".
fn nearest_ring(loops: &[&Loop], q: Point2, at: &str, what: &str, p: &ContourParams) -> Result<(usize, Point2), GenError> {
    let best = loops
        .iter()
        .enumerate()
        .filter_map(|(i, l)| nearest_on_ring(l.points(), q).map(|(_, _, d2)| (i, d2)))
        .min_by(|a, b| a.1.total_cmp(&b.1));
    let Some((i, d2)) = best else {
        return Err(GenError::Invalid(format!("{at}: no contour ring to {what} on")));
    };
    let d = d2.sqrt();
    let lim = reach(p);
    if d.is_nan() || d > lim {
        return Err(GenError::Invalid(format!(
            "{at} is {d:.3} mm from the nearest cutter path, farther than the {lim:.3} mm a \
             {what} point may sit from it - pick on or beside the wall to cut"
        )));
    }
    Ok((i, q))
}

/// Which ring (index into `loops`) the user's exit point (#1473) ends,
/// with the pick; `Ok(None)` when no exit point is set. Refused by name
/// like the start, and where the exit cannot be honoured: tabs, control
/// compensation, a keep-down level link (the next level descends from
/// the seam) and an overlap beside it (both say where the cut leaves).
pub(super) fn pick_exit(loops: &[&Loop], p: &ContourParams) -> Result<Option<(usize, Point2)>, GenError> {
    let ap = &p.links.approach;
    let Some(q) = ap.exit_point else {
        return Ok(None);
    };
    let at = format!("exit point ({:.3}, {:.3})", q.x, q.y);
    let refuse = |why: String| Err(GenError::Invalid(format!("{at}: {why}")));
    if p.tab_count > 0 {
        return refuse("a picked exit is not taken with tabs yet - clear the exit point or the tabs".into());
    }
    if ap.compensation.in_control() {
        return refuse(format!(
            "a picked exit is not taken with {} compensation yet - clear the exit point or \
             compensate in the computer",
            ap.compensation.label()
        ));
    }
    if p.links.level == crate::links::Link::KeepDown {
        return refuse("a picked exit is not taken under the keep-down level link - the next \
                       level descends from the seam; clear the exit point or change the link"
            .into());
    }
    if ap.overlap > 0.0 {
        return refuse("the overlap and the exit point both say where the cut leaves the ring - \
                       clear one of them"
            .into());
    }
    nearest_ring(loops, q, &at, "exit", p).map(Some)
}

/// One window of a closed ring: the polyline from the seam, the walk,
/// the lead-in's start, the lead-in and the lead-out.
pub(super) type Window = (Vec<Point2>, Vec<Seg>, Point2, Vec<Seg>, Vec<Seg>);

/// The closed ring `poly` (from its seam back to it) carried on along
/// its fitted walk `walk` to the projection of the exit pick `q`,
/// re-tracing the walk's own lines and arcs (splitting a segment when
/// the exit lands inside it so both laps share identical moves), and
/// the leads replanned for the new end. Refused by name, never moved: a
/// lead-out asked for that does not fit at the exit, a lead-in the
/// start had that the longer walk loses, and a lead-out that crosses
/// the lead-in.
pub(super) fn exit_at(
    win: Window,
    q: Point2,
    p: &ContourParams,
    legal: &Legal,
    notes: &mut Vec<String>,
) -> Result<Window, GenError> {
    let (poly, walk, _, lead, _) = &win;
    let Some(hit) = poly.first().and_then(|&start| nearest_on_walk(walk, start, q)) else {
        return Ok(win);
    };
    let f = hit.f;
    let (rpoly, rwalk) = extend_walk(poly, walk, hit);
    let (en, li, lo) = seam_leads(&rwalk, &rpoly, p, legal, notes);
    let (en, li, lo) = extend_leads(en, li, lo, &rwalk, &rpoly, p, legal, notes);
    let at = format!("exit point ({:.3}, {:.3})", f.x, f.y);
    if wanted(p.lead_out_shape()) && lo.is_empty() {
        return Err(GenError::Invalid(format!(
            "{at}: no lead-out fits the legal cutter region there - pick another point on the \
             wall, shorten the lead-out, or clear the exit point"
        )));
    }
    if !lead.is_empty() && li.is_empty() {
        return Err(GenError::Invalid(format!(
            "{at}: the lead-in no longer fits once the cut runs on to the exit - pick another \
             exit point or clear it"
        )));
    }
    if crosses(en, &li, f, &lo) {
        return Err(GenError::Invalid(format!(
            "{at}: the lead-out there crosses the lead-in - pick another exit point or clear it"
        )));
    }
    Ok((rpoly, rwalk, en, li, lo))
}

/// Endpoint snap radius (mm): prevents sub-lattice slivers on split segments.
const SNAP_EPS: f64 = 1e-5;

#[derive(Clone, Copy)]
enum HitKind { Start, End, Interior }

#[derive(Clone, Copy)]
struct WalkHit { seg_idx: usize, sub_idx: usize, f: Point2, d2: f64, kind: HitKind }

fn snap_end(a: Point2, b: Point2, f: Point2) -> (Point2, HitKind) {
    if dist2(f, a) <= SNAP_EPS * SNAP_EPS {
        (a, HitKind::Start)
    } else if dist2(f, b) <= SNAP_EPS * SNAP_EPS {
        (b, HitKind::End)
    } else {
        (f, HitKind::Interior)
    }
}

/// Closest point to `q` on the circular arc from `a` to `b` about `center`.
fn arc_foot(a: Point2, b: Point2, center: Point2, ccw: bool, q: Point2) -> (Point2, f64, HitKind) {
    let (ra, rb) = (dist2(a, center).sqrt(), dist2(b, center).sqrt());
    let (a0, a1) = ((a.y - center.y).atan2(a.x - center.x), (b.y - center.y).atan2(b.x - center.x));
    let sweep = if ccw { a1 - a0 } else { a0 - a1 }.rem_euclid(std::f64::consts::TAU);
    if !(ra.is_finite() && rb.is_finite() && ra > 0.0 && rb > 0.0 && sweep.is_finite() && sweep > 0.0) {
        let (f, kind) = snap_end(a, b, foot(a, b, q).0);
        return (f, dist2(q, f), kind);
    }
    let (qx, qy) = (q.x - center.x, q.y - center.y);
    let end = || if dist2(q, a) <= dist2(q, b) { (a, HitKind::Start) } else { (b, HitKind::End) };
    let (f, kind) = if qx == 0.0 && qy == 0.0 {
        end()
    } else {
        let aq = qy.atan2(qx);
        let ang = if ccw { aq - a0 } else { a0 - aq }.rem_euclid(std::f64::consts::TAU);
        if ang <= sweep {
            let r = ra + (rb - ra) * (ang / sweep);
            snap_end(a, b, Point2::new(center.x + r * aq.cos(), center.y + r * aq.sin()))
        } else {
            end()
        }
    };
    (f, dist2(q, f), kind)
}

/// Closest point to `q` on the fitted walk starting at `start` (ties favour start).
fn nearest_on_walk(walk: &[Seg], start: Point2, q: Point2) -> Option<WalkHit> {
    let (mut at, mut best): (Point2, Option<WalkHit>) = (start, None);
    let mut consider = |hit: WalkHit| {
        if hit.d2.is_finite() && best.is_none_or(|b| hit.d2 < b.d2) { best = Some(hit); }
    };
    for (seg_idx, seg) in walk.iter().enumerate() {
        match seg {
            Seg::Line(qs) => {
                for (sub_idx, &to) in qs.iter().enumerate() {
                    if to == at { continue; }
                    let (f, kind) = snap_end(at, to, foot(at, to, q).0);
                    consider(WalkHit { seg_idx, sub_idx, f, d2: dist2(q, f), kind });
                    at = to;
                }
            }
            Seg::Arc { center, ccw, to } => {
                let (f, d2, kind) = arc_foot(at, *to, *center, *ccw, q);
                consider(WalkHit { seg_idx, sub_idx: 0, f, d2, kind });
                at = *to;
            }
        }
    }
    best
}

/// Extend `poly` and `walk` along `walk` to `hit`, splitting an interior hit
/// on the first lap too so both laps share identical CL moves.
fn extend_walk(poly: &[Point2], walk: &[Seg], hit: WalkHit) -> (Vec<Point2>, Vec<Seg>) {
    let tail = match (walk.get(hit.seg_idx), hit.kind) {
        (Some(Seg::Line(qs)), HitKind::Start) if hit.sub_idx > 0 => Some(Seg::Line(qs[..hit.sub_idx].to_vec())),
        (Some(Seg::Line(qs)), HitKind::Interior) => Some(Seg::Line([&qs[..hit.sub_idx], &[hit.f]].concat())),
        (Some(Seg::Line(qs)), HitKind::End) => Some(Seg::Line(qs[..=hit.sub_idx].to_vec())),
        (Some(Seg::Arc { center, ccw, .. }), HitKind::Interior) => Some(Seg::Arc { center: *center, ccw: *ccw, to: hit.f }),
        (Some(seg @ Seg::Arc { .. }), HitKind::End) => Some(seg.clone()),
        _ => None,
    };
    let mut rwalk = walk.to_vec();
    if matches!(hit.kind, HitKind::Interior) {
        match rwalk.get_mut(hit.seg_idx) {
            Some(Seg::Line(qs)) => qs.insert(hit.sub_idx, hit.f),
            Some(&mut Seg::Arc { center, ccw, to }) => {
                rwalk.splice(hit.seg_idx..=hit.seg_idx, [Seg::Arc { center, ccw, to: hit.f }, Seg::Arc { center, ccw, to }]);
            }
            None => {}
        }
    }
    let mut rpoly = poly.to_vec();
    let mut at = rpoly.last().copied().unwrap_or(hit.f);
    for seg in walk[..hit.seg_idx].iter().chain(&tail) {
        match seg {
            Seg::Line(qs) => {
                let clean: Vec<Point2> = qs.iter().copied().filter(|&q| if q != at { at = q; true } else { false }).collect();
                if !clean.is_empty() {
                    rpoly.extend_from_slice(&clean);
                    rwalk.push(Seg::Line(clean));
                }
            }
            Seg::Arc { center, ccw, to } if *to != at => {
                rwalk.push(Seg::Arc { center: *center, ccw: *ccw, to: *to });
                rpoly.push(*to);
                at = *to;
            }
            Seg::Arc { .. } => {}
        }
    }
    (rpoly, rwalk)
}

/// A lead shape that asks for a lead at all.
fn wanted(l: Lead) -> bool {
    match l {
        Lead::None => false,
        Lead::Linear { length } | Lead::Perpendicular { length } => length > 0.0,
        Lead::Arc { .. } | Lead::Tangential { .. } => true,
    }
}

/// `pts` (a closed ring, last vertex wrapping to the first) re-sequenced
/// to begin at the projection of `q` onto it: the same polygon, one
/// vertex added where the pick lands mid-edge. The ring constructor drops
/// the duplicate when the pick lands on a vertex.
pub(super) fn start_at(pts: &[Point2], q: Point2) -> Vec<Point2> {
    let Some((i, f, _)) = nearest_on_ring(pts, q) else {
        return pts.to_vec();
    };
    let n = pts.len();
    let mut out = Vec::with_capacity(n + 1);
    out.push(f);
    for k in 1..=n {
        out.push(pts[(i + k) % n]);
    }
    out
}

/// A ring's start: the pick's projection for the picked ring; otherwise
/// the automatic seam every earlier file cuts - an axis-aligned start
/// (M11.3 C), the control-compensated mid-run seam (M13.11), then the
/// lead room's own reseat; tabbed levels keep the start their windows are
/// measured from.
pub(super) fn seat(
    pts: Vec<Point2>,
    picked: Option<Point2>,
    tabbed: bool,
    comp: bool,
    p: &ContourParams,
    legal: &Legal,
    notes: &mut Vec<String>,
) -> Vec<Point2> {
    if let Some(q) = picked {
        return start_at(&pts, q);
    }
    let ap = &p.links.approach;
    let pts = if !tabbed && ap.axis_start { axis_reseat(&pts, ap.axis_tol).unwrap_or(pts) } else { pts };
    let pts = if comp && !tabbed { contour_comp::seam_mid_run(&pts) } else { pts };
    if tabbed { pts } else { reseat(&pts, p, legal, notes).unwrap_or(pts) }
}

/// The picked start's lead law: a lead-in was asked for and the lead law
/// (`lead_fit`, shrinks included) found none that fits at the pick - the
/// automatic seam would move elsewhere, a pick may not, so it is refused
/// by name. `at` is the seam the pick projected to.
pub(super) fn lead_held(p: &ContourParams, at: Point2, lead: &[Seg]) -> Result<(), GenError> {
    if wanted(p.lead) && lead.is_empty() {
        return Err(GenError::Invalid(format!(
            "start point ({:.3}, {:.3}): no lead-in fits the legal cutter region there - pick \
             another point on the wall, shorten the lead-in, or clear the start point",
            at.x, at.y
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sq() -> Vec<Point2> {
        vec![Point2::new(0.0, 0.0), Point2::new(10.0, 0.0), Point2::new(10.0, 10.0), Point2::new(0.0, 10.0)]
    }

    #[test]
    fn a_mid_edge_pick_inserts_its_foot_and_keeps_the_polygon() {
        let v = start_at(&sq(), Point2::new(12.0, 4.0));
        assert_eq!(v[0], Point2::new(10.0, 4.0));
        assert_eq!(&v[1..], &[Point2::new(10.0, 10.0), Point2::new(0.0, 10.0), Point2::new(0.0, 0.0), Point2::new(10.0, 0.0)]);
    }

    #[test]
    fn a_vertex_pick_starts_on_that_vertex() {
        let v = start_at(&sq(), Point2::new(11.0, 11.0));
        assert_eq!(v[0], Point2::new(10.0, 10.0));
        assert_eq!(v[1], Point2::new(10.0, 10.0), "the duplicate the ring constructor drops");
        assert_eq!(v.len(), 5);
    }
}
