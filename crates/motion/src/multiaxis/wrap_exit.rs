//! The wrap space's world exit (M5.1 stage E2): a planned toolpath
//! living in developed coordinates (s, v, ρ) comes back onto and
//! around the fitted axis of [`super::axis`], posed by the same lean
//! law the rotary finishing kinds use.
//!
//! Ball centers map back at the plan's own ρ plus one tool radius (a
//! radial ball's CENTER stands r outside its tangent cylinder), and
//! the emitted point is the TIP under that center along the pose it
//! holds - on the cylinder itself for a pure radial (the M13 pivot
//! law, [`super::retip`]: the stream locates the tip, and a center
//! posted as a tip stood every level one radius off its skin). Every
//! wrap arc is chorded (no wrap circle is a world circle), and - the
//! law this exit adds to the twins' - no s-sweep is a world LINE
//! either: a wrap straight laps the axis, so each resampled chord
//! spans at most ARC_FIT_TOL of sagitta on the position circle.
//! Rapids state the pure radial (the post's tilt is modal, and a hop
//! lapping the bar with the last cut's fixed direction would swing
//! the tool through it); cut moves stand RADIAL with the lean, both
//! computed in world coordinates straight off the axis line from the
//! very center being emitted
//! (so the stamped axis and the developed point share bits and the
//! two frames pose identically), with [`super::tilt`]'s lean applied
//! about it. A radial feed - a dive leg running straight at the axis
//! - has no tangent plane to lead in; on a surface of revolution the
//!   plane is not missing, the azimuth supplies it, and the lead
//!   rides the constant-`s` plane the wrap itself lives in.
//!
//! Issue #9 (review): under Adaptive the developed XY arcs chord at an
//! ANGULAR cap as well as the sagitta ([`ADAPTIVE_TURN_DEG`] a chord,
//! end chords 0.01 mm of arc): the planner's ~1.5 mm lead and smoothing
//! arcs chorded by sagitta alone turn ~13 degrees at each chord vertex,
//! past the 5 degree in-cut law the developed plan was proved under.
//!
//! Provenance: unwrapped-to-developed toolpath reconstruction from
//! public multi-axis turning-milling literature; composed on this
//! codebase's own axis fit and lean laws. Nothing of any vendor's
//! consulted.

use crate::cl::{MoveKind, PathBuilder, Toolpath};
use crate::geo2d::Point2;
use crate::geometry::Point3;
use crate::machining2d::{GenError, ARC_FIT_TOL};

use super::axis::Axis;
use super::retip::tip_of;
use super::rotary_links;
use super::tilt::{command_axis, AxisControl};
use crate::cl::normalize_axis;

/// The most a developed arc's chord may turn from the last under an
/// Adaptive wrap: under the 5 degree in-cut kink law, with the arc's
/// end chords short ([`trim_ends`]) so a junction gains next to nothing.
pub(super) const ADAPTIVE_TURN_DEG: f64 = 4.0;

/// The world exit: `base` is the ladder's plan in wrap coordinates
/// (x = s, y = v, z = ρ), mapped and posed about `ax`; `rho_ref` is
/// the development's arc-length radius and `r` the tool radius the
/// plan was offset by. `turn_cap` (radians), when given, bounds the
/// sweep of each developed XY arc chord on top of the sagitta.
#[allow(clippy::too_many_arguments)]
pub(super) fn wrap_exit(
    base: &Toolpath,
    ax: &Axis,
    e1: &Point3,
    e2: &Point3,
    rho_ref: f64,
    r: f64,
    c: &AxisControl,
    turn_cap: Option<f64>,
) -> Result<Toolpath, GenError> {
    if !base.cycles.is_empty() {
        return Err(GenError::Invalid(
            "rotary offset cannot pose drill cycles - the spindle never stands still here".into(),
        ));
    }
    let radial = |s: f64| -> Point3 {
        let (sin, cos) = (s / rho_ref).sin_cos();
        Point3::new(
            e1.x * cos + e2.x * sin,
            e1.y * cos + e2.y * sin,
            e1.z * cos + e2.z * sin,
        )
    };
    let map = |pt: Point3| -> Point3 {
        let rc = pt.z + r;
        let n = radial(pt.x);
        Point3::new(
            ax.origin.x + pt.y * ax.dir.x + rc * n.x,
            ax.origin.y + pt.y * ax.dir.y + rc * n.y,
            ax.origin.z + pt.y * ax.dir.z + rc * n.z,
        )
    };
    // `map` names the BALL CENTER (the plan's level plus one radius,
    // out along the radial); the emitted point is the tip that center
    // stands on under the pose it holds ([`super::retip`]).
    // M13.8: the program opens ON the clearance pose the ladder's
    // opening traverse was heading for - the wrap's rapids cylinder
    // over the first cut's own column - holding the pure radial there,
    // never at the development's home (azimuth zero, station zero).
    if base.moves.is_empty() {
        return Err(GenError::Empty("the wrap ladder planned no moves".into()));
    }
    let (skip, at) = rotary_links::opening(base);
    let c0 = map(at);
    let d0 = c0 - ax.origin;
    let s0 = d0.dot(ax.dir);
    let pose0 = normalize_axis([d0.x - ax.dir.x * s0, d0.y - ax.dir.y * s0, d0.z - ax.dir.z * s0]);
    let t0 = tip_of(c0, pose0, r);
    let mut b = PathBuilder::new(t0);
    b.set_axis(pose0);
    b.push(MoveKind::Rapid, t0, None);
    let mut held: Option<[f64; 3]> = pose0;
    for mv in &base.moves[skip..] {
        let air = mv.kind == MoveKind::Rapid;
        let pose = |w: Point3, t: Point3, held: Option<[f64; 3]>| -> Option<[f64; 3]> {
            // The radial computed in world coordinates straight off
            // the axis line, from the very center being emitted - so
            // the stamped axis and the developed point share bits and
            // agree to the last ulp (the twins' pose law).
            let d = w - ax.origin;
            let s = d.dot(ax.dir);
            let foot = Point3::new(d.x - ax.dir.x * s, d.y - ax.dir.y * s, d.z - ax.dir.z * s);
            // On the axis (or worse) there is no radial to stand on:
            // the 1e-24 squared bound is set_axis's own refusal
            // threshold, so a held pose is always a direction.
            if !foot.is_finite() || foot.dot(foot) <= 1e-24 {
                if air {
                    return held; // a rapid keeps the tilt it has
                }
                tracing::warn!(
                    "multiaxis: a rotary offset cut stands ON the axis at {w:?}; the move holds vertical"
                );
                return Some([0.0, 0.0, 1.0]);
            }
            // A rapid laps the work with the head turning WITH it: the
            // post keeps the tilt modal, so a rapid that stated nothing
            // would sweep the last cut's fixed direction round the
            // bar. It states the pure radial - no travel to lead in.
            if air || (c.lead_deg == 0.0 && c.lag_deg == 0.0 && c.max_tilt_deg.is_none()) {
                // No lean asked: the radial stands, and it is stamped
                // from the RAW foot so the one normalization is the
                // vector's only division - the pose then matches the
                // position's own radial to the bit, not the ulp.
                return normalize_axis([foot.x, foot.y, foot.z]);
            }
            let n = match normalize(&foot) {
                Some(n) => n,
                None => return Some([0.0, 0.0, 1.0]),
            };
            // A radial feed (a dive leg runs straight at the axis) has
            // no tangent plane to lead in: the law would hold the
            // radial. On a surface of revolution the plane is not
            // missing - the azimuth supplies it: lead within the
            // constant-`s` plane, the same plane the wrap lives in.
            let hinge = n.cross(t);
            let t = if hinge.dot(hinge) <= 1e-18 * t.dot(t) {
                match normalize(&ax.dir.cross(n)) {
                    Some(a) => a,
                    None => t,
                }
            } else {
                t
            };
            match command_axis(n, t, c) {
                Some(a) => Some([a.x, a.y, a.z]),
                None => {
                    tracing::warn!(
                        "multiaxis: no lean exists at world point {w:?}; the move holds radial"
                    );
                    Some([n.x, n.y, n.z])
                }
            }
        };
        // The move's wrap polyline: an (s,v) arc chords at the arc-fit
        // tolerance with rho carried linearly; a line is its endpoints.
        let mut poly: Vec<Point3> = Vec::new();
        match &mv.arc {
            // XZ / YZ (M11.2 A): the arc's own 3-D chords.
            Some(a) if !a.plane.is_xy() => {
                poly.push(mv.from);
                poly.extend(mv.segments3(ARC_FIT_TOL).into_iter().map(|(_, e)| e));
            }
            Some(a) if turn_cap.is_some() => {
                let ch = mv.chords(arc_tol(mv.from, a.center, turn_cap));
                let mut pts: Vec<Point2> = vec![Point2::new(mv.from.x, mv.from.y)];
                pts.extend(ch.iter().map(|(_, e)| *e));
                trim_ends(&mut pts, a.center, a.ccw);
                // Z rides the chord length (a helix's descent stays
                // uniform along the travel; a planar arc keeps its z).
                let len: Vec<f64> = pts.windows(2).map(|w| (w[1].x - w[0].x).hypot(w[1].y - w[0].y)).collect();
                let total: f64 = len.iter().sum();
                let mut run = 0.0;
                poly.push(mv.from);
                for (i, q) in pts.iter().enumerate().skip(1) {
                    run += len.get(i - 1).copied().unwrap_or(0.0);
                    let f = if total > 0.0 { (run / total).clamp(0.0, 1.0) } else { 1.0 };
                    let z = if i + 1 == pts.len() { mv.to.z } else { mv.from.z + (mv.to.z - mv.from.z) * f };
                    poly.push(Point3::new(q.x, q.y, z));
                }
                if pts.len() < 2 {
                    poly.push(mv.to);
                }
            }
            Some(a) => {
                let ch = mv.chords(arc_tol(mv.from, a.center, turn_cap));
                let n = ch.len().max(1) as f64;
                poly.push(mv.from);
                for (i, (_a, e)) in ch.iter().enumerate() {
                    let zb = mv.from.z + (mv.to.z - mv.from.z) * ((i + 1) as f64 / n);
                    poly.push(Point3::new(e.x, e.y, zb));
                }
                if ch.is_empty() {
                    poly.push(mv.to);
                }
            }
            None => {
                poly.push(mv.from);
                poly.push(mv.to);
            }
        }
        for seg in poly.windows(2) {
            // No s-sweep is a world line: a wrap straight is a lap of
            // the fitted axis. Resample each piece so its world chord
            // spans the position circle within ARC_FIT_TOL of sagitta
            // (a full circle is TAU·rc/(2·cap)-ish chords); pure
            // v-steps, plunges and radial rapids have no s-run and
            // pass through as the single line they are.
            let (pa, pb) = (seg[0], seg[1]);
            let rc = (pa.z.max(pb.z) + r).max(f64::MIN_POSITIVE);
            let cap = 2.0 * (1.0 - (ARC_FIT_TOL / rc).min(2.0)).clamp(-1.0, 1.0).acos();
            let dth = (pb.x - pa.x).abs() / rho_ref.max(f64::MIN_POSITIVE);
            let n = if cap > 1e-12 && dth > cap {
                (dth / cap).ceil() as usize
            } else {
                1
            };
            let n = n.clamp(1, 8192);
            for i in 0..n {
                let at = |t: f64| {
                    Point3::new(
                        pa.x + (pb.x - pa.x) * t,
                        pa.y + (pb.y - pa.y) * t,
                        pa.z + (pb.z - pa.z) * t,
                    )
                };
                let (qa, qb) = (at(i as f64 / n as f64), at((i + 1) as f64 / n as f64));
                let (wf, wt) = (map(qa), map(qb));
                let a = pose(wt, wt - wf, held);
                b.set_axis(a);
                held = a;
                b.push(mv.kind, tip_of(wt, a, r), mv.feed);
            }
        }
    }
    let mut path = b.finish();
    // The plan's notes ride the wrap (M8.1's channel, this leg): an
    // entry substitution said on the developed path must still be
    // readable on the revolved one - the map re-emits every move, and
    // rebuilding the path without the notes would silence the very
    // verdict the entry law posts loudly.
    path.notes.extend(base.notes.iter().cloned());
    if !path.all_finite() {
        return Err(GenError::Invalid(
            "a rotary offset path left the finite horizon".into(),
        ));
    }
    Ok(path)
}

/// The chord sagitta for an XY arc about `o` from `from`: the arc-fit
/// tolerance, tightened under `turn_cap` so no chord sweeps more than
/// the cap (sagitta r(1 - cos(cap/2)) is a chord of exactly cap).
fn arc_tol(from: Point3, o: Point2, turn_cap: Option<f64>) -> f64 {
    let Some(cap) = turn_cap.filter(|c| c.is_finite() && *c > 0.0) else { return ARC_FIT_TOL };
    let ra = (from.x - o.x).hypot(from.y - o.y);
    let t = ra * (1.0 - (0.5 * cap).cos());
    if t.is_finite() && t > 0.0 { t.min(ARC_FIT_TOL) } else { ARC_FIT_TOL }
}

/// The posted end chords of a capped arc are short: an end chord stands
/// off the tangent the neighbouring move meets by half its sweep, so a
/// full-step end chord would add up to half the cap to a junction the
/// plan held just under the kink law. Each end chord is split at
/// [`END_SWEEP_MM`] of arc (never under a post's resolution) when it
/// sweeps more than twice that.
fn trim_ends(pts: &mut Vec<Point2>, o: Point2, ccw: bool) {
    let n = pts.len();
    if n < 2 {
        return;
    }
    let rot = |p: Point2, ang: f64| {
        let (sn, cs) = ang.sin_cos();
        let (dx, dy) = (p.x - o.x, p.y - o.y);
        Point2::new(o.x + dx * cs - dy * sn, o.y + dx * sn + dy * cs)
    };
    let sweep = |a: Point2, b: Point2| {
        let (u, v) = ((a.x - o.x, a.y - o.y), (b.x - o.x, b.y - o.y));
        (u.0 * v.1 - u.1 * v.0).abs().atan2(u.0 * v.0 + u.1 * v.1)
    };
    let dir = if ccw { 1.0 } else { -1.0 };
    let h_at = |p: Point2| {
        let ra = (p.x - o.x).hypot(p.y - o.y);
        if ra.is_finite() && ra > 0.0 { END_SWEEP_MM / ra } else { 0.0 }
    };
    let (first, last) = (pts[0], pts[n - 1]);
    let (hs, he) = (h_at(first), h_at(last));
    let tail = (he > 0.0 && sweep(pts[n - 2], last) > 2.0 * he).then(|| rot(last, -dir * he));
    if let Some(q) = tail {
        pts.insert(n - 1, q);
    }
    if hs > 0.0 && sweep(first, pts[1]) > 2.0 * hs {
        pts.insert(1, rot(first, dir * hs));
    }
}

/// The arc length of a capped arc's posted end chords.
const END_SWEEP_MM: f64 = 0.01;

fn normalize(v: &Point3) -> Option<Point3> {
    let l = (v.dot(*v)).sqrt();
    if !l.is_finite() || l <= f64::EPSILON {
        None
    } else {
        Some(Point3::new(v.x / l, v.y / l, v.z / l))
    }
}
