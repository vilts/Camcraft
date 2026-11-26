//! The parallel of an analytic wire profile at the kerf offset (#3937):
//! each line moved along its normal, each arc made concentric, and every
//! corner closed - a pivot arc about the corner where the two parallels
//! part, a trim at their meet where they overlap. A parallel that a trim
//! consumes, or an arc tighter than the offset on its waste side, is
//! refused by name: nothing is clamped or rounded.
//!
//! Provenance: the offset-curve construction for line and arc profiles
//! (Tiller and Hanson, "Offsets of two-dimensional profiles", IEEE CG&A
//! 1984; M. Held, "On the Computational Geometry of Pocket Machining",
//! Springer 1991: parallel elements, joining arcs at convex vertices,
//! trimming at concave ones). Our own implementation.

use crate::geo2d::Point2;
use crate::geometry::Point3;
use crate::machining2d::GenError;

use super::arc_chain::JOINT_TOL;
use super::arc_dist::{carriers_meet, carriers_touch};
use super::arc_span::{cross, dist, dot, left, mul, add, Parallel, Span};

/// Two unit tangents whose cross product is under this meet smoothly: no
/// pivot and no trim, the parallels already share their end.
const SMOOTH: f64 = 1e-7;

fn refuse(msg: String, at: Point2) -> GenError {
    GenError::invalid_at(format!("wire contour: {msg}"), Point3::new(at.x, at.y, 0.0))
}

/// One parallel element, with its source span and whether a trim moved
/// its start or end.
struct Piece {
    span: Span,
    src: usize,
    start: Point2,
    end: Point2,
    start_moved: bool,
    end_moved: bool,
}

/// The parallel of the counter-clockwise `spans` at signed offset `s`
/// (positive: inside, to the left of the walk). `sharp` lets an arc
/// tighter than the offset drop out to a corner (#3207); without it that
/// arc is refused. Returns the closed walk of lines and arcs, and the
/// widest miss (mm) where two near-tangent parallels were joined across a
/// gap instead of at a crossing (at most [`JOINT_TOL`]).
pub(super) fn parallel_walk(spans: &[Span], s: f64, sharp: bool) -> Result<(Vec<Span>, f64), GenError> {
    let mut miss: f64 = 0.0;
    let k = s.abs();
    let mut pieces: Vec<Piece> = Vec::with_capacity(spans.len());
    for (i, sp) in spans.iter().enumerate() {
        match sp.parallel(s) {
            Some(Parallel::Span(o)) => pieces.push(Piece { span: o, src: i, start: o.start(), end: o.end(), start_moved: false, end_moved: false }),
            Some(Parallel::Point) => {}
            Some(Parallel::Inverted(by)) => {
                if !sharp {
                    let Span::Arc { r, .. } = *sp else { continue };
                    let at = sp.point_at(0.5);
                    return Err(refuse(
                        format!(
                            "an inside arc of radius {r:.4} mm near X{:.3} Y{:.3} is tighter than the wire can follow on this side - the minimum inside radius is {k:.4} mm (wire radius + spark gap), {by:.4} mm more than the arc; open the radius or cut with a thinner wire",
                            at.x, at.y
                        ),
                        at,
                    ));
                }
            }
            None => return Err(refuse(format!("span {} of the profile has no direction", i + 1), sp.start())),
        }
    }
    if pieces.is_empty() {
        let at = spans.first().map_or(Point2::new(0.0, 0.0), Span::start);
        return Err(refuse(format!("the profile is too small for the wire - no point of it stands the kerf offset ({k:.4} mm) from its edge"), at));
    }
    let n = pieces.len();
    let mut pivots: Vec<Option<Span>> = vec![None; n];
    for i in 0..n {
        let j = (i + 1) % n;
        let (oi, oj) = (pieces[i].span, pieces[j].span);
        let (Some(t1), Some(t2)) = (oi.dir_end(), oj.dir_start()) else {
            return Err(refuse("a parallel of the profile has no direction".into(), oi.end()));
        };
        let (e, f) = (pieces[i].end, pieces[j].start);
        let turn = cross(t1, t2);
        let consecutive = (pieces[i].src + 1) % spans.len() == pieces[j].src;
        if dist(e, f) <= 1e-9 || (turn.abs() <= SMOOTH && dot(t1, t2) > 0.0) {
            continue;
        }
        if turn.abs() <= SMOOTH {
            return Err(refuse("the profile turns back on itself at a joint - there is no side for the wire there".into(), e));
        }
        if turn * s < 0.0 {
            // The parallels part: the wire pivots about the drawing's corner.
            let p = spans[pieces[i].src].end();
            if !consecutive || n == 1 {
                return Err(refuse("an outside corner follows an arc too tight for the wire - the parallel cannot be closed there".into(), p));
            }
            let b = add(p, mul(left(t2), s));
            let a = add(p, mul(left(t1), s));
            let sweep = turn.abs().atan2(dot(t1, t2));
            pivots[i] = Some(Span::Arc { c: p, r: k, a, b, ccw: s < 0.0, sweep });
            continue;
        }
        // The parallels overlap: both end at their meet nearest the corner.
        let mid = mul(add(e, f), 0.5);
        let meet = carriers_meet(&oi, &oj).into_iter().min_by(|a, b| dist(*a, mid).total_cmp(&dist(*b, mid)));
        // Near-tangent parallels that just miss (a drawing's joint gap):
        // joined across the miss, which the path's margin then holds.
        let touch = || carriers_touch(&oi, &oj).filter(|(_, g)| *g <= JOINT_TOL && dist(e, f) <= 10.0 * JOINT_TOL);
        let Some(x) = meet.or_else(|| {
            touch().map(|(x, g)| {
                miss = miss.max(g);
                x
            })
        }) else {
            return Err(refuse(
                format!("the parallels of the profile at the kerf offset ({k:.4} mm) do not meet at the inside corner near X{:.3} Y{:.3} - a feature there is narrower than the kerf", e.x, e.y),
                e,
            ));
        };
        pieces[i].end = x;
        pieces[i].end_moved = true;
        pieces[j].start = x;
        pieces[j].start_moved = true;
    }
    let mut out = Vec::with_capacity(2 * n);
    for (i, pc) in pieces.iter().enumerate() {
        let Some(sp) = pc.span.between(pc.start, pc.end, pc.start_moved, pc.end_moved, 10.0 * JOINT_TOL) else {
            let at = pc.span.point_at(0.5);
            return Err(refuse(
                format!(
                    "the profile narrows near X{:.3} Y{:.3} under the kerf width ({:.4} mm) - the wire's parallel there is consumed by its neighbours; widen the feature or cut with a thinner wire",
                    at.x,
                    at.y,
                    2.0 * k
                ),
                at,
            ));
        };
        out.push(sp);
        if let Some(pv) = pivots[i] {
            out.push(pv);
        }
    }
    Ok((out, miss))
}
