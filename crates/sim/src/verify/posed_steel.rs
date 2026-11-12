//! The posed sweep of the fixture laws (c2)/(d2) (M13.8): every move
//! the machine runs with its tool off the upright - a radial retract,
//! an index on the retract cylinder, a plunge back in, a tilted cut -
//! swept with its REAL pose against the fixture boxes, the whole stack
//! (flute, shank, holder, the spindle housing when stated) as tilted
//! capsules along the pose, not a vertical column standing above the
//! tip. A tool pointing down under a bar's equator reaches toward the
//! chuck, the tailstock, the table; the column enclosures of
//! [`super::laws`] look upward from the move's deepest point and the
//! rapid enclosure reads only the flute, so a holder swept through a
//! jaw on an index passed silent. This law closes that.
//!
//! The pose along a move is the controller's: the move starts on the
//! pose held coming in (the tilt is modal - a move stating nothing
//! keeps it) and ends on the one it states, the letters lerped
//! between ([`crate::cl::pose`]), the tip on its straight line (an
//! arc on its own 3-D chords). The walk is the mid-move pose law's
//! ([`super::midpose`], the one law the part's solid and contact
//! readings walk too): each move cut into intervals no longer than a
//! half millimetre of travel at the stack's far end; per interval the
//! stack is placed at the midpoint and every capsule's radius grows by
//! the farthest any of its points can travel within the interval - the
//! tip's travel plus reach times the interval's proven swing, which
//! bounds EVERY pose inside the interval, not only its two ends - so
//! the capsules ENCLOSE the swept volume continuously: an over-report
//! of a graze, never a hidden crash between two samples. On an arc the
//! tip's travel also takes the chord's bulge ([`super::arc_bulge`]):
//! the true tip stands off its chord by up to the sagitta (#491). A capsule
//! encloses its cylinder (its caps reach one radius past each end),
//! the same err-toward-HIT direction. The segment-box distance is
//! exact: the distance to a convex box along a segment is convex in
//! the segment parameter, minimized by ternary search.
//!
//! Upright moves (held and stated pose both upright, or unstated) are
//! not this law's: the prismatic laws keep them byte-identical. A
//! posed move whose pose does not change walks the same intervals with
//! no swing - its reading is the one it had.
//!
//! Provenance: swept-volume enclosure by bounded sampling and
//! capsule/AABB distance are textbook computational geometry (Ericson,
//! "Real-Time Collision Detection", public material); the modal pose
//! law is RS-274's. Our own design; no vendor source consulted.

use crate::cl::{is_vertical, MoveKind, Toolpath};
use crate::fixture::{FixtureBox, FIXTURE_HIT_TOL};
use crate::geometry::Point3;
use crate::tools::ToolGeom;
use camcraft_tools::holder_profile::HolderStack;

use super::arc_bulge::Bulge;
use super::midpose;
use super::rotary::RotaryAxis;
use super::types::FixturePart;

/// Ternary-search rounds for the segment-box distance: (2/3)^90 of
/// the segment, far below the hit tolerance on any real move.
const ROUNDS: usize = 90;

/// Per move, the element a POSED move strikes a fixture with (first in
/// the order shank, holder, spindle, flute), None where it strikes
/// nothing or is not posed. `rapids` picks the move kinds judged: the
/// rapids (law d2, reported as [`FixturePart::Rapid`]) or every other
/// move (law c2).
pub(super) fn posed_hits(
    path: &Toolpath,
    tool: &ToolGeom,
    holder: &HolderStack,
    fixtures: &[FixtureBox],
    spindle: Option<(f64, f64)>,
    chord_tol: f64,
    rapids: bool,
) -> Vec<Option<FixturePart>> {
    sweep(path, tool, holder, fixtures, spindle, chord_tol, rapids, None)
        .into_iter()
        .map(|v| match v {
            Verdict::Hit(p) => Some(p),
            // Unreachable without a frame; were it reached it fails
            // closed - never silent.
            Verdict::Unjudged => Some(if rapids { FixturePart::Rapid } else { FixturePart::Flute }),
            Verdict::NotPosed | Verdict::Clear => None,
        })
        .collect()
}

/// One posed sweep's verdict on one move.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Verdict {
    /// Upright (or the other law's move kind): not this law's.
    NotPosed,
    /// Posed, but some sample's machine frame cannot be stated (the
    /// pose leaves the rotary's plane, or the rotary is not
    /// horizontal): the caller keeps its own path-frame verdict.
    Unjudged,
    /// Posed and swept: no box met.
    Clear,
    /// Posed and swept: this element met a box first.
    Hit(FixturePart),
}

/// The posed sweep, in the path frame (`frame` None: the boxes ride
/// the part, a chuck) or in the MACHINE frame of a rotary turning the
/// part about `frame` (M13.8 (2), a table or tailstock that does not
/// turn): each interval's stack is carried through the rotation that
/// stands its pose upright - the turn the machine itself must make -
/// and judged against boxes stated at rotary zero.
#[allow(clippy::too_many_arguments)]
pub(super) fn sweep(
    path: &Toolpath,
    tool: &ToolGeom,
    holder: &HolderStack,
    fixtures: &[FixtureBox],
    spindle: Option<(f64, f64)>,
    chord_tol: f64,
    rapids: bool,
    frame: Option<&RotaryAxis>,
) -> Vec<Verdict> {
    let mut out = vec![Verdict::NotPosed; path.moves.len()];
    if fixtures.is_empty() {
        return out;
    }
    let fl = tool.flute_len;
    // #852: the shank as swept - a drawn neck's bands, or the cylinder.
    let mut stack: Vec<(FixturePart, f64, f64, f64)> =
        holder.shank_bands(tool).iter().map(|b| (FixturePart::Shank, b.above, b.above + b.len, b.r)).collect();
    // What stands above the shank (#368): the holder cylinder - the
    // same sums as ever - or a modelled body's bands.
    stack.extend(holder.bands.iter().map(|b| (FixturePart::Holder, b.above, b.above + b.len, b.r)));
    if let Some((sr, slen)) = spindle {
        stack.push((FixturePart::Spindle, holder.top, holder.top + slen, sr));
    }
    stack.push((FixturePart::Flute, 0.0, fl, tool.radius));
    // A band of no length is no steel; a non-finite one fails closed
    // (kept, and the distance law reads NaN as a hit).
    stack.retain(|&(_, lo, hi, r)| hi - lo > 0.0 || !(lo.is_finite() && hi.is_finite() && r.is_finite()));
    let reach = stack.iter().map(|e| e.2).fold(0.0f64, |a, b| if b.is_finite() { a.max(b) } else { a });
    let tol = if chord_tol.is_finite() && chord_tol > 0.0 { chord_tol } else { 0.01 };
    let upright = |a: Option<[f64; 3]>| a.is_none_or(|a| is_vertical(&a));
    let mut held: Option<[f64; 3]> = path.moves.first().and_then(|m| m.axis);
    for (i, m) in path.moves.iter().enumerate() {
        let from_pose = held;
        let to_pose = m.axis.or(held);
        held = to_pose;
        if (m.kind == MoveKind::Rapid) != rapids || (upright(from_pose) && upright(to_pose)) {
            continue;
        }
        let a = from_pose.unwrap_or([0.0, 0.0, 1.0]);
        let b = to_pose.unwrap_or([0.0, 0.0, 1.0]);
        let hit = |part: FixturePart| Verdict::Hit(if rapids { FixturePart::Rapid } else { part });
        // A move that is nowhere has no intervals: it fails closed on
        // the first band, as the NaN capsule did (no band: no steel).
        if !(m.from.is_finite() && m.to.is_finite()) {
            out[i] = stack.first().map_or(Verdict::Clear, |e| hit(e.0));
            continue;
        }
        // An arc's true tip strays off its chords by up to the bulge
        // (#491): every capsule of the move grows by it (zero on a
        // straight move, which keeps its reading).
        let bulge = if m.arc.is_some() { Bulge::of(m, m.segments3(tol).len()).deviation() } else { 0.0 };
        let mut struck = Verdict::Clear;
        'walk: for iv in midpose::intervals(m, a, b, reach, tol) {
            let travel = dist(iv.pm, iv.p0).max(dist(iv.pm, iv.p1)) + bulge;
            // The machine frame: the turn standing `am` upright, and
            // the most the rotary turns off it within the interval -
            // the end poses' turns, and never under the proven swing
            // (an in-plane pose turns the rotary as fast as itself).
            let turn = match frame {
                None => None,
                Some(ax) => match (upright_turn(ax, iv.am), upright_turn(ax, iv.a0), upright_turn(ax, iv.a1)) {
                    (Some(m), Some(t0), Some(t1)) => {
                        Some((ax, m, wrap(m - t0).abs().max(wrap(m - t1).abs()).max(iv.swing)))
                    }
                    _ => {
                        struck = Verdict::Unjudged;
                        break 'walk;
                    }
                },
            };
            for &(part, lo, hi, r) in &stack {
                let grow = travel + hi.abs() * iv.swing;
                let mut c0 = add(iv.pm, iv.am, lo);
                let mut c1 = add(iv.pm, iv.am, hi);
                let mut rad = r + grow;
                if let Some((ax, th, dth)) = turn {
                    // Every point the path-frame capsule holds sits no
                    // farther than `rho` off the axis (distance to a
                    // line is convex along a segment); a turn of up to
                    // `dth` moves it at most `rho * dth` (arc >= chord).
                    let rho = off_axis(ax, c0).max(off_axis(ax, c1)) + rad;
                    rad += rho * dth;
                    c0 = turn_about(ax, c0, th);
                    c1 = turn_about(ax, c1, th);
                }
                if fixtures.iter().any(|b| capsule_hits(c0, c1, rad, b)) {
                    struck = hit(part);
                    break 'walk;
                }
            }
        }
        out[i] = struck;
    }
    out
}

/// Does the capsule `c0`-`c1` of radius `r` meet the box? Distance
/// within `r` (plus the hit tolerance) counts; anything non-finite
/// fails closed.
fn capsule_hits(c0: Point3, c1: Point3, r: f64, b: &FixtureBox) -> bool {
    if !(c0.is_finite() && c1.is_finite() && r.is_finite() && b.min.is_finite() && b.max.is_finite()) {
        return true;
    }
    let lo = [b.min.x.min(b.max.x), b.min.y.min(b.max.y), b.min.z.min(b.max.z)];
    let hi = [b.max.x.max(b.min.x), b.max.y.max(b.min.y), b.max.z.max(b.min.z)];
    let (p, q) = ([c0.x, c0.y, c0.z], [c1.x, c1.y, c1.z]);
    let lim = r.max(0.0) + FIXTURE_HIT_TOL;
    // Quick reject: the segment's box grown by the radius misses.
    for k in 0..3 {
        if p[k].min(q[k]) - lim > hi[k] || p[k].max(q[k]) + lim < lo[k] {
            return false;
        }
    }
    let d2 = |t: f64| {
        (0..3)
            .map(|k| {
                let x = p[k] + (q[k] - p[k]) * t;
                let e = if x < lo[k] { lo[k] - x } else if x > hi[k] { x - hi[k] } else { 0.0 };
                e * e
            })
            .sum::<f64>()
    };
    let (mut a, mut z) = (0.0f64, 1.0f64);
    for _ in 0..ROUNDS {
        let m1 = a + (z - a) / 3.0;
        let m2 = z - (z - a) / 3.0;
        if d2(m1) <= d2(m2) {
            z = m2;
        } else {
            a = m1;
        }
    }
    let best = d2(0.0).min(d2(1.0)).min(d2(0.5 * (a + z)));
    best <= lim * lim
}

/// Pose and rotary within this of the rotary's plane / of horizontal:
/// the rotary alone stands the pose upright.
const PLANE_TOL: f64 = 1e-6;

/// The signed turn about `ax` carrying the unit pose `a` onto machine
/// +Z - unique, both lying in the plane normal to the axis, so it is
/// the turn any controller makes whatever its sign convention. None
/// when the rotary alone cannot stand the pose upright (the pose out
/// of the plane, a non-horizontal or degenerate axis): the sample's
/// machine frame is unknown.
pub(super) fn upright_turn(ax: &RotaryAxis, a: [f64; 3]) -> Option<f64> {
    let d = unit3(ax.dir)?;
    if d[2].abs() > PLANE_TOL || dot3(a, d).abs() > PLANE_TOL {
        return None;
    }
    let z = [0.0, 0.0, 1.0];
    let t = dot3(d, cross3(a, z)).atan2(dot3(a, z));
    t.is_finite().then_some(t)
}

/// `t` folded into (-pi, pi].
pub(super) fn wrap(t: f64) -> f64 {
    use std::f64::consts::{PI, TAU};
    let w = t.rem_euclid(TAU);
    if w > PI { w - TAU } else { w }
}

/// Distance of `p` from the axis line; a degenerate axis reads as
/// unbounded, which the capsule test fails closed on.
pub(super) fn off_axis(ax: &RotaryAxis, p: Point3) -> f64 {
    let Some(d) = unit3(ax.dir) else { return f64::INFINITY };
    let w = [p.x - ax.origin.x, p.y - ax.origin.y, p.z - ax.origin.z];
    let c = cross3(w, d);
    dot3(c, c).sqrt()
}

/// `p` turned by `t` about the axis line (Rodrigues' formula).
pub(super) fn turn_about(ax: &RotaryAxis, p: Point3, t: f64) -> Point3 {
    let Some(d) = unit3(ax.dir) else { return Point3::new(f64::NAN, f64::NAN, f64::NAN) };
    let w = [p.x - ax.origin.x, p.y - ax.origin.y, p.z - ax.origin.z];
    let (s, c) = t.sin_cos();
    let k = cross3(d, w);
    let e = dot3(d, w) * (1.0 - c);
    Point3::new(
        ax.origin.x + w[0] * c + k[0] * s + d[0] * e,
        ax.origin.y + w[1] * c + k[1] * s + d[1] * e,
        ax.origin.z + w[2] * c + k[2] * s + d[2] * e,
    )
}

fn dot3(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn cross3(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}

fn unit3(a: [f64; 3]) -> Option<[f64; 3]> {
    let l = dot3(a, a).sqrt();
    (l.is_finite() && l > 1e-12).then(|| [a[0] / l, a[1] / l, a[2] / l])
}

fn add(p: Point3, a: [f64; 3], s: f64) -> Point3 {
    Point3::new(p.x + a[0] * s, p.y + a[1] * s, p.z + a[2] * s)
}

fn dist(a: Point3, b: Point3) -> f64 {
    ((b.x - a.x).powi(2) + (b.y - a.y).powi(2) + (b.z - a.z).powi(2)).sqrt()
}
