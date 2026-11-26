//! Place the perpendicular native-job lead on the waste side, on the native
//! ISO lattice (planar half-plane geometry). The ring has the waste on its
//! right: outside a counter-clockwise punch, inside a clockwise die opening.
use super::{Point, cross, dot, vector};
use crate::agie::AgieSide;

const LEAD: i128 = 300_000; // 3 mm on the ISO lattice.
const HOLE_RADIUS: i128 = 100_000; // The predrilled 2 mm threading hole.

/// The complete approach stays on the waste side of this emitted face.
fn clears_entry_line(a: Point, b: Point, hole: Point) -> bool {
    let side = cross(a, b, hole);
    let edge = vector(a, b);
    side < 0 && side * side >= LEAD * LEAD * dot(edge, edge)
}

/// Whether `p` lies strictly on the waste side of the line through `a` and
/// `b`, more than `clearance` lattice units from that line.
fn deeper_than(a: Point, b: Point, p: Point, clearance: i128) -> bool {
    let signed = cross(a, b, p);
    let edge = vector(a, b);
    signed < 0 && signed * signed > clearance * clearance * dot(edge, edge)
}

/// Quantize the perpendicular projection without moving the start hole or
/// cutting into retained material. Compare the four nearest cell corners
/// against the exact rational projection. Allow at most one grid unit
/// (0.00001 mm) Euclidean error to choose a waste-side point when nearest
/// rounding is toward the retained material.
fn outside_seam(a: Point, b: Point, hole: Point, along: i128) -> Option<Point> {
    let edge = vector(a, b);
    let length2 = dot(edge, edge);
    let numerator: [i128; 2] =
        std::array::from_fn(|k| i128::from(a[k]) * length2 + i128::from(edge[k]) * along);
    // The projected point lies strictly between the bounded endpoints, so it fits an i64.
    let floor = [
        i64::try_from(numerator[0].div_euclid(length2)).ok()?,
        i64::try_from(numerator[1].div_euclid(length2)).ok()?,
    ];
    let mut best = None;
    for x in floor[0]..=floor[0] + 1 {
        for y in floor[1]..=floor[1] + 1 {
            let p = [x, y];
            let residual: [i128; 2] =
                std::array::from_fn(|k| i128::from(p[k]) * length2 - numerator[k]);
            let error2 = residual[0] * residual[0] + residual[1] * residual[1];
            if error2 > length2 * length2 || cross(a, b, p) > 0 {
                continue;
            }
            let from_a = dot(vector(a, p), edge);
            let from_b = length2 - from_a;
            // Measure room along the original face, before and after the seam.
            // Neither coordinate rounding nor a short angled leg may add
            // fictitious room for the transition and separation segment.
            if from_a <= 0
                || from_b <= 0
                || from_a * from_a < LEAD * LEAD * length2
                || from_b * from_b < LEAD * LEAD * length2
                || !clears_entry_line(a, p, hole)
                || !clears_entry_line(p, b, hole)
            {
                continue;
            }
            if best.is_none_or(|(previous, _)| error2 < previous) {
                best = Some((error2, p));
            }
        }
    }
    best.map(|(_, p)| p)
}

/// Choose the entry face and the seam point on it.
///
/// A punch needs a supporting exterior face: the whole punch lies on one side
/// of it, so the lead outside it stays in the waste. A die opening must be
/// convex (the caller checks this). It is the intersection of the half-planes
/// of its walls. The complete hole lies inside every wall, and the seam point
/// clears every other wall by the rough pass and wire `margin`. The straight
/// lead between the two then stays inside the opening, clear of each wall.
pub(super) fn entry_face(
    ring: &[Point],
    hole: Point,
    side: AgieSide,
    margin: i64,
) -> Result<(usize, Point), String> {
    let n = ring.len();
    let wall = |i: usize| (ring[i], ring[(i + 1) % n]);
    if side == AgieSide::Die
        && (0..n).any(|i| {
            let (a, b) = wall(i);
            !deeper_than(a, b, hole, HOLE_RADIUS)
        })
    {
        return Err("The complete 2 mm start hole must lie inside the die opening".into());
    }
    let mut best = None;
    let mut nominal_room = false;
    let mut nearest = f64::INFINITY;
    for i in 0..n {
        let (a, b) = wall(i);
        if !clears_entry_line(a, b, hole)
            || (side == AgieSide::Punch && ring.iter().any(|&p| cross(a, b, p) < 0))
        {
            continue;
        }
        let edge = vector(a, b);
        let length2 = dot(edge, edge);
        let along = dot(vector(a, hole), edge);
        let remaining = length2 - along;
        if along <= 0
            || remaining <= 0
            || along * along < LEAD * LEAD * length2
            || remaining * remaining < LEAD * LEAD * length2
        {
            continue;
        }
        nominal_room = true;
        let distance = -(cross(a, b, hole) as f64) / (length2 as f64).sqrt();
        if distance < nearest
            && let Some(foot) = outside_seam(a, b, hole, along)
        {
            if side == AgieSide::Die
                && (0..n).any(|j| {
                    let (c, d) = wall(j);
                    j != i && !deeper_than(c, d, foot, i128::from(margin))
                })
            {
                continue;
            }
            nearest = distance;
            best = Some((i, foot));
        }
    }
    best.ok_or_else(|| match side {
        AgieSide::Die => "The job needs 3 mm of clear inward lead from the start hole and 3 mm of straight wall on both sides of the entry into the die opening".into(),
        AgieSide::Punch if nominal_room => "The job cannot place an exterior entry on the 0.00001 mm ISO grid and keep the required 3 mm clearance".into(),
        AgieSide::Left | AgieSide::Right => "An open cut has no start hole entry".into(),
        AgieSide::Punch => "The job needs the start hole at least 3 mm outside a straight face, with at least 3 mm of straight face on both sides of the perpendicular entry for the lead and separation".into(),
    })
}
