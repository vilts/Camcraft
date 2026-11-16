//! The wire contour's rings (#532): the one closed profile read out of
//! the picks, its parallel at the kerf offset on the stated side, and
//! the polyline walks the emitter rides.
//!
//! Provenance: the parallel curve is the textbook polygon offset with
//! mitred joins (clipper2 via `geo2d::Region::offset_sharp`); the walks
//! are plain polyline geometry. Our own code.

use crate::geo2d::{GeoError, Loop, Point2, Region};
use crate::machining2d::GenError;

use super::WireSide;

/// The smallest ring area (mm²) that bounds anything: below it a closed
/// profile is a zero-length or collinear ring.
const MIN_AREA: f64 = 1e-6;

fn invalid(why: String) -> GenError {
    GenError::Invalid(format!("wire contour: {why}"))
}

pub(super) fn dist(a: Point2, b: Point2) -> f64 {
    (b.x - a.x).hypot(b.y - a.y)
}

/// Distance from `p` to segment a-b.
pub(super) fn pt_seg(p: Point2, a: Point2, b: Point2) -> f64 {
    let (dx, dy) = (b.x - a.x, b.y - a.y);
    let l2 = dx * dx + dy * dy;
    let t = if l2 > 0.0 { (((p.x - a.x) * dx + (p.y - a.y) * dy) / l2).clamp(0.0, 1.0) } else { 0.0 };
    (p.x - (a.x + t * dx)).hypot(p.y - (a.y + t * dy))
}

/// Distance from `p` to the open polyline `run` (#1109).
pub(super) fn pt_poly(p: Point2, run: &[Point2]) -> f64 {
    match run {
        [] => f64::INFINITY,
        [one] => dist(p, *one),
        _ => run.windows(2).map(|w| pt_seg(p, w[0], w[1])).fold(f64::INFINITY, f64::min),
    }
}

/// Distance from `p` to the closed ring.
pub(super) fn pt_ring(p: Point2, ring: &[Point2]) -> f64 {
    let n = ring.len();
    (0..n).map(|i| pt_seg(p, ring[i], ring[(i + 1) % n])).fold(f64::INFINITY, f64::min)
}

/// The one closed profile of the picks: its points as given (what the
/// laws judge the cut against) and its region. Open runs, several
/// profiles, a crossing or area-less ring are refused by name.
pub(super) fn profile(loops: &[Loop], chains: &[Vec<Point2>]) -> Result<(Vec<Point2>, Region), GenError> {
    if !chains.is_empty() {
        return Err(invalid(format!(
            "the profile is open ({} open run(s) picked) - this operation cuts one CLOSED profile; an open profile has no punch or die side",
            chains.len()
        )));
    }
    let one = match loops {
        [] => return Err(GenError::Empty("wire contour: no profile to cut".into())),
        [one] => one,
        many => {
            return Err(invalid(format!(
                "{} closed profiles are given - one operation cuts one profile from its own start hole; pick one, and give each profile its own operation",
                many.len()
            )));
        }
    };
    let region = Region::new(vec![one.clone()]).map_err(|e| match e {
        GeoError::Invalid(w) if w.contains("crosses itself") => {
            invalid("the profile crosses itself - a wire path cannot cross its own kerf".into())
        }
        e => invalid(format!("the profile is unusable: {e}")),
    })?;
    let area = region.area().abs();
    if region.loops().len() != 1 || !(area.is_finite() && area > MIN_AREA) {
        return Err(invalid(
            "the profile has no area (a zero-length or collinear ring) - there is nothing to cut around".into(),
        ));
    }
    Ok((one.points().to_vec(), region))
}

/// The wire-centre ring: the profile shrunk by `k` (a die - the wire
/// inside) or grown by `k` (a punch - the wire outside), mitred so no
/// corner of the product is rolled over, counter-clockwise. A profile
/// the wire does not fit, or one whose offset splits or closes a gap,
/// is refused by name.
pub(super) fn wire_ring(profile: &Region, side: WireSide, k: f64) -> Result<Vec<Point2>, GenError> {
    let off = profile.offset_sharp(match side {
        WireSide::Die => -k,
        WireSide::Punch => k,
        WireSide::Left | WireSide::Right => {
            return Err(invalid("a closed profile states its die or punch side, not left or right".into()));
        }
    })?;
    let kerf = 2.0 * k;
    let one = match (off.loops(), side) {
        ([one], _) => one,
        ([], WireSide::Die) => {
            return Err(invalid(format!(
                "the profile is too small for the wire - no point inside it stands the kerf offset ({k:.4} mm) from its edge"
            )));
        }
        (_, WireSide::Die) => {
            return Err(invalid(format!(
                "the profile narrows to a neck under the kerf width ({kerf:.4} mm) - the wire cannot pass it without burning the die"
            )));
        }
        (_, WireSide::Punch | WireSide::Left | WireSide::Right) => {
            return Err(invalid(format!(
                "the profile has a slot or notch narrower than the kerf width ({kerf:.4} mm) - the wire cannot enter it; widen it or use a thinner wire"
            )));
        }
    };
    let mut pts = one.points().to_vec();
    if pts.len() < 3 {
        return Err(invalid("the wire path has collapsed to a line - the profile is too small for the wire".into()));
    }
    if !one.is_ccw() {
        pts.reverse();
    }
    Ok(pts)
}

/// The nearest point of the closed ring to `p`: (edge index, foot). The
/// first nearest edge wins a tie, so the answer is deterministic.
pub(super) fn nearest(ring: &[Point2], p: Point2) -> (usize, Point2) {
    let n = ring.len();
    let mut best = (0usize, ring.first().copied().unwrap_or(p), f64::INFINITY);
    for i in 0..n {
        let (a, b) = (ring[i], ring[(i + 1) % n]);
        let (dx, dy) = (b.x - a.x, b.y - a.y);
        let dd = dx * dx + dy * dy;
        let t = if dd > 0.0 { (((p.x - a.x) * dx + (p.y - a.y) * dy) / dd).clamp(0.0, 1.0) } else { 0.0 };
        let f = Point2::new(a.x + t * dx, a.y + t * dy);
        let d = dist(f, p);
        if d < best.2 {
            best = (i, f, d);
        }
    }
    (best.0, best.1)
}

/// The closed walk of the counter-clockwise `ring` from `s` on edge
/// `seg`: s, the ring's vertices in order, and back to s. Coincident
/// neighbours are dropped.
pub(super) fn walk_from(ring: &[Point2], seg: usize, s: Point2) -> Vec<Point2> {
    let n = ring.len();
    let mut out = vec![s];
    let push = |p: Point2, out: &mut Vec<Point2>| {
        if out.last().is_none_or(|q| dist(*q, p) > 1e-9) {
            out.push(p);
        }
    };
    for k in 1..=n {
        push(ring[(seg + k) % n], &mut out);
    }
    push(s, &mut out);
    out
}
