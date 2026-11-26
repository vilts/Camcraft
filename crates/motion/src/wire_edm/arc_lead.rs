//! The tangent arc lead (#3991): an opt-in lead-in and lead-out for the
//! analytic wire contour (`arc_cut`). The straight lead stays the default.
//!
//! - **Lead-in.** A straight move from the start hole to the point where
//!   it touches the lead circle, then an arc on that circle to the entry
//!   point. The lead circle has the stated radius, its centre on the waste
//!   side of the entry point, so it touches the wire path there: the wire
//!   arrives in the path's own direction and does not stop at the entry.
//!   The straight is tangent to the circle too, so the lead has no corner.
//! - **Lead-out.** The mirror: from the exit of the lap (the same entry
//!   point) on along the lead circle that touches the path's end tangent,
//!   then a straight tangent move back to the start hole.
//! - **Radius rule.** The stated radius, never changed. It must be larger
//!   than the kerf offset (a lead arc tighter than the wire's own burn
//!   radius is no smoother than a corner). The start hole must stand
//!   outside the lead circle, so that the straight move has a length. The
//!   lead circle's centre is written on the CL lattice; its radius is then
//!   the distance from that centre to the entry point (within 1e-6 mm of
//!   the stated radius), so the arc meets the path at the entry exactly.
//! - **Refusals.** A radius not over the kerf offset, a hole inside or on
//!   the lead circle, and a lead that would come nearer the profile than
//!   the kerf offset (an arc that crosses the profile or the waste-side
//!   clearance) - each refused by name. The radius is never clamped, and
//!   the cut never falls back to the straight lead silently.
//!
//! Provenance: our own implementation. Plane geometry from any analytic
//! geometry text: the circle that touches a line at a given point, and
//! the tangents from an outside point to a circle. RS-274 / ISO 6983
//! circular interpolation for the arc blocks.

use std::f64::consts::TAU;

use crate::cl::lattice;
use crate::geo2d::Point2;
use crate::geometry::Point3;
use crate::machining2d::GenError;

use super::arc_chain::ArcChain;
use super::arc_dist::{nearest, span_span};
use super::arc_span::{add, cross, dist, dot, left, mul, sub, unit, Span, ANG_EPS};
use super::{MIN_LEAD, PATH_TOL};

/// How the wire is led from the start hole onto the path and back.
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub enum WireLead {
    /// A straight lead to the path's nearest point - the default.
    #[default]
    Straight,
    /// A tangent arc lead-in and lead-out of this radius (mm).
    Arc { radius: f64 },
}

/// The lead-in and lead-out spans of a tangent arc lead.
pub(super) struct ArcLeads {
    pub lead_in: Vec<Span>,
    pub lead_out: Vec<Span>,
    /// The radius the arcs are cut at (the stated one, to the lattice).
    pub radius: f64,
    /// The straight part of the lead-in, mm.
    pub straight: f64,
}

fn invalid(why: String) -> GenError {
    GenError::Invalid(format!("wire contour: {why}"))
}

fn rotate(u: Point2, a: f64) -> Point2 {
    let (s, c) = a.sin_cos();
    Point2::new(u.x * c - u.y * s, u.x * s + u.y * c)
}

/// The turn (rad, in (0, 2pi)) from `a` to `b` about `c` in sense `ccw`.
fn sweep(c: Point2, a: Point2, b: Point2, ccw: bool) -> f64 {
    let (u, v) = (sub(a, c), sub(b, c));
    let t = cross(u, v).atan2(dot(u, v));
    (if ccw { t } else { -t }).rem_euclid(TAU)
}

/// The lead circle that touches the path at `at` (walk direction `dir`)
/// on the waste side: its lattice centre and radius.
fn circle(at: Point2, dir: Point2, waste_left: bool, radius: f64) -> Point2 {
    let n = if waste_left { left(dir) } else { mul(left(dir), -1.0) };
    let c = add(at, mul(n, radius));
    Point2::new(lattice(c.x), lattice(c.y))
}

/// The point where the tangent from `hole` touches the circle (`c`, `r`),
/// on the side where the circle is walked in sense `ccw` away from the hole
/// (`into` true: the walk on the circle continues the straight from the
/// hole) or towards it (`into` false: the straight to the hole continues
/// the walk). `None` when the hole is not outside the circle.
fn touch(hole: Point2, c: Point2, r: f64, ccw: bool, into: bool) -> Option<(Point2, f64)> {
    let d = dist(hole, c);
    let straight = (d * d - r * r).sqrt();
    if !(straight.is_finite() && straight > MIN_LEAD && d > r) {
        return None;
    }
    let u = unit(sub(hole, c))?;
    let alpha = (r / d).acos();
    [alpha, -alpha]
        .into_iter()
        .map(|a| add(c, mul(rotate(u, a), r)))
        .find(|&q| {
            let walk = mul(left(sub(q, c)), if ccw { 1.0 } else { -1.0 });
            let line = if into { sub(q, hole) } else { sub(hole, q) };
            dot(walk, line) > 0.0
        })
        .map(|q| (q, straight))
}

/// The tangent arc lead onto the closed walk `seq` (cut from its first
/// span's start to its last span's end) from `hole`, on the waste side
/// (`waste_left`: left of the walk), at `radius`.
///
/// # Errors
/// Each refusal of the module head, by name.
pub(super) fn arc_leads(
    hole: Point2,
    seq: &[Span],
    waste_left: bool,
    radius: f64,
    chain: &ArcChain,
    k: f64,
    z: f64,
) -> Result<ArcLeads, GenError> {
    if !(radius.is_finite() && radius > k) {
        return Err(invalid(format!(
            "the tangent lead arc radius ({radius} mm) must be larger than the kerf offset ({k:.4} mm) - set a larger radius or the straight lead"
        )));
    }
    let (Some(first), Some(last)) = (seq.first(), seq.last()) else {
        return Err(invalid("the wire path is empty".into()));
    };
    let (Some(t_in), Some(t_out)) = (first.dir_start(), last.dir_end()) else {
        return Err(invalid("the wire path has no direction at its entry point - refused".into()));
    };
    let ccw = waste_left;
    let too_near = |at: Point2| {
        GenError::invalid_at(
            format!(
                "wire contour: the start hole at X{:.3} Y{:.3} stands too near the entry point X{:.3} Y{:.3} for a tangent lead arc of radius {radius} mm - the hole must stand outside the lead circle (on the normal: more than {:.3} mm from the path); set a smaller radius, move the hole or use the straight lead",
                hole.x,
                hole.y,
                at.x,
                at.y,
                2.0 * radius
            ),
            Point3::new(hole.x, hole.y, z),
        )
    };
    let (p_in, p_out) = (first.start(), last.end());
    let c_in = circle(p_in, t_in, waste_left, radius);
    let r_in = dist(c_in, p_in);
    let (a, straight) = touch(hole, c_in, r_in, ccw, true).ok_or_else(|| too_near(p_in))?;
    let c_out = circle(p_out, t_out, waste_left, radius);
    let r_out = dist(c_out, p_out);
    let (b, _) = touch(hole, c_out, r_out, ccw, false).ok_or_else(|| too_near(p_out))?;
    let mut lead_in = vec![Span::Line { a: hole, b: a }];
    let s_in = sweep(c_in, a, p_in, ccw);
    if s_in > ANG_EPS {
        lead_in.extend(Span::Arc { c: c_in, r: r_in, a, b: p_in, ccw, sweep: s_in }.halves());
    }
    let mut lead_out = Vec::new();
    let s_out = sweep(c_out, p_out, b, ccw);
    if s_out > ANG_EPS {
        lead_out.extend(Span::Arc { c: c_out, r: r_out, a: p_out, b, ccw, sweep: s_out }.halves());
    }
    lead_out.push(Span::Line { a: b, b: hole });
    for s in lead_in.iter().chain(&lead_out) {
        clear(s, chain, k, radius, z)?;
    }
    Ok(ArcLeads { lead_in, lead_out, radius: r_in, straight })
}

/// One lead span stands at least the kerf offset off every span of the
/// profile, or the lead is refused by name.
fn clear(s: &Span, chain: &ArcChain, k: f64, radius: f64, z: f64) -> Result<(), GenError> {
    for cs in chain.spans() {
        let d = span_span(s, cs);
        if d.is_nan() || d < k - PATH_TOL {
            let (_, q) = nearest(s.point_at(0.5), cs);
            return Err(GenError::invalid_at(
                format!(
                    "wire contour: the tangent lead of radius {radius} mm would bring the wire within {d:.4} mm of the profile near X{:.3} Y{:.3} - under the kerf offset ({k:.4} mm), the lead would cross the profile or burn the product; set a smaller radius, move the start hole or use the straight lead",
                    q.x, q.y
                ),
                Point3::new(q.x, q.y, z),
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
#[path = "arc_lead_tests.rs"]
mod tests;
