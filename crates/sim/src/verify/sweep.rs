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

use crate::cl::{Move, MoveKind, Toolpath, angles_to_axis, axis_to_angles, is_vertical, normalize_axis};
use crate::geo2d::Point2;
use crate::geometry::Point3;

use super::laws::dist2_seg;
use super::rest::Steps;
use super::types::Warning;

/// Path fractions sampled INSIDE the move (its endpoints are law (c)'s
/// domain - the to-point at the to-pose is (c) verbatim, and the
/// from-point was the previous move's to-point). Four points: the
/// same coarse-by-design sampling the rest of verify lives with, and
/// no fewer on a straight move, where (c) otherwise sees only the
/// endpoint. A pure-Z move keeps its XY and these become its Z
/// stations - the sliding-window case above.
const PATH_FRACS: [f64; 4] = [0.2, 0.4, 0.6, 0.8];
/// Pose fractions through the turn, endpoints INCLUDED: the head may
/// finish its arc before the table arrives (its leg is the short one
/// on a coordinated move), so "at the to-pose but not yet at the
/// to-point" is reachable, and on a straight move law (c) - which
/// samples the to-point only - would otherwise never see it.
const POSE_FRACS: [f64; 5] = [0.0, 0.25, 0.5, 0.75, 1.0];

/// The reachable-set sweep of one path: for every cut move that
/// CHANGES the commanded pose (the modal law threaded through the
/// path itself - rapids hold, an unposed cut is home), test the band
/// stack's law-(c) column over a grid of (path position, turn
/// fraction). Moves law (c) already convicted are skipped: their
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
    hidden: &dyn Fn(Point2, Point2, f64, f64, f64, usize) -> bool,
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
/// only; `None`: clean. The column stands at the sampled tip with
/// the lerped-pose lean - law (c)'s enclosure formula, pose by pose.
///
/// The 40 placements (4 path stations x 5 turn fractions x the live
/// bands) are enumerated once into a state list, and the list's own
/// bounding box filters the sample columns first: a band can only
/// touch a column its state's disc covers, and on a real fixture the
/// far-field majority of samples is outside every disc at once. The
/// grid's price is then 20x law (c)'s reach, not 20x its sample
/// count - which is what lets (c3) run for the Envelope programs,
/// where the posed kinds live. The columns' own answers ride the
/// path-wide `reach` memo (see [`mid_turn_sweeps`]): read once per
/// near column, never recomputed for the other 39 states that ask.
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
    hidden: &dyn Fn(Point2, Point2, f64, f64, f64, usize) -> bool,
) -> Option<bool> {
    // (pc, tip, r2, h0): the band's column, the tip whose kerf the
    // flutes cleared (law (c)'s inner edge - it stays at the tip when
    // the band leans off it, #903), its outer reach squared, its
    // bottom. Plus the union box of every disc, for the filter.
    let mut states: Vec<(Point2, Point2, f64, f64)> = Vec::new();
    let mut bands_at: Vec<(Point2, Point2, f64, f64)> = Vec::new();
    let (mut x0, mut y0) = (f64::INFINITY, f64::INFINITY);
    let (mut x1, mut y1) = (f64::NEG_INFINITY, f64::NEG_INFINITY);
    for &t in &PATH_FRACS {
        // XY walks the chord sequence (an arc's sweep is its chords,
        // the discretization every crossing/chord law lives with);
        // Z rides the move's own from-to lerp at the same parameter.
        let p = path_point(m, t, chord_tol);
        for &g in &POSE_FRACS {
            let abc = [
                from_abc[0] + (to_abc[0] - from_abc[0]) * g,
                from_abc[1] + (to_abc[1] - from_abc[1]) * g,
                from_abc[2] + (to_abc[2] - from_abc[2]) * g,
            ];
            let lean = angles_to_axis(abc).filter(|a| !is_vertical(a));
            for &(above, band_r, band_len) in bands {
                // Law (c)'s skip, read the same way: a band within the
                // cutter's radius is the flutes' kerf only upright (#903).
                if band_len <= 0.0 || band_r <= 0.0 || (lean.is_none() && band_r <= r) {
                    continue;
                }
                let (offx, offy, r_band, h0) = match lean {
                    Some(a) => {
                        let mid = above + band_len * 0.5;
                        let spread = (band_len * 0.5) * (a[0] * a[0] + a[1] * a[1]).sqrt();
                        let za = p.z + above * a[2];
                        let zb = p.z + (above + band_len) * a[2];
                        (mid * a[0], mid * a[1], band_r + spread, za.min(zb) - band_r)
                    }
                    None => (0.0, 0.0, band_r, p.z + above),
                };
                let pc = Point2::new(p.x + offx, p.y + offy);
                let tip = Point2::new(p.x, p.y);
                // #904: law (c)'s growth on a coarse model, read the
                // same way - the band reaches `grow` further.
                let r_band = r_band + grow;
                states.push((pc, tip, (r_band + 1e-9) * (r_band + 1e-9), h0));
                bands_at.push((pc, tip, r_band, h0));
                x0 = x0.min(pc.x - r_band);
                x1 = x1.max(pc.x + r_band);
                y0 = y0.min(pc.y - r_band);
                y1 = y1.max(pc.y + r_band);
            }
        }
    }
    if states.is_empty() {
        return None; // every band upright within the tool's radius: nothing to sweep
    }
    let near: Vec<Point2> = samples
        .iter()
        .copied()
        .filter(|q| q.x >= x0 && q.x <= x1 && q.y >= y0 && q.y <= y1)
        .collect();
    // This move's near columns, answers read once: the memo has
    // them, the three laws compute them on the first ask anywhere
    // in the path. Eager for every near column where the old loop
    // asked only after the annulus filter - legal because all three
    // laws are pure in q, so the early ask is the same float, the
    // same bool, the old lazy ask would have got. The state loop
    // then costs a distance and three compares per pair whatever
    // the column laws' own price; and reading `in_stock` before the
    // annulus is a legal reordering of filters over one predicate
    // set - the worst-offender max cannot see the difference.
    let mut vals: Vec<(Point2, bool, f64, bool)> = Vec::with_capacity(near.len());
    for q in near {
        let k = (q.x.to_bits(), q.y.to_bits());
        let v = reach.entry(k).or_insert_with(|| (in_stock(q), surface(q), part_at(q)));
        vals.push((q, v.0, v.1.before(upto), v.2));
    }
    let inner2 = (r + gouge_tol) * (r + gouge_tol);
    let mut worst = None;
    for &(pc, tip, r2, h0) in &states {
        for &(q, instk, zq, part) in &vals {
            if !instk {
                continue; // outside the stock for every state's annulus
            }
            if dist2_seg(q, pc, pc) > r2 || dist2_seg(q, tip, tip) <= inner2 {
                continue; // outside this band's annulus
            }
            if h0 < zq {
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
    if worst.is_none() && bands_at.iter().any(|&(pc, tip, rb, h0)| hidden(pc, tip, r + gouge_tol, rb, h0, upto)) {
        worst = Some(false);
    }
    worst
}

/// The point `t` along the move: XY by cumulative chord walk, Z on
/// the move's own linear ramp. A zero-XY move keeps its column and
/// the Z lerp does the work - which is the whole point for a plunge
/// that turns while it descends.
fn path_point(m: &Move, t: f64, chord_tol: f64) -> Point3 {
    // An XZ / YZ arc (M11.2 A) moves in Z along its sweep, not on the
    // endpoints' lerp: its own exact point at the sweep fraction.
    if let Some(a) = m.arc.filter(|a| !a.plane.is_xy())
        && let Some(p) = a.point_at(m.from, m.to, t)
    {
        return p;
    }
    let chords = m.chords(chord_tol);
    let mut total = 0.0;
    for (a, b) in &chords {
        total += ((b.x - a.x) * (b.x - a.x) + (b.y - a.y) * (b.y - a.y)).sqrt();
    }
    let z = m.from.z + (m.to.z - m.from.z) * t;
    // `!(total > 0.0)` in NaN-honest form: a non-positive OR
    // non-finite walk parks the column at the move's start.
    if total <= 0.0 || !total.is_finite() || chords.is_empty() {
        return Point3::new(m.from.x, m.from.y, z);
    }
    let target = t * total;
    let mut acc = 0.0;
    for (a, b) in &chords {
        let l = ((b.x - a.x) * (b.x - a.x) + (b.y - a.y) * (b.y - a.y)).sqrt();
        if acc + l >= target {
            let f = if l > 0.0 {
                ((target - acc) / l).clamp(0.0, 1.0)
            } else {
                0.0
            };
            return Point3::new(a.x + (b.x - a.x) * f, a.y + (b.y - a.y) * f, z);
        }
        acc += l;
    }
    Point3::new(m.to.x, m.to.y, z)
}
