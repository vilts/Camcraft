//! A contour dive below the levels above (#3678): the floor that the
//! posted cuts left under the full tool disc, and the peck that keeps
//! each feed down to one stepdown below it.
//!
//! Provenance: the peck (step) plunge of public G-code manuals (the G83
//! deep-hole cycle of RS-274 / ISO 6983: feed one step, retract, feed the
//! next) and the stepdown rule of machining handbooks; the floor reading
//! is this crate's own geometry. No vendor material consulted.

use crate::cl::{Move, MoveKind, PathBuilder};
use crate::geo2d::Point2;
use crate::geometry::Point3;

use super::contour_params::ContourParams;

/// Height slack: a cut on the dive's own plane counts as at its depth.
const Z_EPS: f64 = 1e-6;
/// A conservative floor under the complete tool disc of radius `r`.
/// A previous cut proves clearance only if its centreline contains `xy`
/// exactly. Then the disc swept at that centre contains the entire new
/// disc, including its rim. Unknown coverage stays at `uncut`; samples
/// and arc chords cannot prove that a thin crescent is clear.
#[must_use]
pub(super) fn disc_floor(moves: &[Move], xy: Point2, r: f64, uncut: f64) -> f64 {
    if !(r.is_finite() && r > 0.0) {
        return uncut;
    }
    moves
        .iter()
        .filter(|m| m.kind != MoveKind::Rapid && m.to.z < uncut - Z_EPS)
        .filter(|m| {
            let from = Point2::new(m.from.x, m.from.y);
            let to = Point2::new(m.to.x, m.to.y);
            let at_from = xy.x == from.x && xy.y == from.y;
            let at_to = xy.x == to.x && xy.y == to.y;
            let column = from == to && m.to.z < m.from.z;
            let flat = m.from.z == m.to.z;
            // Endpoints are on both straight and circular centreline paths.
            // For an interior point, certify only axis-aligned straight
            // feeds: a rounded orientation test could hide a rim crescent.
            let on_straight = m.kind == MoveKind::Feed
                && m.arc.is_none()
                && ((from.x == to.x && xy.x == from.x && between(xy.y, from.y, to.y))
                    || (from.y == to.y && xy.y == from.y && between(xy.x, from.x, to.x)));
            (column && at_to) || (flat && (at_from || at_to || on_straight))
        })
        .map(|m| m.to.z)
        .fold(uncut, f64::min)
}

fn between(x: f64, a: f64, b: f64) -> bool {
    x >= a.min(b) && x <= a.max(b)
}

/// The depths a dive at `xy` down to `z` stops at: one stepdown apart
/// below the disc floor, the last one `z`. A dive within one stepdown of
/// the floor (or with no `uncut` floor: a ring's first level) is `[z]`.
#[must_use]
pub(super) fn peck_depths(
    moves: &[Move],
    xy: Point2,
    z: f64,
    r: f64,
    step: f64,
    uncut: Option<f64>,
) -> Vec<f64> {
    let Some(uncut) = uncut else { return vec![z] };
    if !(step.is_finite() && step > 0.0 && z.is_finite()) {
        return vec![z];
    }
    let floor = disc_floor(moves, xy, r, uncut);
    let mut out = Vec::new();
    // The contour's level budget already bounds this span to 100,000
    // stepdowns. Do not truncate it: an appended target after a cap can
    // turn the last peck into a cut thousands of stepdowns deep.
    // Compute each depth from the floor to avoid accumulated subtraction.
    let mut index = 1_u32;
    loop {
        let d = floor - f64::from(index) * step;
        if d <= z + Z_EPS {
            break;
        }
        out.push(d);
        index += 1;
    }
    out.push(z);
    out
}

/// The floor with no cut under the disc, for a dive on level `rzi` of a
/// ring whose levels are `rz`: the material's top, or one stepdown over
/// the ring's first level where that is lower (a shelf band). `None` on
/// the ring's first level: that dive never pecks.
#[must_use]
pub(super) fn uncut_floor(p: &ContourParams, rz: &[f64], rzi: usize) -> Option<f64> {
    let first = *rz.first()?;
    (rzi > 0).then(|| p.entry_top().min(first + p.stepdown))
}

/// Where the tool waits between pecks: the plunge height over the
/// material, held under the retract plane (`feed_down`'s own rapid plane).
#[must_use]
fn retract_plane(p: &ContourParams, hop_z: f64) -> f64 {
    let top = p.entry_top();
    (top + p.links.approach.plunge_height.max(0.0)).min(hop_z.max(top))
}

/// The dive the tool stands over at `xy`: the first feed down by `first`
/// (the caller's plunge to the depth it is given), then the pecks to `z`.
#[allow(clippy::too_many_arguments)]
pub(super) fn dive(
    b: &mut PathBuilder,
    p: &ContourParams,
    xy: Point2,
    z: f64,
    uncut: Option<f64>,
    hop_z: f64,
    feed: f64,
    first: impl FnOnce(&mut PathBuilder, f64),
) {
    let depths = peck_depths(b.moves(), xy, z, p.tool_diameter * 0.5, p.stepdown, uncut);
    first(b, depths.first().copied().unwrap_or(z));
    push_pecks(b, xy, &depths, retract_plane(p, hop_z), p.links.approach.plunge_height, feed);
}

/// The pecks after the first feed down (`depths[0]`, already posted): up
/// to `retract` - air over the material - by rapid, down by rapid to the
/// plunge height over the last depth, then the feed one stepdown deeper.
/// A pecked dive is said in one note for the path.
fn push_pecks(b: &mut PathBuilder, xy: Point2, depths: &[f64], retract: f64, h: f64, feed: f64) {
    let Some((&first, rest)) = depths.split_first() else { return };
    let mut last = first;
    for &d in rest {
        b.push(MoveKind::Rapid, Point3::new(xy.x, xy.y, retract), None);
        let back = last + h.max(0.0);
        if h > 0.0 && back < retract {
            b.push(MoveKind::Rapid, Point3::new(xy.x, xy.y, back), None);
        }
        b.push(MoveKind::Plunge, Point3::new(xy.x, xy.y, d), Some(feed));
        last = d;
    }
    if !rest.is_empty() {
        const SAID: &str = "entry: a dive more than one stepdown below the floor that the levels above left under the tool disc";
        if !b.notes().iter().any(|n| n.starts_with(SAID)) {
            b.note(format!(
                "{SAID} is pecked one stepdown at a time (first at ({:.3}, {:.3}) to Z {:.3}, {} pecks)",
                xy.x,
                xy.y,
                depths[depths.len() - 1],
                depths.len()
            ));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_dive_within_one_stepdown_or_on_a_first_level_does_not_peck() {
        assert_eq!(peck_depths(&[], Point2::new(0.0, 0.0), -4.5, 3.0, 4.5, Some(0.0)), vec![-4.5]);
        assert_eq!(peck_depths(&[], Point2::new(0.0, 0.0), -18.0, 3.0, 4.5, None), vec![-18.0]);
    }

    #[test]
    fn a_dive_into_uncut_stock_pecks_one_stepdown_at_a_time() {
        let d = peck_depths(&[], Point2::new(0.0, 0.0), -18.0, 3.0, 4.5, Some(0.0));
        assert_eq!(d, vec![-4.5, -9.0, -13.5, -18.0]);
    }
    #[test]
    fn an_offset_column_leaves_an_uncut_rim() {
        // At x=-3 the new disc lies 3.002 mm from the prior column's
        // centre. Its actual 3 mm radius cannot clear that rim point.
        let prior = Move::new(
            MoveKind::Plunge,
            Point3::new(0.002, 0.0, 0.0),
            Point3::new(0.002, 0.0, -18.0),
            Some(100.0),
        );
        let xy = Point2::new(0.0, 0.0);
        assert_eq!(disc_floor(&[prior], xy, 3.0, 0.0), 0.0);
        assert_eq!(
            peck_depths(&[prior], xy, -18.0, 3.0, 4.5, Some(0.0)),
            vec![-4.5, -9.0, -13.5, -18.0]
        );
    }

    #[test]
    fn an_offset_straight_cut_does_not_prove_full_disc_coverage() {
        let prior = Move::new(
            MoveKind::Feed,
            Point3::new(-5.0, 0.002, -18.0),
            Point3::new(5.0, 0.002, -18.0),
            Some(100.0),
        );
        // The analytic distance of (0,-3) to the segment is 3.002 mm.
        assert_eq!(disc_floor(&[prior], Point2::new(0.0, 0.0), 3.0, 0.0), 0.0);
        // A centre exactly on this straight cut proves the whole disc.
        assert_eq!(
            disc_floor(&[prior], Point2::new(0.0, 0.002), 3.0, 0.0),
            -18.0
        );
    }

    #[test]
    fn a_dive_beyond_4096_pecks_preserves_every_stepdown() {
        let depths = peck_depths(&[], Point2::new(0.0, 0.0), -50.0, 3.0, 0.01, Some(0.0));
        assert_eq!(depths.len(), 5000);
        let mut previous = 0.0;
        for depth in depths {
            assert!(
                previous - depth <= 0.01 + 1e-10,
                "unsafe final cut: {previous} -> {depth}"
            );
            previous = depth;
        }
        assert_eq!(previous, -50.0);
    }
}
