//! Law (c3): the mid-turn sweep (M5.1 stage G).
//!
//! Provenance: the coordinated-motion reachable set - every axis
//! group's interpolation law is "each family runs its own clock and
//! holds at arrival", the controller model stated in public G-code
//! manuals' descriptions of simultaneous linear+rotary blocks
//! (behavior only, per the Clean-Room Protocol). The enclosure test
//! reuses law (c)'s column exactly; the grid is ours, and its limits
//! are stated where it runs.
//!
//! Law (c) judges the band stack at the poses each move STATES. That
//! was the whole story while every move stated home vertical. Since
//! stage F a move can command a new A/B/C pose - and a coordinated
//! block drives the table and the head AT ONCE, so between the start
//! and end poses the machine passes through every lerped letter
//! combination, at every point along the path, in EITHER family's
//! order of arrival. A wall both endpoint poses clear can still be
//! struck mid-turn: the column swings like a metronome while the tip
//! walks. And the endpoints themselves are not enough even at fixed
//! XY - a plunge that turns while it descends slides its lean column
//! through a Z window that is the union (a superset) of the two
//! endpoints' windows. This law names what meets steel in between.

use crate::cl::{Move, MoveKind, Toolpath, axis_to_angles, normalize_axis};
use crate::geo2d::Point2;

use super::rest::Steps;
use super::turn_cells::{Cell, push_cells};
use super::turn_hidden::backstop_asks;
use super::types::Warning;

/// The turn fractions #1303's span floors are read at, endpoints
/// INCLUDED (the sampled sweep's poses before #1313): the cells holding
/// one read no higher than its floor ([`band_floors`]).
const POSE_FRACS: [f64; 5] = [0.0, 0.25, 0.5, 0.75, 1.0];

/// The overhang backstop's ask ([`super::hidden`]'s `hidden_in_cover`):
/// (box, lowest plane, footprint probes - built on demand -, the
/// footprint's membership and the heights its bottom takes over a
/// point, judged move).
pub(super) type Hidden<'a> =
    dyn Fn((Point2, Point2), f64, &dyn Fn() -> Vec<Point2>, &super::hidden::Heights<'_>, usize) -> bool + 'a;

/// The reachable-set sweep of one path: for every cut move that
/// CHANGES the commanded pose (the modal law threaded through the
/// path itself - rapids hold, an unposed cut is home), test the band
/// stack's law-(c) column over the whole rectangle of (path position,
/// turn fraction) - swept cells, not a grid (#1313). Moves law (c) already convicted are skipped: their
/// same-band verdict is spoken, and this law exists for what (c)
/// finds clean. One [`Warning::SweepHit`] per move; part material
/// outranks stock, like (c)'s worst-offender rule.
///
/// The three column laws (`in_stock`, `surface`, `part_at`) are pure
/// functions of the sample column alone - the cut list, the island
/// slices and the stock are fixed for the whole path - so their
/// answers ride a memo keyed by the column's exact bits, built once
/// (the surface as its cut-order staircase, M13.7: move i reads the
/// column as the cuts up to and including move i left it, never a
/// later move's clearing - the same answer `surface_before` gives)
/// per path and read by every move's `swing_hit`. Without it the
/// grid's price is states x samples x PATH per move: a flowline's
/// 800 posed moves ask the same ten-odd thousand columns forty
/// times apiece, and the verdict spent minutes reading the same
/// answer. The memo cannot move a verdict: it is the same float, the
/// same bool, and the max-severity rule (part outranks stock outranks
/// clean) does not care what order the pairs are seen in.
/// `gouge_tol` is the kerf slack about the tip and `grow` the band
/// growth, both as law (c) reads them on a coarse model (#904).
#[allow(clippy::too_many_arguments)]
pub(super) fn mid_turn_sweeps(
    path: &Toolpath,
    bands: &[(f64, f64, f64)],
    r: f64,
    (gouge_tol, grow): (f64, f64),
    chord_tol: f64,
    shanked: &[usize],
    samples: &[Point2],
    in_stock: &dyn Fn(Point2) -> bool,
    surface: &dyn Fn(Point2) -> Steps,
    part_at: &dyn Fn(Point2) -> bool,
    hidden: &Hidden<'_>,
) -> Vec<Warning> {
    let mut warns = Vec::new();
    // The column memo, one per path (see the module law above): a
    // sample's (in stock, surface height, part?) truth, keyed by the
    // column's exact bits. Eager per column seen near any move's
    // sweep; the three laws are pure, so asking early asks the same
    // answer the old lazy ask would have got.
    let mut reach: std::collections::HashMap<(u64, u64), (bool, Steps, bool)> = std::collections::HashMap::new();
    // The held letters, exactly as `sim::script` threads them (the
    // same modal law, the same canonical form): a path's first move
    // turns FROM home - the cross-operation swing belongs to the
    // document's replay timeline, not to one path's verdict, and the
    // connector rapids between operations command no angle anyway.
    let mut held = [0.0f64; 3];
    for (i, m) in path.moves.iter().enumerate() {
        let next = if m.kind == MoveKind::Rapid || !(m.from.is_finite() && m.to.is_finite()) {
            held
        } else {
            m.axis
                .and_then(normalize_axis)
                .map_or([0.0; 3], axis_to_angles)
        };
        if m.kind != MoveKind::Rapid
            && held != next
            && !shanked.contains(&i)
            && let Some(part) = swing_hit(
                m, i + 1, held, next, bands, r, (gouge_tol, grow), chord_tol, samples, &mut reach, in_stock, surface, part_at, hidden,
            )
        {
            warns.push(Warning::SweepHit { at: i, part });
        }
        held = next;
    }
    warns
}

/// Does the swing's reachable set meet steel? `Some(true)`: part
/// material (worst offender, search ended); `Some(false)`: stock
/// only; `None`: clean.
///
/// The reachable set is every tip position of the move under every
/// pose of the turn (either family may arrive first). Each band's
/// law-(c) column is enclosed over it by [`super::turn_cells`]: one
/// cell per tip chord and interval of the turn, holding every column
/// between its end poses with a stated sag (at most
/// [`super::turn_cells::SAG`] until the cell cap). A sample column
/// convicts when some position of a cell puts the band over it,
/// outside the flutes' kerf at that same tip position, with the
/// band's bottom under the column's surface. Nothing between two
/// samples of the turn is left unread (#1313: five poses at four
/// stations let a band cross a wall between them).
///
/// The cells' union box filters the sample columns first, and the
/// columns' own answers ride the path-wide `reach` memo (see
/// [`mid_turn_sweeps`]): read once per near column.
#[allow(clippy::too_many_arguments)]
fn swing_hit(
    m: &Move,
    upto: usize,
    from_abc: [f64; 3],
    to_abc: [f64; 3],
    bands: &[(f64, f64, f64)],
    r: f64,
    (gouge_tol, grow): (f64, f64),
    chord_tol: f64,
    samples: &[Point2],
    reach: &mut std::collections::HashMap<(u64, u64), (bool, Steps, bool)>,
    in_stock: &dyn Fn(Point2) -> bool,
    surface: &dyn Fn(Point2) -> Steps,
    part_at: &dyn Fn(Point2) -> bool,
    hidden: &Hidden<'_>,
) -> Option<bool> {
    // The tip's own path: a straight move, or an arc's chords with
    // their Z (the discretization every chord law lives with).
    let pieces = m.segments3(chord_tol);
    let mut cells: Vec<Cell> = Vec::new();
    for &band in bands {
        let floors = band_floors(from_abc, to_abc, band);
        push_cells(&mut cells, &pieces, from_abc, to_abc, band, r, grow, &floors);
    }
    if cells.is_empty() {
        return None; // every band upright within the tool's radius: nothing to sweep
    }
    let (mut x0, mut y0) = (f64::INFINITY, f64::INFINITY);
    let (mut x1, mut y1) = (f64::NEG_INFINITY, f64::NEG_INFINITY);
    for c in &cells {
        x0 = x0.min(c.lo.x);
        y0 = y0.min(c.lo.y);
        x1 = x1.max(c.hi.x);
        y1 = y1.max(c.hi.y);
    }
    // This move's near columns in the stock, answers read once: the
    // memo has them, the three laws compute them on the first ask
    // anywhere in the path (all three are pure in q).
    let mut vals: Vec<(Point2, f64, bool)> = Vec::new();
    for &q in samples.iter().filter(|q| q.x >= x0 && q.x <= x1 && q.y >= y0 && q.y <= y1) {
        let k = (q.x.to_bits(), q.y.to_bits());
        let v = reach.entry(k).or_insert_with(|| (in_stock(q), surface(q), part_at(q)));
        if v.0 {
            vals.push((q, v.1.before(upto), v.2));
        }
    }
    let kerf2 = (r + gouge_tol) * (r + gouge_tol);
    let mut worst = None;
    for c in &cells {
        for &(q, zq, part) in &vals {
            if q.x < c.lo.x || q.x > c.hi.x || q.y < c.lo.y || q.y > c.hi.y {
                continue; // outside this cell's box
            }
            if worst.is_some() && !part {
                continue; // a stock column cannot raise a stock verdict
            }
            if c.bottom_over(q, kerf2).is_some_and(|h| h < zq) {
                if part {
                    return Some(true); // report the worst offender
                }
                worst = Some(false);
            }
        }
    }
    // The overhang backstop (M13.7 B, [`super::hidden`]): stock hidden
    // under a ledge is in no column sample; asked only when the column
    // law found nothing, so it adds convictions, never removes one.
    // Since #1316 it reads the same reachable set, on its own coarser
    // cells and a stated plane budget ([`super::turn_hidden`]) - and
    // every column over the whole range of heights the band's bottom
    // takes there, because hidden stock is not monotone in the plane.
    if worst.is_none()
        && backstop_asks(&pieces, from_abc, to_abc, bands, r, grow)
            .iter()
            .any(|a| hidden((a.lo, a.hi), a.plane, &|| a.probes(), &|q, hit| a.heights(q, kerf2, hit), upto))
    {
        worst = Some(false);
    }
    worst
}
/// The letters at each of [`POSE_FRACS`], lerped from `from` to `to`.
fn pose_letters(from: [f64; 3], to: [f64; 3]) -> [[f64; 3]; 5] {
    POSE_FRACS.map(|g| {
        [from[0] + (to[0] - from[0]) * g, from[1] + (to[1] - from[1]) * g, from[2] + (to[2] - from[2]) * g]
    })
}

/// #1303's span floor at the pose `k` of `poses`, over the tip: the
/// lowest the band can reach between that pose and either neighbour
/// where the lerp can lean past both samples ([`super::tilt_span`]);
/// `+inf` inside the cap, where the samples bound the span.
fn span_floor_at(poses: &[[f64; 3]; 5], k: usize, (above, band_r, band_len): (f64, f64, f64)) -> f64 {
    let span = |j: usize| super::tilt_span::span_floor(&poses[j], &poses[k], above, band_r, band_len);
    [k.checked_sub(1), Some(k + 1).filter(|&j| j < poses.len())]
        .into_iter()
        .flatten()
        .filter_map(span)
        .fold(f64::INFINITY, f64::min)
}

/// The span floors of one band at the turn fractions [`POSE_FRACS`],
/// `(g, floor over the tip)`: what the sampled sweep before #1313 read
/// at those poses. A turn cell holding a fraction reads no higher
/// ([`push_cells`]), so the cell law convicts all the sampled law did;
/// the overhang backstop's coarser cells hold the same floors
/// ([`super::turn_hidden`]), so it convicts all the sampled backstop did.
pub(super) fn band_floors(from: [f64; 3], to: [f64; 3], band: (f64, f64, f64)) -> [(f64, f64); 5] {
    let poses = pose_letters(from, to);
    std::array::from_fn(|k| (POSE_FRACS[k], span_floor_at(&poses, k, band)))
}

#[cfg(test)]
#[path = "sweep_tests.rs"]
mod tests;
