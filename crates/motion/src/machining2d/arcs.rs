//! Split from `machining2d.rs` by M5.0.1; structure only.

use std::f64::consts::PI;
use crate::cl::{lattice, MoveKind, PathBuilder};
use crate::geo2d::Point2;
use crate::geometry::Point3;

use super::arc_side;
use super::contour::unit;
use super::ring::dist;
// The lead geometry moved to `arc_leads` (#689, the 400-line law); its
// importers keep reaching it here.
pub(super) use super::arc_leads::{lead_in, lead_out};
/// One piece of tool motion: a straight feed walk through the listed
/// points (exactly how generators have always walked polylines), or one
/// arc move ending at `to` about `center`. A [`Seg`] list describes a
/// walk starting where the builder's cursor already is, so endpoints
/// chain exactly.
#[derive(Clone, Debug, PartialEq)]
pub enum Seg {
    Line(Vec<Point2>),
    Arc { center: Point2, ccw: bool, to: Point2 },
}

/// Chord tolerance for arc fitting where no parameter carries one
/// (waterline rings, and the depth kinds without a `tolerance` field).
/// An arc may only replace a polyline if both sit within this of each
/// other - the same budget a chord's gouge allowance already spends.
pub const ARC_FIT_TOL: f64 = 0.01;

/// The share of the fit tolerance a vertex may stand off the fitted
/// circle (#689). A vertex off the circle means the arc passes that far
/// on the other side of a point the plan verified - toward the part as
/// often as away - and the verifier forgives the program exactly `tol`
/// against a mesh whose own chords already spend part of it. Half keeps
/// the fit's error toward the part inside the gouge verdict with the
/// other half left for the model's chords; the chord-hug test (the arc's
/// bow off its own chords) keeps the full `tol`, since an honest
/// tessellation's own sagitta is what that test measures.
pub const ARC_FIT_VERTEX_SHARE: f64 = 0.5;

/// Longest vertex run a single candidate arc may span: bounds the
/// greedy walk on pathological input (no honest tessellation needs it).
pub(super) const ARC_FIT_MAX_RUN: usize = 4096;

/// How many still-too-straight windows a run may look ahead for its
/// first claim of curvature. A finely tessellated circle only proves
/// "this is a curve" once the window spans enough of it - two vertices
/// into a 1000-gon look flat - and a greedy that gave up at the first
/// flat window would never fit such a rim at all. Straight runs pay
/// this cost once per start vertex and then walk on as lines; no
/// honest tessellation that a `tol`-budgeted arc could replace needs
/// more than a handful of these (sagitta grows quadratically in the
/// window), so the cap only ever bounds pathological flat input.
pub(super) const ARC_FIT_STRAIGHT_LOOKAHEAD: usize = 64;

/// Walk a polyline's pieces: lines feed through their points, arcs get
/// one [`PathBuilder::push_arc`] each. At cut level `z` with `feed`.
pub(crate) fn push_segs(b: &mut PathBuilder, segs: &[Seg], z: f64, feed: f64) {
    for seg in segs {
        match seg {
            Seg::Line(qs) => {
                for q in qs {
                    b.push(MoveKind::Feed, Point3::new(q.x, q.y, z), Some(feed));
                }
            }
            Seg::Arc { center, ccw, to } => {
                b.push_arc(MoveKind::Feed, Point3::new(to.x, to.y, z), Some(feed), *center, *ccw);
            }
        }
    }
}

/// Travel direction of a fitted walk at its front vertex (`front`) or
/// its last one: the true circle tangent where that end rides an arc,
/// the adjacent-vertex chord otherwise (a straight's chord IS its
/// tangent, and the polyline fallback covers degenerate walks). A lead
/// must leave or join the wall on this vector for the join to be smooth.
pub(super) fn walk_tangent(segs: &[Seg], poly: &[Point2], front: bool) -> Point2 {
    let n = poly.len();
    if n < 2 {
        return Point2::new(1.0, 0.0);
    }
    let chord = if front {
        unit(poly[1].x - poly[0].x, poly[1].y - poly[0].y)
    } else {
        unit(poly[n - 1].x - poly[n - 2].x, poly[n - 1].y - poly[n - 2].y)
    };
    let arc = match (front, segs.first(), segs.last()) {
        (true, Some(Seg::Arc { center, ccw, .. }), _) => Some((*center, *ccw, poly[0])),
        (false, _, Some(Seg::Arc { center, ccw, to })) => Some((*center, *ccw, *to)),
        _ => None,
    };
    match arc {
        Some((c, ccw, p)) => {
            let (dx, dy) = (p.x - c.x, p.y - c.y);
            let m = (dx * dx + dy * dy).sqrt();
            if m == 0.0 || !m.is_finite() {
                chord
            } else if ccw {
                Point2::new(-dy / m, dx / m)
            } else {
                Point2::new(dy / m, -dx / m)
            }
        }
        None => chord,
    }
}

/// Replace maximal arc-fittable runs of a polyline with arc moves,
/// keeping everything else point-for-point.
///
/// A run earns arcs only if both directions agree: every vertex lies
/// within [`ARC_FIT_VERTEX_SHARE`] of `tol` of one circle (the arc is no
/// lie about the tessellation, and its error toward the part leaves the
/// model's own chords room inside the gouge verdict - #689), and every
/// chord lies within `tol` of that circle's
/// arc (the arc is no surprise against the verified polygon) - so
/// trading chords for the arc moves the tool nowhere the plan didn't
/// already tolerate. Near-straight runs (all deviations within `tol` of
/// the end chord) stay lines on principle: a huge-radius "arc" along a
/// straight would re-describe motion the golden already spells point by
/// point - but a window that is merely too short to have proven its
/// curvature yet extends the run instead of ending it (bounded by
/// [`ARC_FIT_STRAIGHT_LOOKAHEAD`]), because a fine tessellation of a
/// real circle looks flat until the chord spans enough of it. Runs that
/// genuinely fail - corners, coarse tessellations, backtracks - end the
/// run, and a run that claimed nothing advances one vertex at a time,
/// which is exactly the old walk, so polygonal paths are byte-identical
/// through this function.
///
/// Sweeps over half a turn split at a vertex into <= pi pieces (the
/// builder refuses zero-sweep arcs, so an unsplit full circle would
/// erase its ring here); endpoints are always polyline points, so
/// chaining is exact by construction. Consumers fit at use; nothing
/// about a [`crate::geometry::Features`] digest ever depends on this.
pub fn fit_arcs(pts: &[Point2], tol: f64) -> Vec<Seg> {
    fit_arcs_sided(pts, tol, None)
}

/// [`fit_arcs`] for a wall walk that knows its part's side (#1579):
/// `air_left` `Some(true)` when the air (no part) is left of travel, the
/// contour family's `climb` convention. Every fitted arc then passes each
/// vertex of its run on the air side only (`arc_side`), so the fit never
/// cuts past a point the plan verified; `None` is the two-sided fit.
pub fn fit_arcs_sided(pts: &[Point2], tol: f64, air_left: Option<bool>) -> Vec<Seg> {
    let tol = if tol.is_finite() && tol > 0.0 { tol } else { ARC_FIT_TOL };
    let n = pts.len();
    let mut segs = Vec::new();
    let mut s = 0usize;
    while s + 1 < n {
        let cap = (s + ARC_FIT_MAX_RUN).min(n - 1);
        let mut best: Option<(usize, Point2, bool, f64)> = None;
        let mut e = s + 2;
        let mut flat = 0;
        let mut missed = 0;
        while e <= cap {
            match arc_over(&pts[s..=e], tol, air_left) {
                Over::Fit(center, ccw, total) => {
                    best = Some((e, center, ccw, total));
                    missed = 0;
                    e += 1;
                }
                // #1579: a sided fit refuses a run that ends on a vertex
                // short of the circle (a facet's flat) yet takes the run
                // one or two vertices on (the next corner): look past a
                // few refusals before the run ends.
                Over::Fail if air_left.is_some() && best.is_some() && missed < arc_side::ARC_SIDE_LOOKAHEAD => {
                    missed += 1;
                    e += 1;
                }
                // Too straight to have said anything yet: extend, up
                // to the lookahead's word.
                Over::Straight if flat < ARC_FIT_STRAIGHT_LOOKAHEAD => {
                    flat += 1;
                    e += 1;
                }
                Over::Straight | Over::Fail => break,
            }
        }
        match best {
            Some((e, center, ccw, total)) => {
                // Split at vertices into sub-sweeps of at most half a
                // turn; a closing full circle becomes two arcs meeting at
                // the far vertex, so the builder never sees the
                // zero-sweep single arc that would erase the ring.
                // M4.2b: a closed ring's `total` is 2*pi to the last
                // accumulated atan2 bit - glibc and the Windows CRT
                // disagree there, and a raw ceil(2.0 ± ULP) would let
                // the platform pick 2 or 3 splits for every ring in
                // the shop. The lattice swallows that noise (it is
                // 1e-13 here, a thousand times under the quantum)
                // and still honours a genuine fraction: a ratio that
                // really sits > 5e-7 above an integer wanted another
                // piece and keeps it.
                let k = split_count(total);
                for m in 1..=k {
                    let idx = if m == k { e } else { s + (e - s) * m / k };
                    segs.push(Seg::Arc {
                        center,
                        ccw,
                        to: pts[idx],
                    });
                }
                s = e;
            }
            None => {
                segs.push(Seg::Line(vec![pts[s + 1]]));
                s += 1;
            }
        }
    }
    segs
}

/// How many pieces of at most half a turn a fitted sweep `total` posts
/// as (see [`fit_arcs_sided`]).
fn split_count(total: f64) -> usize {
    (lattice(total / PI) + 1e-12).ceil().max(1.0) as usize
}

/// What a window of at least 3 points says about sitting on one circle
/// (see [`fit_arcs`]).
pub(super) enum Over {
    /// A circle within `tol`, both directions: (center, ccw, total
    /// sweep).
    Fit(Point2, bool, f64),
    /// Flatter than `tol` end to end - the window cannot tell arc from
    /// line yet, and the run's next, longer window might.
    Straight,
    /// Not one circle within `tol`.
    Fail,
}

/// The verdict on whether a run of >= 3 points sits on one circle
/// within `tol` in both directions, and if so which circle (see
/// [`fit_arcs`]). `Straight` is the honest answer when no vertex leaves
/// the end chord by more than `tol`: there is nothing there an arc
/// could describe better than the chords already do.
pub(super) fn arc_over(run: &[Point2], tol: f64, air_left: Option<bool>) -> Over {
    let (a, b) = (run[0], run[run.len() - 1]);
    let dev = |p: Point2| chord_dev(p, a, b);
    // Farthest interior vertex from the end chord anchors the circle.
    let mut pivot = None;
    let mut pd = 0.0;
    for p in &run[1..run.len() - 1] {
        let d = dev(*p);
        if !d.is_finite() {
            return Over::Fail;
        }
        if d > pd {
            pd = d;
            pivot = Some(*p);
        }
    }
    // Curvature must exceed the tolerance to justify an arc at all -
    // but a short window on a finely tessellated curve is no refusal,
    // just not yet evidence.
    let Some(pivot) = pivot.filter(|_| pd > tol) else {
        return Over::Straight;
    };
    // A run closing a full circle has an end chord of zero length - no
    // usable anchor at `b`. Second anchor: the vertex farthest from the
    // a-pivot line, which for an honest tessellation is a quarter turn
    // out. Either anchor pair defines the circle; an open run's turning
    // direction comes from the triple it was fitted on, a closing ring's
    // from its own signed area. The validation loops are the judges.
    // A run whose ends meet within `tol` is a closing ring, even when its
    // seam is a few ULP shy of exact: the end triple there is a
    // near-degenerate circle (#689), so it takes the anchor branch.
    let closing = dist(a, b) <= tol;
    let (c, r, ccw) = match (closing, circle3(a, pivot, b)) {
        (false, Some((c, r))) => (
            c,
            r,
            ((pivot.x - a.x) * (b.y - pivot.y) - (pivot.y - a.y) * (b.x - pivot.x)) > 0.0,
        ),
        (true, _) => {
            let mut second = None;
            let mut sd = 0.0;
            for p in &run[1..] {
                let d = chord_dev(*p, a, pivot);
                if d > sd && d.is_finite() {
                    sd = d;
                    second = Some(*p);
                }
            }
            let Some(anchor) = second.filter(|_| sd > tol) else {
                return Over::Straight;
            };
            let Some((c, r)) = circle3(a, pivot, anchor) else {
                return Over::Fail;
            };
            // The turning direction of a closing ring is its own signed
            // area, never the anchor triple: the two quarter-turn
            // vertices stand equally far off the a-pivot line, and when
            // the one picked lies BEFORE the pivot in travel the triple
            // reads the ring backwards - the fit then failed on its first
            // step and the ring's last few degrees walked as a chord the
            // lead-out crossed (#689, the Ø25 hole on Milling-test.step).
            (c, r, signed_area2(run) > 0.0)
        }
        (false, None) => return Over::Fail,
    };
    // #1579: a walk that knows its part's side keeps every vertex on the
    // air side of its arc (`arc_side::sided`).
    let Some((c, r)) = arc_side::sided(run, c, r, ccw, closing, air_left) else {
        return Over::Fail;
    };
    if !r.is_finite() || r <= tol {
        return Over::Fail;
    }
    // Direction one: vertices on the circle, within the vertex share of
    // `tol` (#689) - see ARC_FIT_VERTEX_SHARE.
    let vtol = tol * ARC_FIT_VERTEX_SHARE;
    for p in run {
        let d = (((p.x - c.x).powi(2) + (p.y - c.y).powi(2)).sqrt() - r).abs();
        if d > vtol {
            return Over::Fail;
        }
    }
    // Direction two: chords hug the arc; a step against the claimed
    // direction lands near a full turn here and fails the same test.
    let sag_bound = 1.0 - tol / r;
    let mut total = 0.0;
    for w in run.windows(2) {
        let t0 = (w[0].y - c.y).atan2(w[0].x - c.x);
        let t1 = (w[1].y - c.y).atan2(w[1].x - c.x);
        let step = if ccw { t1 - t0 } else { t0 - t1 };
        if !step.is_finite() {
            return Over::Fail;
        }
        let step = step.rem_euclid(2.0 * PI);
        if (step * 0.5).cos() < sag_bound {
            return Over::Fail;
        }
        total += step;
    }
    if total <= 0.0 || total > 2.0 * PI + tol / r {
        return Over::Fail;
    }
    // #1579: a sweep past half a turn posts as pieces that end on interior
    // vertices (`fit_arcs_sided`). The control and the verifier read each
    // piece from its own endpoints, so a split vertex off the circle bends
    // the piece between the checked vertices; a sided fit takes only split
    // vertices on the circle within the slack.
    if air_left.is_some() && !arc_side::splits_on_circle(run, c, r, split_count(total)) {
        return Over::Fail;
    }
    Over::Fit(c, ccw, total)
}

/// Twice the signed area the closed walk `run` encloses (shoelace):
/// positive for a counter-clockwise ring.
fn signed_area2(run: &[Point2]) -> f64 {
    let n = run.len();
    (0..n)
        .map(|i| {
            let (p, q) = (run[i], run[(i + 1) % n]);
            p.x * q.y - q.x * p.y
        })
        .sum()
}

/// Distance from `p` to segment `a`-`b`.
pub(super) fn chord_dev(p: Point2, a: Point2, b: Point2) -> f64 {
    let (dx, dy) = (b.x - a.x, b.y - a.y);
    let dd = dx * dx + dy * dy;
    if dd <= f64::EPSILON {
        return ((p.x - a.x).powi(2) + (p.y - a.y).powi(2)).sqrt();
    }
    let t = (((p.x - a.x) * dx + (p.y - a.y) * dy) / dd).clamp(0.0, 1.0);
    ((p.x - (a.x + t * dx)).powi(2) + (p.y - (a.y + t * dy)).powi(2)).sqrt()
}

/// Circle through three points: center and radius, or None when they
/// are (near-)collinear or land outside the finite horizon.
pub(super) fn circle3(a: Point2, b: Point2, c: Point2) -> Option<(Point2, f64)> {
    let d = 2.0 * (a.x * (b.y - c.y) + b.x * (c.y - a.y) + c.x * (a.y - b.y));
    if !d.is_finite() || d.abs() <= f64::EPSILON {
        return None;
    }
    let (a2, b2, c2) = (
        a.x * a.x + a.y * a.y,
        b.x * b.x + b.y * b.y,
        c.x * c.x + c.y * c.y,
    );
    let ux = (a2 * (b.y - c.y) + b2 * (c.y - a.y) + c2 * (a.y - b.y)) / d;
    let uy = (a2 * (c.x - b.x) + b2 * (a.x - c.x) + c2 * (b.x - a.x)) / d;
    if !ux.is_finite() || !uy.is_finite() {
        return None;
    }
    let o = Point2::new(ux, uy);
    let r = dist(a, o);
    if !r.is_finite() {
        return None;
    }
    Some((o, r))
}
