//! The wire contour's two laws (#532), run before the path leaves the
//! generator:
//!
//! - **Cover law - the corner minimum** (#290 D8). The wire is a circle
//!   of the kerf offset's radius rolling along the profile on the waste
//!   side, so the tightest inside corner it can leave has that radius:
//!   wire radius plus spark gap. Wherever the profile turns tighter -
//!   a sharp inside corner, a fillet under the minimum, a slot or notch
//!   narrower than the kerf - the wire-centre path stands farther than
//!   the offset from the profile and material the drawing calls cut is
//!   left standing. Every point of the profile must therefore lie
//!   within the offset (plus [`COVER_TOL`]) of the path; the worst
//!   point that does not is refused by name, with its spot. Profile
//!   points carry no arc or mesh provenance, so their shape cannot
//!   justify a larger tolerance (#3208).
//! - **Kerf law.** No move - the leads included - brings the wire's
//!   centre nearer the profile than the offset (less [`PATH_TOL`]): the
//!   kerf never enters the product. The path is built to hold this; the
//!   law is its certificate, and a failure is refused, never trimmed.
//!
//! Provenance: plain 2D distance geometry over the rolling-circle
//! reading of a wire cut (see the module head). Our own code.

use crate::cl::Toolpath;
use crate::geo2d::Point2;
use crate::geometry::Point3;
use crate::machining2d::GenError;

use super::ring::{dist, pt_poly, pt_ring};
use super::{WireParams, COVER_TOL, PATH_TOL};

/// The most samples either law reads along the profile or the path.
const MAX_SAMPLES: f64 = 40_000.0;

/// The sampling step (mm) along a run of `total` mm for offset `k`:
/// half the offset, widened so the whole run takes at most
/// [`MAX_SAMPLES`] - vertices are always read, whatever the step.
fn step_for(k: f64, total: f64) -> f64 {
    (k * 0.5).max(total / MAX_SAMPLES).max(1e-4)
}

/// The segments of `pts` as index pairs: closing back to the first
/// point for a ring, not for an open run (#1109).
fn segs(n: usize, closed: bool) -> impl Iterator<Item = (usize, usize)> {
    let m = if closed { n } else { n.saturating_sub(1) };
    (0..m).map(move |i| (i, (i + 1) % n.max(1)))
}

fn perimeter(pts: &[Point2], closed: bool) -> f64 {
    segs(pts.len(), closed).map(|(i, j)| dist(pts[i], pts[j])).sum()
}

/// Distance from `q` to `pts`, a ring or an open run.
fn to(q: Point2, pts: &[Point2], closed: bool) -> f64 {
    if closed {
        pt_ring(q, pts)
    } else {
        pt_poly(q, pts)
    }
}

/// Worst uncut corner or slot rest above [`COVER_TOL`] accepted under
/// an explicit `heal_tolerance`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct UncutRest {
    pub(super) depth: f64,
    pub(super) at: Point3,
}

impl UncutRest {
    pub(super) fn worse(a: Option<Self>, b: Option<Self>) -> Option<Self> {
        match (a, b) {
            (Some(x), Some(y)) => Some(if y.depth > x.depth { y } else { x }),
            (x, y) => x.or(y),
        }
    }
}

/// The corner minimum: every point of the profile `given` lies within
/// the kerf offset of the wire-centre `path_ring` - or, when `p` states
/// sharp inside corners (#3207), within twice it. The worst rest past
/// [`COVER_TOL`] that the option let pass is returned.
pub(super) fn cover(given: &[Point2], path_ring: &[Point2], p: &WireParams) -> Result<Option<UncutRest>, GenError> {
    cover_run(given, true, path_ring, true, p)
}

/// The rest (mm) a stated sharp inside corner may keep (#3207): the
/// kerf offset itself - the wire's own corner radius. A sharp corner of
/// 60 degrees or more leaves no more than that; a narrower one, or a
/// slot deeper than the offset that the kerf cannot enter, still does.
pub(super) fn sharp_reach(p: &WireParams) -> f64 {
    if p.sharp_corners {
        p.kerf_offset()
    } else {
        0.0
    }
}

/// What a stated sharp inside corner left (#3207), said on the path.
pub(super) fn sharp_note(r: UncutRest, k: f64) -> String {
    format!(
        "{} - the tightest inside corner keeps the wire's own radius {k:.4} mm (wire radius + spark gap); worst uncut rest {:.4} mm near X{:.3} Y{:.3}",
        super::SHARP_NOTE,
        r.depth, r.at.x, r.at.y
    )
}

/// [`cover`] with an explicit `heal_tolerance` (`0.0` enforces strict
/// [`COVER_TOL`]), returning the worst uncut rest above [`COVER_TOL`]
/// when accepted.
pub(super) fn cover_healed(
    given: &[Point2],
    path_ring: &[Point2],
    p: &WireParams,
    heal_tolerance: f64,
) -> Result<Option<UncutRest>, GenError> {
    cover_run_with(given, true, path_ring, true, p, heal_tolerance, false)
}

/// [`cover`] over a ring or an open run (#1109): `given_closed` and
/// `path_closed` say which each polyline is.
pub(super) fn cover_run(
    given: &[Point2],
    given_closed: bool,
    path: &[Point2],
    path_closed: bool,
    p: &WireParams,
) -> Result<Option<UncutRest>, GenError> {
    cover_run_with(given, given_closed, path, path_closed, p, sharp_reach(p), p.sharp_corners)
}

fn cover_run_with(
    given: &[Point2],
    given_closed: bool,
    path: &[Point2],
    path_closed: bool,
    p: &WireParams,
    heal_tolerance: f64,
    sharp: bool,
) -> Result<Option<UncutRest>, GenError> {
    let k = p.kerf_offset();
    // Coordinates alone cannot distinguish mesh facets from authored
    // corners. Keep the strict cover tolerance without arc provenance.
    let step = step_for(k, perimeter(given, given_closed));
    let mut worst_over: Option<(f64, Point2)> = None;
    let mut worst_healed: Option<(f64, Point2)> = None;
    for (i, j) in segs(given.len(), given_closed) {
        let (a, b) = (given[i], given[j]);
        let len = dist(a, b);
        let steps = (len / step).ceil().clamp(1.0, 65_536.0) as u32;
        for s in 0..steps {
            let f = f64::from(s) / f64::from(steps);
            let q = Point2::new(a.x + (b.x - a.x) * f, a.y + (b.y - a.y) * f);
            let strict = k + COVER_TOL;
            let reach = k + heal_tolerance.max(COVER_TOL);
            let d = to(q, path, path_closed);
            // A NaN distance is never "covered".
            if d.is_nan() {
                return Err(GenError::Invalid(
                    "wire contour: the profile's distance to the wire path is not a number - refused".into(),
                ));
            }
            if d > reach && worst_over.is_none_or(|(w, _)| d > w) {
                worst_over = Some((d, q));
            } else if d > strict && worst_healed.is_none_or(|(w, _)| d > w) {
                worst_healed = Some((d, q));
            }
        }
    }
    if let Some((d, q)) = worst_over {
        return Err(uncovered(q, d, p, heal_tolerance, sharp));
    }
    Ok(worst_healed.map(|(d, q)| UncutRest { depth: d - k, at: Point3::new(q.x, q.y, p.plane_z) }))
}

/// The cover law's refusal: profile point `q` stands `d` from the wire
/// path, past the reach `heal_tolerance` (or the stated sharp corners)
/// allows.
pub(super) fn uncovered(q: Point2, d: f64, p: &WireParams, heal_tolerance: f64, sharp: bool) -> GenError {
    let k = p.kerf_offset();
    let rest = d - k;
    let msg = if sharp {
        format!(
            "wire contour: an inside corner or slot of the profile near X{:.3} Y{:.3} is tighter than the wire can cut on the {} side, even with sharp inside corners stated - the kerf would leave {rest:.4} mm standing there, past the wire's own corner radius {k:.4} mm (wire radius {:.4} + spark gap {:.4}); open the corner to 60 degrees or more, round it, or cut with a thinner wire",
            q.x,
            q.y,
            p.side.word(),
            p.wire_diameter * 0.5,
            p.gap,
        )
    } else if heal_tolerance == 0.0 {
        format!(
            "wire contour: an inside corner or slot of the profile near X{:.3} Y{:.3} is tighter than the wire can cut on the {} side - the minimum inside radius is {k:.4} mm (wire radius {:.4} + spark gap {:.4}), and the kerf would leave {rest:.4} mm standing there; round it to at least that radius, or cut with a thinner wire",
            q.x,
            q.y,
            p.side.word(),
            p.wire_diameter * 0.5,
            p.gap,
        )
    } else {
        format!(
            "wire contour: an inside corner or slot of the profile near X{:.3} Y{:.3} is tighter than the wire can cut on the {} side - the minimum inside radius is {k:.4} mm (wire radius {:.4} + spark gap {:.4}), and the kerf would leave {rest:.4} mm standing there, past the 4-axis heal tolerance {heal_tolerance:.4} mm; round it, cut with a thinner wire, or raise heal_tolerance",
            q.x,
            q.y,
            p.side.word(),
            p.wire_diameter * 0.5,
            p.gap,
        )
    };
    GenError::invalid_at(msg, Point3::new(q.x, q.y, p.plane_z))
}

/// The certificate: no move of `path` brings the wire's centre nearer
/// the profile `given` (a ring, or an open run when `!given_closed`,
/// #1109) than the kerf offset `k`.
pub(super) fn kerf(path: &Toolpath, given: &[Point2], given_closed: bool, k: f64, z: f64) -> Result<(), GenError> {
    let moves: Vec<(Point2, Point2)> =
        path.moves.iter().map(|m| (Point2::new(m.from.x, m.from.y), Point2::new(m.to.x, m.to.y))).collect();
    kerf_segments(&moves, given, given_closed, k, z)
}

/// [`kerf`] over a closed ring of wire points walked from and back to `start`.
pub(super) fn kerf_ring_with_start(
    ring: impl Iterator<Item = Point2>,
    start: Point2,
    given: &[Point2],
    k: f64,
    z: f64,
) -> Result<(), GenError> {
    let pts: Vec<Point2> = ring.collect();
    let Some(&first) = pts.first() else {
        return Ok(());
    };
    let mut moves = Vec::with_capacity(pts.len() + 2);
    let mut prev = start;
    for q in pts.into_iter().chain(std::iter::once(first)).chain(std::iter::once(start)) {
        moves.push((prev, q));
        prev = q;
    }
    kerf_segments(&moves, given, true, k, z)
}

fn kerf_segments(moves: &[(Point2, Point2)], given: &[Point2], given_closed: bool, k: f64, z: f64) -> Result<(), GenError> {
    let total: f64 = moves.iter().map(|(a, b)| dist(*a, *b)).sum();
    let step = step_for(k, total);
    let floor = k - PATH_TOL;
    for (i, &(a, b)) in moves.iter().enumerate() {
        let steps = (dist(a, b) / step).ceil().clamp(1.0, 65_536.0) as u32;
        for j in 0..=steps {
            let f = f64::from(j) / f64::from(steps);
            let q = Point2::new(a.x + (b.x - a.x) * f, a.y + (b.y - a.y) * f);
            let d = to(q, given, given_closed);
            if d.is_nan() || d < floor {
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
