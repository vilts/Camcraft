//! Provenance: outer-versus-hole reading of closed sketch loops. Containment is the crossing-
//! number test with the half-open edge rule (Shimrat, "Algorithm 112: Position of point relative
//! to polygon", CACM 5(8), 1962; Haines, "Point in Polygon Strategies", Graphics Gems IV, 1994),
//! extended to circular arcs by cutting each arc into y-monotone pieces at its top and bottom
//! points. A loop inside an even number of loops is an outer boundary and inside an odd number
//! a hole: the even-odd rule `geo2d::Region` reads its loops by. Original implementation.

use super::crossing::{Sealed, sweep_on_y};
use super::curve::{Piece, Shape};
use super::{Budget, ChainError};
use crate::geo2d::Point2;
use std::f64::consts::{FRAC_PI_2, PI};

/// Whether a ray from `p` towards +x crosses the y-monotone span from `ya` to `yb`, whose x at
/// the ray's height is `x()`. Lower end inclusive, upper end exclusive.
fn crosses(p: Point2, ya: f64, yb: f64, x: impl Fn() -> f64) -> bool {
    (ya > p.y) != (yb > p.y) && x() > p.x
}

fn arc_parity(
    p: Point2,
    piece: &Piece,
    center: Point2,
    radius: f64,
    start: f64,
    sweep: f64,
) -> bool {
    // Walk the arc by increasing angle; the drawn ends keep the exact points of the piece.
    let (drawn_from, drawn_to) = if piece.reversed {
        (piece.b, piece.a)
    } else {
        (piece.a, piece.b)
    };
    let (from, to, y_from, y_to) = if sweep < 0.0 {
        (start + sweep, start, drawn_to.y, drawn_from.y)
    } else {
        (start, start + sweep, drawn_from.y, drawn_to.y)
    };
    let dy = p.y - center.y;
    let half = (radius * radius - dy * dy).max(0.0).sqrt();
    let mut inside = false;
    let (mut angle, mut y) = (from, y_from);
    let mut k = ((from - FRAC_PI_2) / PI).floor() + 1.0;
    // The top and bottom points lie half a turn apart, so one turn holds at most three of
    // them: the bound is never reached.
    for _ in 0..8 {
        let turn = FRAC_PI_2 + k * PI;
        if turn <= angle {
            k += 1.0;
            continue;
        }
        let last = turn >= to;
        let (next, y_next) = if last {
            (to, y_to)
        } else if (k as i64).rem_euclid(2) == 0 {
            (turn, center.y + radius)
        } else {
            (turn, center.y - radius)
        };
        let right = (0.5 * (angle + next)).cos() >= 0.0;
        inside ^= crosses(p, y, y_next, || {
            if right {
                center.x + half
            } else {
                center.x - half
            }
        });
        if last {
            break;
        }
        (angle, y) = (next, y_next);
        k += 1.0;
    }
    inside
}

/// Crossing-number containment of `p` in a watertight loop.
fn contains(sealed: &Sealed, p: Point2) -> bool {
    let mut inside = false;
    for piece in &sealed.pieces {
        inside ^= match piece.shape {
            Shape::Line => crosses(p, piece.a.y, piece.b.y, || {
                piece.a.x + (p.y - piece.a.y) * (piece.b.x - piece.a.x) / (piece.b.y - piece.a.y)
            }),
            Shape::Circle { center, radius } => {
                let dy = p.y - center.y;
                let half = (radius * radius - dy * dy).max(0.0).sqrt();
                let (bottom, top) = (center.y - radius, center.y + radius);
                crosses(p, bottom, top, || center.x + half)
                    ^ crosses(p, top, bottom, || center.x - half)
            }
            Shape::Arc {
                center,
                radius,
                start,
                sweep,
            } => arc_parity(p, piece, center, radius, start, sweep),
        };
    }
    inside
}

/// Per loop: the loops that contain it. `skip` loops neither contain nor are contained.
/// A container's bounds span those of what it contains, so the loops are swept by their low
/// edge on the wider-spread axis and only those still open there are tested.
pub(super) fn containers(
    loops: &[Sealed],
    skip: &[bool],
    budget: &mut Budget,
) -> Result<Vec<Vec<usize>>, ChainError> {
    let mut order: Vec<usize> = (0..loops.len())
        .filter(|&i| !skip.get(i).copied().unwrap_or(false))
        .collect();
    let on_y = sweep_on_y(
        order
            .iter()
            .filter_map(|&i| loops.get(i))
            .map(|sealed| (sealed.lo, sealed.hi)),
    );
    let along = |p: Point2| if on_y { p.y } else { p.x };
    let low = |i: usize| {
        loops
            .get(i)
            .map_or(f64::INFINITY, |sealed| along(sealed.lo))
    };
    order.sort_by(|&i, &j| low(i).total_cmp(&low(j)).then(i.cmp(&j)));
    let mut out = vec![Vec::new(); loops.len()];
    let mut active: Vec<usize> = Vec::new();
    for i in order {
        let Some(inner) = loops.get(i) else { continue };
        let Some(sample) = inner.pieces.first().map(|piece| piece.a) else {
            continue;
        };
        active.retain(|&j| {
            loops
                .get(j)
                .is_some_and(|outer| along(outer.hi) >= along(inner.lo))
        });
        budget.spend(active.len())?;
        for &j in &active {
            let Some(outer) = loops.get(j) else { continue };
            let around = outer.lo.x <= inner.lo.x
                && outer.lo.y <= inner.lo.y
                && outer.hi.x >= inner.hi.x
                && outer.hi.y >= inner.hi.y;
            if !around {
                continue;
            }
            budget.spend(outer.pieces.len())?;
            if contains(outer, sample)
                && let Some(list) = out.get_mut(i)
            {
                list.push(j);
            }
        }
        active.push(i);
    }
    Ok(out)
}
