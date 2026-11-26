//! Provenance: monotone piecewise-linear preimages of polygon parallels; chord-length parameterization as in `pairing` (#2694).

use super::pairing::{pair, Ring};
use super::ring::dist;
use super::ruled::{arcs, invalid};
use super::ruled_laws::{foot, Ruling};
use super::WireSide;
use crate::geo2d::Point2;
use crate::machining2d::GenError;

const EPS: f64 = 1e-9;
// Clipper's integer lattice is 1e-6 mm. Short surviving edges can turn
// slightly when their endpoints round to that lattice.
const LATTICE: f64 = 8e-6;

fn refused() -> GenError {
    invalid("the healed offset has no monotone input preimage - simplify the profile or use a smaller kerf offset".into())
}

/// Drop redundant straight vertices before checking which input edges
/// survive. This preserves the input's geometric arc length.
fn source(given: &[Point2]) -> Result<Ring, GenError> {
    let ring = Ring::new(given).ok_or_else(refused)?;
    let n = ring.pts.len();
    let pts: Vec<_> = (0..n)
        .filter(|i| {
            let (a, b, c) = (
                ring.pts[(*i + n - 1) % n],
                ring.pts[*i],
                ring.pts[(*i + 1) % n],
            );
            let (dx, dy, ex, ey) = (b.x - a.x, b.y - a.y, c.x - b.x, c.y - b.y);
            (dx * ey - dy * ex).abs() > EPS * dx.hypot(dy) * ex.hypot(ey)
                || dx * ex + dy * ey <= 0.0
        })
        .map(|i| ring.pts[i])
        .collect();
    Ring::new(&pts).ok_or_else(refused)
}

/// Identify each surviving edge by its direction and its midpoint's
/// nearest parallel input edge. Opposite flanks cannot exchange sources.
fn edges(input: &Ring, offset: &Ring) -> Vec<Option<usize>> {
    (0..offset.pts.len())
        .map(|i| {
            let (a, b) = (offset.pts[i], offset.pts[(i + 1) % offset.pts.len()]);
            let (dx, dy) = (b.x - a.x, b.y - a.y);
            let len = dx.hypot(dy);
            let mid = Point2::new(a.x + dx * 0.5, a.y + dy * 0.5);
            (0..input.pts.len())
                .filter_map(|j| {
                    let (c, d) = (input.pts[j], input.pts[(j + 1) % input.pts.len()]);
                    let (ex, ey) = (d.x - c.x, d.y - c.y);
                    let other = ex.hypot(ey);
                    let parallel = (dx * ey - dy * ex).abs() <= LATTICE * (len + other);
                    if parallel && dx * ex + dy * ey > 0.0 {
                        Some((j, dist(mid, foot(mid, c, d).1)))
                    } else {
                        None
                    }
                })
                .min_by(|a, b| a.1.total_cmp(&b.1))
                .map(|(j, _)| j)
        })
        .collect()
}

fn healed(input: &Ring, offset: &Ring) -> bool {
    let mut survives = vec![false; input.pts.len()];
    for edge in edges(input, offset).into_iter().flatten() {
        survives[edge] = true;
    }
    survives.contains(&false)
}

fn projected(input: &Ring, edge: usize, q: Point2) -> f64 {
    let (a, b) = (input.pts[edge], input.pts[(edge + 1) % input.pts.len()]);
    input.cum[edge] + foot(q, a, b).0 * dist(a, b)
}

/// One input arc coordinate per offset vertex, except at a healed
/// intersection: its incoming and outgoing preimages both hold that
/// vertex. Interpolation across that interval therefore makes a fan.
fn mapped(input: &Ring, offset: &Ring) -> Result<Ring, GenError> {
    let from: Vec<usize> = edges(input, offset).into_iter().collect::<Option<_>>().ok_or_else(refused)?;
    let (n, m) = (from.len(), input.pts.len());
    let mut knots = Vec::with_capacity(n * 2);
    let mut turn = 0;
    for i in 0..n {
        let (prev, next, q) = (from[(i + n - 1) % n], from[i], offset.pts[i]);
        let step = (next + m - prev) % m;
        turn += step;
        match step {
            0 => knots.push((projected(input, next, q), q)),
            1 => knots.push((input.cum[next], q)),
            _ => {
                knots.push((projected(input, prev, q), q));
                knots.push((projected(input, next, q), q));
            }
        }
    }
    if turn != m {
        return Err(refused());
    }
    for (s, _) in &mut knots {
        *s = s.rem_euclid(input.total);
    }
    knots.sort_by(|a, b| a.0.total_cmp(&b.0));
    let mut pts = Vec::with_capacity(knots.len());
    let mut cum: Vec<f64> = Vec::with_capacity(knots.len());
    for (s, q) in knots {
        if cum.last().is_some_and(|last| s - last <= EPS) {
            if pts.last().is_none_or(|p| dist(*p, q) > LATTICE) {
                return Err(refused());
            }
            continue;
        }
        cum.push(s);
        pts.push(q);
    }
    // Keep zero as the first knot, so Ring::at also interpolates the
    // closing span when the input seam lies in an excised interval.
    let first = cum.first().copied().ok_or_else(refused)?;
    if first > EPS {
        let last = cum.len() - 1;
        let span = input.total - cum[last] + first;
        let t = (input.total - cum[last]) / span;
        let (a, b) = (pts[last], pts[0]);
        pts.insert(0, Point2::new(a.x + t * (b.x - a.x), a.y + t * (b.y - a.y)));
        cum.insert(0, 0.0);
    }
    Ok(Ring {
        pts,
        cum,
        total: input.total,
    })
}

pub(super) struct Sewn {
    pub(super) wire: Vec<Ruling>,
    /// Lower, upper: only healed rings need directions from surviving edges.
    pub(super) healed: [bool; 2],
}

/// Move off the surviving parallel, rather than following an excised
/// flank's normal back into a healed corner. The two surviving edge
/// normals form the miter. Include neighbouring segments within one
/// offset: a trimmed corner can retain lattice-sized bevels that would
/// reverse under a second offset if each used only its immediate edges.
pub(super) fn surviving_away(q: Point2, path: &[Point2], k: f64, side: WireSide) -> (f64, f64) {
    let sign = if side == WireSide::Die { -1.0 } else { 1.0 };
    let mut normals = Vec::new();
    for i in 0..path.len() {
        let (a, b) = (path[i], path[(i + 1) % path.len()]);
        let (dx, dy) = (b.x - a.x, b.y - a.y);
        let len = dx.hypot(dy);
        if len > EPS && dist(q, foot(q, a, b).1) <= k + LATTICE {
            normals.push((sign * dy / len, -sign * dx / len));
        }
    }
    let sum = normals
        .iter()
        .fold((0.0, 0.0), |s, n| (s.0 + n.0, s.1 + n.1));
    let len = sum.0.hypot(sum.1);
    if !(len.is_finite() && len > EPS) {
        return (0.0, 0.0);
    }
    let unit = (sum.0 / len, sum.1 / len);
    let low = normals
        .iter()
        .map(|n| n.0 * unit.0 + n.1 * unit.1)
        .fold(f64::INFINITY, f64::min);
    if low > 0.2 {
        (k * unit.0 / low, k * unit.1 / low)
    } else {
        (0.0, 0.0)
    }
}

/// Preserve established parallel-length pairing when neither ring
/// heals. Once either loses edges, sew both on their input arc lengths.
pub(super) fn sew(
    upper: &Ring,
    lower: &Ring,
    given: [&[Point2]; 2],
    anchors: &[(Point2, Point2, usize)],
    start: Point2,
) -> Result<Sewn, GenError> {
    let (gl, gu) = (source(given[0])?, source(given[1])?);
    let healed = [healed(&gl, lower), healed(&gu, upper)];
    let wire = if healed.contains(&true) {
        let (wl, wu) = (mapped(&gl, lower)?, mapped(&gu, upper)?);
        pair(
            &wu,
            &wl,
            &arcs(anchors, &gu, &gl, false),
            Some(wl.nearest(start, false).0),
        )?
    } else {
        pair(
            upper,
            lower,
            &arcs(anchors, upper, lower, true),
            Some(lower.nearest(start, false).0),
        )?
    };
    Ok(Sewn { wire, healed })
}

#[cfg(test)]
mod tests {
    use super::{mapped, Point2, Ring};

    fn square() -> Ring {
        Ring::new(&[
            Point2::new(0.0, 0.0),
            Point2::new(10.0, 0.0),
            Point2::new(10.0, 10.0),
            Point2::new(0.0, 10.0),
        ])
        .expect("a source square")
    }

    #[test]
    fn a_parallel_that_walks_the_source_twice_has_no_monotone_preimage() {
        let once = [
            Point2::new(-1.0, -1.0),
            Point2::new(11.0, -1.0),
            Point2::new(11.0, 11.0),
            Point2::new(-1.0, 11.0),
        ];
        let twice: Vec<_> = once.into_iter().cycle().take(8).collect();
        let offset = Ring::new(&twice).expect("a nonzero-area double walk");
        let why = mapped(&square(), &offset)
            .err()
            .expect("two source turns must be refused")
            .to_string();
        assert!(why.contains("no monotone input preimage"), "{why}");
    }

    #[test]
    fn distinct_offset_vertices_cannot_share_one_input_arc_position() {
        let offset = Ring::new(&[
            Point2::new(-2.0, -1.0),
            Point2::new(-1.0, -1.0),
            Point2::new(11.0, -1.0),
            Point2::new(11.0, 11.0),
            Point2::new(-2.0, 11.0),
        ])
        .expect("an offset with two distinct feet at source arc zero");
        let why = mapped(&square(), &offset)
            .err()
            .expect("conflicting preimage knots must be refused")
            .to_string();
        assert!(why.contains("no monotone input preimage"), "{why}");
    }

    fn inset() -> Ring {
        let offset = Ring::new(&[
            Point2::new(1.0, 1.0),
            Point2::new(9.0, 1.0),
            Point2::new(9.0, 9.0),
            Point2::new(1.0, 9.0),
        ])
        .expect("an inset square");
        mapped(&square(), &offset).expect("one source turn with distinct knots")
    }

    #[test]
    fn input_arc_coordinates_interpolate_the_surviving_flank() {
        let point = inset().at(5.0);
        assert!(
            (point.x - 5.0).abs() < 1e-9 && (point.y - 1.0).abs() < 1e-9,
            "input arc 5 must reach the surviving flank midpoint: {point:?}"
        );
    }

    #[test]
    fn input_arc_coordinates_project_the_surviving_flank() {
        let (arc, distance) = inset().nearest(Point2::new(5.0, 1.0), false);
        assert!(
            (arc - 5.0).abs() < 1e-9 && distance < 1e-9,
            "the surviving flank midpoint must project to input arc 5: {arc}, {distance}"
        );
    }

    #[test]
    fn subdividing_a_surviving_edge_does_not_hide_an_excised_fillet() {
        use super::super::preimage_tests::rounded;
        use super::super::ring::{dist, wire_ring};
        use super::super::WireSide;
        use crate::geo2d::Region;

        let (upper, lower) = (rounded(0.3), rounded(0.164));
        let lower_region = Region::new(vec![lower.clone()]).expect("lower region");
        let upper_region = Region::new(vec![upper.clone()]).expect("upper region");
        let lower_wire = wire_ring(&lower_region, WireSide::Die, 0.165).expect("healed lower ring");
        let upper_wire = wire_ring(&upper_region, WireSide::Die, 0.165).expect("upper ring");
        let longest = (0..lower_wire.len())
            .max_by(|a, b| {
                dist(lower_wire[*a], lower_wire[(*a + 1) % lower_wire.len()])
                    .total_cmp(&dist(lower_wire[*b], lower_wire[(*b + 1) % lower_wire.len()]))
            })
            .expect("a surviving straight edge");
        let mut subdivided = Vec::new();
        for i in 0..lower_wire.len() {
            let pieces = if i == longest { 150 } else { 1 };
            for j in 0..pieces {
                let fraction = f64::from(j) / f64::from(pieces);
                let (a, b) = (lower_wire[i], lower_wire[(i + 1) % lower_wire.len()]);
                subdivided.push(Point2::new(a.x + fraction * (b.x - a.x), a.y + fraction * (b.y - a.y)));
            }
        }
        assert!(subdivided.len() >= lower.points().len(), "extra straight vertices hide the loss from a count-only check");
        let (gu, gl) = (Ring::new(upper.points()).expect("upper input"), Ring::new(lower.points()).expect("lower input"));
        let (wu, wl) = (Ring::new(&upper_wire).expect("upper parallel"), Ring::new(&subdivided).expect("subdivided lower parallel"));
        let start = Point2::new(-5.0, -1.0);
        let anchors = [(gu.at(gu.nearest(start, false).0), gl.at(gl.nearest(start, false).0), 0)];
        let sewn = super::sew(&wu, &wl, [lower.points(), upper.points()], &anchors, start).expect("collinear subdivision keeps the preimage");
        let fan = sewn.wire.windows(2).any(|w| dist(w[0].0, w[1].0) < 1e-9 && dist(w[0].1, w[1].1) > 1e-5);
        assert!(fan, "the excised lower fillet must still make a stationary fan after straight-edge subdivision");
    }
}
