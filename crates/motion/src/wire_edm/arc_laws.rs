//! The wire contour's two laws (`laws`) read against the analytic profile
//! (#3937): the path's moves - lines and arcs as the toolpath records them,
//! each arc about its lattice centre - against the drawing's own lines and
//! arcs, never against a chord list of either.
//!
//! - **Kerf law.** The least distance from each move to each span of the
//!   profile, exact (`arc_dist`), is the kerf offset less [`PATH_TOL`] or
//!   more. A recorded arc whose two end radii differ (a spiral) is read
//!   as the circle of their mean radius, and half the difference is taken
//!   off the reading.
//! - **Cover law.** Every sampled point of the profile stands within the
//!   offset plus [`COVER_TOL`] (or the stated sharp corners' reach,
//!   #3207) of the path; the distance to a move is exact.
//!
//! Provenance: plain 2D distance geometry over the rolling-circle reading
//! of a wire cut (module `laws`). Our own implementation.

use std::f64::consts::TAU;

use crate::cl::{Move, Toolpath};
use crate::geo2d::Point2;
use crate::geometry::Point3;
use crate::machining2d::GenError;

use super::arc_chain::ArcChain;
use super::arc_dist::{nearest, pt_span, span_span};
use super::arc_span::{cross, dist, dot, sub, Span};
use super::laws::{sharp_reach, uncovered, UncutRest};
use super::{WireParams, COVER_TOL, PATH_TOL};

/// The most points the cover law samples along the profile.
const MAX_SAMPLES: f64 = 40_000.0;

/// A recorded move as a span, with how far (mm) the recorded curve may
/// stand from that span. `None` for a move that is not a flat XY move.
pub(super) fn move_span(m: &Move) -> Option<(Span, f64)> {
    let (a, b) = (Point2::new(m.from.x, m.from.y), Point2::new(m.to.x, m.to.y));
    let Some(arc) = m.arc else {
        return (dist(a, b) > 0.0).then_some((Span::Line { a, b }, 0.0));
    };
    if !arc.plane.is_xy() {
        return None;
    }
    let c = arc.center;
    let (ra, rb) = (dist(a, c), dist(b, c));
    if !(ra > 0.0 && rb > 0.0) {
        return None;
    }
    let (u, v) = (sub(a, c), sub(b, c));
    let t = cross(u, v).atan2(dot(u, v));
    let sweep = if arc.ccw { t } else { -t }.rem_euclid(TAU);
    let sweep = if sweep <= 0.0 { TAU } else { sweep };
    Some((Span::Arc { c, r: 0.5 * (ra + rb), a, b, ccw: arc.ccw, sweep }, 0.5 * (ra - rb).abs()))
}

fn spans_of(path: &Toolpath) -> Result<Vec<(Span, f64)>, GenError> {
    path.moves
        .iter()
        .filter(|m| m.from.x != m.to.x || m.from.y != m.to.y)
        .map(|m| move_span(m).ok_or_else(|| GenError::Invalid("wire contour: the path holds a move that is not a flat XY line or arc - refused".into())))
        .collect()
}

/// The certificate: no move of `path` brings the wire's centre nearer
/// the profile than `k` (less [`PATH_TOL`]).
pub(super) fn kerf(path: &Toolpath, chain: &ArcChain, k: f64, z: f64) -> Result<(), GenError> {
    let floor = k - PATH_TOL;
    for (i, (ms, dev)) in spans_of(path)?.iter().enumerate() {
        for cs in chain.spans() {
            let d = span_span(ms, cs) - dev;
            if d.is_nan() || d < floor {
                let (_, q) = nearest(ms.point_at(0.5), cs);
                return Err(GenError::invalid_at(
                    format!(
                        "wire contour: move {i} would bring the wire within {d:.4} mm of the profile near X{:.3} Y{:.3} - under the kerf offset ({k:.4} mm), so the kerf would burn into the product; refused",
                        q.x, q.y
                    ),
                    Point3::new(q.x, q.y, z),
                ));
            }
        }
    }
    Ok(())
}

/// The corner minimum: every point of the profile lies within the kerf
/// offset (plus [`COVER_TOL`], or the stated sharp corners' reach) of
/// the path. Returns the worst rest the sharp corners let pass.
pub(super) fn cover(path: &Toolpath, chain: &ArcChain, p: &WireParams) -> Result<Option<UncutRest>, GenError> {
    let k = p.kerf_offset();
    let moves = spans_of(path)?;
    let heal = sharp_reach(p);
    let (strict, reach) = (k + COVER_TOL, k + heal.max(COVER_TOL));
    let step = (k * 0.5).max(chain.perimeter() / MAX_SAMPLES).max(1e-4);
    let mut over: Option<(f64, Point2)> = None;
    let mut healed: Option<(f64, Point2)> = None;
    for s in chain.spans() {
        let steps = (s.length() / step).ceil().clamp(2.0, 65_536.0) as u32;
        for j in 0..=steps {
            let q = s.point_at(f64::from(j) / f64::from(steps));
            let d = moves.iter().map(|(ms, dev)| pt_span(q, ms) + dev).fold(f64::INFINITY, f64::min);
            if d.is_nan() {
                return Err(GenError::Invalid("wire contour: the profile's distance to the wire path is not a number - refused".into()));
            }
            if d > reach && over.is_none_or(|(w, _)| d > w) {
                over = Some((d, q));
            } else if d > strict && healed.is_none_or(|(w, _)| d > w) {
                healed = Some((d, q));
            }
        }
    }
    if let Some((d, q)) = over {
        return Err(uncovered(q, d, p, heal, p.sharp_corners));
    }
    Ok(healed.map(|(d, q)| UncutRest { depth: d - k, at: Point3::new(q.x, q.y, p.plane_z) }))
}
