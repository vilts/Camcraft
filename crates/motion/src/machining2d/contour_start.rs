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

/// The closest point of segment `a`-`b` to `q`, and its squared distance.
fn foot(a: Point2, b: Point2, q: Point2) -> (Point2, f64) {
    let (dx, dy) = (b.x - a.x, b.y - a.y);
    let dd = dx * dx + dy * dy;
    let t = if dd > 0.0 { (((q.x - a.x) * dx + (q.y - a.y) * dy) / dd).clamp(0.0, 1.0) } else { 0.0 };
    let f = Point2::new(a.x + dx * t, a.y + dy * t);
    (f, (q.x - f.x) * (q.x - f.x) + (q.y - f.y) * (q.y - f.y))
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
/// farther than [`reach`] from every ring, and the cases this first slice
/// does not certify - tabs (their windows are measured from the automatic
/// start) and compensation in the control (its seam is the comp law's).
pub(super) fn pick_ring(loops: &[&Loop], p: &ContourParams) -> Result<Option<(usize, Point2)>, GenError> {
    let ap = &p.links.approach;
    let Some(q) = ap.start_point else {
        return Ok(None);
    };
    let at = format!("start point ({:.3}, {:.3})", q.x, q.y);
    if p.tab_count > 0 {
        return Err(GenError::Invalid(format!(
            "{at}: a picked start is not taken with tabs yet - clear the start point or the tabs"
        )));
    }
    if ap.compensation.in_control() {
        return Err(GenError::Invalid(format!(
            "{at}: a picked start is not taken with {} compensation yet - its seam is the \
             compensation law's; clear the start point or compensate in the computer",
            ap.compensation.label()
        )));
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
/// its own polyline to the projection of the exit pick `q`, the walk
/// extended by the same chords - the cutter-centre ring itself, so no
/// new crossing - and the leads replanned for the new end. Refused by
/// name, never moved: a lead-out asked for that does not fit at the
/// exit, a lead-in the start had that the longer walk loses, and a
/// lead-out that crosses the lead-in.
pub(super) fn exit_at(
    win: Window,
    q: Point2,
    p: &ContourParams,
    legal: &Legal,
    notes: &mut Vec<String>,
) -> Result<Window, GenError> {
    let (poly, walk, _, lead, _) = &win;
    let Some((i, f)) = nearest_on_run(poly, q) else {
        return Ok(win);
    };
    let mut run: Vec<Point2> = poly.iter().skip(1).take(i).copied().collect();
    if run.last().copied().unwrap_or(poly[0]) != f {
        run.push(f);
    }
    let (mut rpoly, mut rwalk) = (poly.clone(), walk.clone());
    if !run.is_empty() {
        rpoly.extend_from_slice(&run);
        rwalk.push(Seg::Line(run));
    }
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

/// The closest point to `q` on the open polyline `pts`: `(segment
/// index, point)`, the first segment winning a tie (so an exit on the
/// seam ends at the seam, adding nothing).
fn nearest_on_run(pts: &[Point2], q: Point2) -> Option<(usize, Point2)> {
    let mut best: Option<(usize, Point2, f64)> = None;
    for (i, w) in pts.windows(2).enumerate() {
        let (f, d2) = foot(w[0], w[1], q);
        if d2.is_finite() && best.is_none_or(|b| d2 < b.2) {
            best = Some((i, f, d2));
        }
    }
    best.map(|(i, f, _)| (i, f))
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
