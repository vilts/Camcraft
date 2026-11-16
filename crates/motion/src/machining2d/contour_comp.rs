//! Control-side cutter compensation for the contour (M13.11): the
//! door that names what G41/G42 needs of the parameters, and the span
//! builder that records, over a ring already walked at cutter centre,
//! the PART contour the post writes under G41/G42 instead.
//!
//! Provenance: RS-274 / ISO 6983 G41 / G42 / G40 and the published
//! startup / cancel block behaviour of Fanuc-style controls - the offset
//! vector is built over the straight block carrying G41/G42 (its end
//! point lands one radius off the next block's line) and withdrawn over
//! the straight block carrying G40, so both must be linear and at least
//! a radius long; between them the control offsets the programmed
//! contour by the register. The centre moves the generator proved stay
//! the path's truth (the sim and the verifier read them); the
//! programmed geometry rides beside them in [`CompSpan`]. The wear
//! variant (#38, the handbooks' wear-offset practice) programs those
//! centre moves themselves under G41/G42, the register holding only
//! the wear delta: the same engage / cancel blocks, the same lead law.
//! Our own model; no vendor material consulted.

use crate::cl::{CompSide, CompSpan, Move, MoveKind, PathBuilder};
use crate::geo2d::{Point2, Region};
use crate::geometry::Point3;
use crate::links::{Compensation, Link};

use super::arcs::{fit_arcs_sided, Seg};
use super::contour_comp_room as room;
use super::contour::{ContourParams, Lead, Side};
use super::face::GenError;
use super::ring::dist;

pub(super) fn refused(why: impl std::fmt::Display) -> GenError {
    GenError::Invalid(format!("control compensation {why}"))
}

/// A refusal named for the mode `p` asks: "control compensation ..."
/// or "wear compensation ...".
pub(super) fn refused_as(p: &ContourParams, why: impl std::fmt::Display) -> GenError {
    GenError::Invalid(format!("{} compensation {why}", p.links.approach.compensation.label()))
}

fn straight(lead: Lead) -> Option<f64> {
    match lead {
        Lead::Linear { length } | Lead::Perpendicular { length } => Some(length),
        _ => None,
    }
}

/// Does `p` ask the control to compensate (control or wear)?
pub(super) fn wanted(p: &ContourParams) -> bool {
    p.links.approach.compensation.in_control()
}

/// Does `p` ask for the wear variant?
fn wear(p: &ContourParams) -> bool {
    p.links.approach.compensation == Compensation::Wear
}

/// The door: every refusal by name, before any geometry is touched.
pub(super) fn door(p: &ContourParams) -> Result<(), GenError> {
    if !wanted(p) {
        return Ok(());
    }
    let r = p.tool_diameter * 0.5;
    for (end, lead) in [("in", p.lead), ("out", p.lead_out_shape())] {
        match straight(lead) {
            None => {
                return Err(refused_as(p, format!(
                    "needs a straight lead-{end} (Linear or Perpendicular) of at least the tool radius - \
                     the control builds the offset over that move, and an arc or no lead cannot carry G41/G42"
                )))
            }
            Some(len) if len + 1e-9 < r => {
                return Err(refused_as(p, format!(
                    "needs a straight lead-{end} of at least the tool radius ({r:.3} mm) - the lead-{end} is {len:.3} mm"
                )))
            }
            Some(_) => {}
        }
    }
    if p.tab_count > 0 {
        return Err(refused_as(p, "does not ride tabs - a tab window has no lead to engage on"));
    }
    if p.blend_turns {
        return Err(refused_as(p, "does not ride corner blends - the control rounds the offset corners itself"));
    }
    if p.links.approach.overlap > 0.0 {
        return Err(refused_as(p, "does not ride an overlap past the start - the cancel block must leave from the ring's seam"));
    }
    if p.links.level == Link::KeepDown || p.links.pass == Link::KeepDown {
        return Err(refused_as(p, "needs Clearance or Retract links - a keep-down link engages the next cut without a lead"));
    }
    Ok(())
}

/// The contour the post programs: the region at the stock to leave
/// (the control adds the radius). `None` when the parameters do not
/// ask the control.
pub(super) fn programmed_region(region: &Region, p: &ContourParams) -> Result<Option<Region>, GenError> {
    if !wanted(p) {
        return Ok(None);
    }
    // Wear programs the centre walk: the region only marks the ring as
    // compensated (its seam re-seats mid-run like control's).
    if wear(p) || p.stock_to_leave <= 0.0 {
        return Ok(Some(region.clone()));
    }
    let d = if p.side == Side::Inside { -p.stock_to_leave } else { p.stock_to_leave };
    let prog = region.offset(d)?;
    if prog.is_empty() {
        return Err(refused("found no part contour once the stock to leave was taken off"));
    }
    Ok(Some(prog))
}

/// The ring restarted at the midpoint of its longest straight edge:
/// the seam a control-compensated ring needs. At a vertex seam the
/// offset the control builds over the engage block (square to the
/// first cut) and the one it withdraws over the cancel block (square
/// to the last) differ, so the tool would leave the ring off the
/// proved centre path; mid-run the two coincide and the engage,
/// the offset walk and the cancel reproduce the centre moves exactly.
pub(super) fn seam_mid_run(pts: &[Point2]) -> Vec<Point2> {
    let n = pts.len();
    if n < 3 {
        return pts.to_vec();
    }
    let mut best = (0usize, -1.0f64);
    for i in 0..n {
        let d = dist(pts[i], pts[(i + 1) % n]);
        if d > best.1 {
            best = (i, d);
        }
    }
    let (i, _) = best;
    let (a, b) = (pts[i], pts[(i + 1) % n]);
    let mut v = Vec::with_capacity(n + 1);
    v.push(Point2::new((a.x + b.x) * 0.5, (a.y + b.y) * 0.5));
    v.extend((1..=n).map(|k| pts[(i + k) % n]));
    v
}

/// The closest point of `prog`'s boundary to `q`: (loop index, edge
/// index, the point, the distance).
fn closest(prog: &Region, q: Point2) -> Option<(usize, usize, Point2, f64)> {
    let mut best: Option<(usize, usize, Point2, f64)> = None;
    for (li, l) in prog.loops().iter().enumerate() {
        let pts = l.points();
        let n = pts.len();
        for i in 0..n {
            let (a, b) = (pts[i], pts[(i + 1) % n]);
            let (dx, dy) = (b.x - a.x, b.y - a.y);
            let len2 = dx * dx + dy * dy;
            let t = if len2 > 0.0 { (((q.x - a.x) * dx + (q.y - a.y) * dy) / len2).clamp(0.0, 1.0) } else { 0.0 };
            let c = Point2::new(a.x + dx * t, a.y + dy * t);
            let d = dist(c, q);
            if best.is_none_or(|(_, _, _, bd)| d < bd) {
                best = Some((li, i, c, d));
            }
        }
    }
    best
}

/// What a span needs from the walk it replaces.
pub(super) struct SpanCtx<'a> {
    pub prog: &'a Region,
    pub p: &'a ContourParams,
}

/// Record the span over the moves `first..=last` of `b` - `first` the
/// lead-in move that lands on the wall, `last` the lead-out move that
/// leaves it, the closed ring walked between them at one Z - as the
/// programmed geometry: the same lead-in ending on the part contour,
/// the part contour around (arcs fitted at the op's tolerance), the
/// same lead-out leaving from it. Every refusal names its place.
pub(super) fn push_span(b: &mut PathBuilder, ctx: &SpanCtx, first: usize, last: usize) -> Result<(), GenError> {
    let p = ctx.p;
    let r = p.tool_diameter * 0.5;
    let moves = b.moves();
    let (Some(m_in), Some(m_out)) = (moves.get(first), moves.get(last)) else {
        return Err(refused_as(p, "found no lead-in or lead-out move to engage on"));
    };
    if first >= last || m_in.arc.is_some() || m_out.arc.is_some() {
        return Err(refused_as(p, "needs straight lead moves to engage and cancel on"));
    }
    let (m_in, m_out) = (*m_in, *m_out);
    let xy = |q: Point3| Point2::new(q.x, q.y);
    let (s_c, e_c) = (xy(m_in.to), xy(m_out.from));
    let z = m_in.to.z;
    let tol = p.tolerance.max(1e-6);
    for (end, m) in [("in", m_in), ("out", m_out)] {
        let len = dist(xy(m.from), xy(m.to));
        if len + 1e-9 < r {
            return Err(refused_as(p, format!(
                "needs a straight lead-{end} of at least the tool radius ({r:.3} mm) at ({:.3}, {:.3}) - the room law left {len:.3} mm",
                s_c.x, s_c.y
            )));
        }
    }
    if dist(s_c, e_c) > 1e-6 {
        return Err(refused_as(p, format!("needs a closed ring - the walk from ({:.3}, {:.3}) does not return to its seam", s_c.x, s_c.y)));
    }
    if moves[first + 1..last].iter().any(|m| m.from.z != z || m.to.z != z || m.kind != MoveKind::Feed) {
        return Err(refused_as(p, "needs one planar feed walk between its leads"));
    }
    if wear(p) {
        return push_wear(b, p, first, last, s_c);
    }
    let Some((li, ei, s_p, d)) = closest(ctx.prog, s_c) else {
        return Err(refused("found no part contour to program"));
    };
    // The room law (#40): the control must be able to offset this ring
    // by the register - inside corners and necks are judged here, by name.
    if let Some(l) = ctx.prog.loops().get(li) {
        room::corners(l, p)?;
        room::neck(l, p)?;
    }
    // The seam law: the ring's first and last cuts are straight and
    // the centre stands square to both at the seam, so the offset the
    // control builds on the engage block is the one it withdraws on
    // the cancel block - and the tool runs the proved centre moves.
    let off = Point2::new(s_c.x - s_p.x, s_c.y - s_p.y);
    for (which, m) in [("first", moves[first + 1]), ("last", moves[last - 1])] {
        let t = Point2::new(m.to.x - m.from.x, m.to.y - m.from.y);
        let len = t.x.hypot(t.y);
        let square = len > 0.0 && (t.x * off.x + t.y * off.y).abs() / len <= tol + 0.02;
        if m.arc.is_some() || !square {
            return Err(refused(format!(
                "needs the ring seam on a straight run - at ({:.3}, {:.3}) the {which} cut is not square to the offset",
                s_c.x, s_c.y
            )));
        }
    }
    // The programmed contour already carries the stock to leave, so
    // the centre stands exactly one radius off it.
    let want = r;
    if (d - want).abs() > tol + 0.02 {
        return Err(refused(format!(
            "found the part contour {d:.3} mm from the cutter centre at ({:.3}, {:.3}), not the tool radius {want:.3} mm",
            s_c.x, s_c.y
        )));
    }
    let Some(l) = ctx.prog.loops().get(li) else {
        return Err(refused("lost its loop"));
    };
    let pts = l.points();
    let n = pts.len();
    if n < 3 {
        return Err(refused("needs a ring of at least three points"));
    }
    // Forward: s_p -> pts[ei+1] -> ... -> pts[ei] -> s_p. Reversed when
    // the centre walk leaves its seam the other way.
    let t_c = moves.get(first + 1).map(|m| Point2::new(m.to.x - m.from.x, m.to.y - m.from.y));
    let fwd: Vec<Point2> = (1..=n).map(|k| pts[(ei + k) % n]).collect();
    let rev: Vec<Point2> = (0..n).map(|k| pts[(ei + n - k) % n]).collect();
    let first_dir = |v: &[Point2]| v.iter().find(|q| dist(**q, s_p) > 1e-9).map(|q| Point2::new(q.x - s_p.x, q.y - s_p.y));
    let along = match (t_c, first_dir(&fwd)) {
        (Some(t), Some(f)) => t.x * f.x + t.y * f.y,
        _ => 1.0,
    };
    let ring = if along >= 0.0 { fwd } else { rev };
    let mut poly = Vec::with_capacity(n + 2);
    poly.push(s_p);
    for q in ring {
        if poly.last().is_none_or(|l| dist(*l, q) > 1e-9) {
            poly.push(q);
        }
    }
    if poly.last().is_none_or(|l| dist(*l, s_p) > 1e-9) {
        poly.push(s_p);
    }
    // #1579: the control cuts this part contour offset by its register,
    // so an arc of it that passes a vertex on the part side cuts past the
    // wall just as the centre walk's would; it walks the centre ring's
    // way round, so the air is on the same side (`climb`).
    let walk = fit_arcs_sided(&poly, p.tolerance, Some(p.climb));
    room::arcs(&walk, s_p, p)?;
    let feed = moves.get(first + 1).and_then(|m| m.feed).unwrap_or(p.feed);
    let mut programmed = Vec::with_capacity(walk.len() + 2);
    let s3 = Point3::new(s_p.x, s_p.y, z);
    programmed.push(Move::new(MoveKind::Feed, m_in.from, s3, m_in.feed));
    let mut at = s3;
    for seg in &walk {
        match seg {
            Seg::Line(qs) => {
                for q in qs {
                    let to = Point3::new(q.x, q.y, z);
                    if to != at {
                        programmed.push(Move::new(MoveKind::Feed, at, to, Some(feed)));
                        at = to;
                    }
                }
            }
            Seg::Arc { center, ccw, to } => {
                let to = Point3::new(to.x, to.y, z);
                if to != at {
                    programmed.push(Move::new(MoveKind::Feed, at, to, Some(feed)).with_arc(*center, *ccw));
                    at = to;
                }
            }
        }
    }
    if at != s3 {
        programmed.push(Move::new(MoveKind::Feed, at, s3, Some(feed)));
    }
    programmed.push(Move::new(MoveKind::Feed, s3, m_out.to, m_out.feed));
    let span = CompSpan { side: side_of(p), radius: r, wear: false, first, last, programmed };
    if !span.is_sane(b.moves()) {
        return Err(refused(format!("could not chain its programmed contour at ({:.3}, {:.3})", s_c.x, s_c.y)));
    }
    b.push_comp(span);
    Ok(())
}

/// Climb travel keeps the material on the right of the cutter (M3,
/// #1555), so the centre stands left of the part contour - G41 - in
/// control and in wear alike; conventional is G42.
fn side_of(p: &ContourParams) -> CompSide {
    if p.climb {
        CompSide::Left
    } else {
        CompSide::Right
    }
}

/// The wear span (#38): the programmed moves ARE the centre moves
/// `first..=last` - the engage lead, the centre ring, the cancel lead -
/// so the control, its register holding the wear delta, runs the
/// proved path shifted by that delta and nothing more.
fn push_wear(b: &mut PathBuilder, p: &ContourParams, first: usize, last: usize, s_c: Point2) -> Result<(), GenError> {
    let programmed: Vec<Move> = b.moves().get(first..=last).map(<[Move]>::to_vec).unwrap_or_default();
    let span = CompSpan { side: side_of(p), radius: p.tool_diameter * 0.5, wear: true, first, last, programmed };
    if !span.is_sane(b.moves()) {
        return Err(refused_as(p, format!("could not chain its centre walk at ({:.3}, {:.3})", s_c.x, s_c.y)));
    }
    b.push_comp(span);
    Ok(())
}
