//! The rotary kinds' start and link laws (M13.8): where a radial
//! program begins, and how Rotary Parallel leaves a pass and finds the
//! next one.
//!
//! The planning ladders all start at their frame's home, `(0, 0,
//! clearance)` - on a table that is a point above the work, but in a
//! rotary frame it is the axis line itself (the contour), or azimuth
//! zero at station zero (the wrap): a place no clearance law names.
//! [`opening`] skips the ladder's opening traverse in that home plane,
//! so the program's first record is the clearance pose the traverse
//! was heading for - above the first cut, on its own radial - and the
//! exits state the pure radial there before anything moves.
//!
//! [`radial_relink`] rebuilds Rotary Parallel's links in its laid
//! frame (local x the axis, the local y-z plane the radial one): the
//! raster left each pass with a lift along the frame's z, which on a
//! turned bar is a sideways sweep of a radially posed tool. Each link
//! becomes a radial retract to the retract cylinder, an index on that
//! cylinder (chorded at `ARC_FIT_TOL`, short way round), and a radial
//! plunge into the next pass. A ball tangent to a solid of revolution
//! with no undercut can always back straight out along its radial:
//! the solid's ball-dilation is star-shaped about the axis in every
//! meridian, so the retract never re-enters the forbidden zone - the
//! property a Z lift does not have once the pass sits on a flank.
//!
//! Provenance: radial retract / angular index / radial infeed linking
//! of 4-axis rotary milling from general machining practice; the
//! star-shaped dilation argument is elementary geometry. Nothing of
//! any vendor's consulted.

use crate::cl::{Move, MoveKind, Toolpath};
use crate::geometry::{Point3, Triangle};
use crate::machining2d::{GenError, ARC_FIT_TOL};

/// The ladder's opening traverse in its home plane: the index of the
/// first move to replay and the plan point the program starts at. A
/// ladder that opens any other way starts where it says it does.
pub(super) fn opening(base: &Toolpath) -> (usize, Point3) {
    let Some(home) = base.start() else {
        return (0, Point3::ZERO);
    };
    let mut at = home;
    let mut k = 0usize;
    for m in &base.moves {
        if m.kind != MoveKind::Rapid || (m.to.z - home.z).abs() > 1e-9 * home.z.abs().max(1.0) {
            break;
        }
        at = m.to;
        k += 1;
    }
    (k, at)
}

/// The largest radius of `facets` about the LOCAL x axis (the laid
/// frame's axis line).
pub(super) fn local_rho_max(facets: &[Triangle]) -> f64 {
    facets
        .iter()
        .flat_map(|t| [t.a, t.b, t.c])
        .map(|q| (q.y * q.y + q.z * q.z).sqrt())
        .filter(|v| v.is_finite())
        .fold(0.0f64, f64::max)
}

/// The sloped-entry reading of [`radial_relink`] (#15): the ramp angle
/// in degrees, the laid-frame part facets the ramps are proved against,
/// and the plan tolerance. None keeps every infeed a radial plunge.
#[derive(Clone, Copy)]
pub(super) struct Ramp<'a> {
    pub deg: f64,
    pub facets: &'a [Triangle],
    pub tol: f64,
}

/// Rotary Parallel's links, rebuilt radial. `base` is the laid-frame
/// plan in its vertical-ball tip convention (ball center `r` up local
/// z); `clear`/`hop` are TIP radii about the local x axis - the
/// clearance and retract planes read as heights above the axis, the
/// laid frame's own reading. The cut moves are kept verbatim. With
/// `ramp` (the centre-cutting guard's sloped retry), each infeed whose
/// ramp is proved clear descends from the retract cylinder in the
/// landing's meridian while advancing along the axis in the pass's own
/// direction ([`ramp_start`]), instead of plunging along the radius.
#[allow(clippy::too_many_arguments)] // one relink's flat read
pub(super) fn radial_relink(
    base: &Toolpath,
    r: f64,
    clear: f64,
    hop: f64,
    rho_part: f64,
    feed: f64,
    plunge_feed: f64,
    ramp: Option<Ramp<'_>>,
) -> Result<Toolpath, GenError> {
    for (name, v) in [("clearance", clear), ("retract", hop)] {
        if !(v.is_finite() && v > rho_part + ARC_FIT_TOL) {
            // #677: names no spot - a height (a radius about the bar), not a place.
            return Err(GenError::Invalid(format!(
                "Rotary Parallel reads its {name} height as a radius about the bar's axis: {v:.3} mm does not clear the work's {rho_part:.3} mm radius - raise it"
            )));
        }
    }
    let ms = &base.moves;
    let link = |i: usize| -> bool {
        let m = &ms[i];
        match m.kind {
            MoveKind::Rapid | MoveKind::Plunge => true,
            // The lift off a pass: straight up the frame's z, into air.
            MoveKind::Feed => {
                m.arc.is_none()
                    && m.from.x == m.to.x
                    && m.from.y == m.to.y
                    && m.to.z > m.from.z
                    && ms.get(i + 1).is_some_and(|n| n.kind == MoveKind::Rapid)
            }
        }
    };
    let mut runs: Vec<&[Move]> = Vec::new();
    let mut i = 0usize;
    while i < ms.len() {
        if link(i) {
            i += 1;
            continue;
        }
        let s = i;
        while i < ms.len() && !link(i) {
            i += 1;
        }
        runs.push(&ms[s..i]);
    }
    if ramp.is_some() {
        // #15: a run that opens with the raster's own steep drop onto its
        // line (a Feed the lift test keeps, within the plunge law's cone
        // of local -z: axial run under half its length) enters at the
        // drop's FOOT - the ramp lands there, and the drop, posed
        // radially a plunge down the tool's own axis, stands down.
        let drop = |m: &Move| {
            let d = m.to - m.from;
            m.arc.is_none() && d.z < 0.0 && d.x.abs() <= 0.5 * d.dot(d).sqrt()
        };
        for run in runs.iter_mut() {
            while run.len() > 1 && drop(&run[0]) {
                *run = &run[1..];
            }
        }
    }
    let (Some(first), Some(last)) = (runs.first(), runs.last()) else {
        return Err(GenError::Empty("the rotary raster left no pass to link".into()));
    };
    let (Some(b0), Some(end)) = (first.first().map(|m| m.from), last.last().map(|m| m.to)) else {
        return Err(GenError::Empty("the rotary raster left no pass to link".into()));
    };
    let mut out: Vec<Move> = Vec::with_capacity(ms.len());
    let (mut ramped, mut kept) = (0usize, 0usize);
    let mut infeed = |run: &[Move], b: Point3| -> Result<Option<Point3>, GenError> {
        let Some(rp) = ramp else { return Ok(None) };
        let s = ramp_start(run, b, r, hop, rp)?;
        if s.is_some() { ramped += 1 } else { kept += 1 }
        Ok(s)
    };
    let start = lift(b0, r, clear)?;
    match infeed(first, b0)? {
        Some(s) => {
            let from = lift(Point3::new(s.x, b0.y, b0.z), r, clear)?;
            out.push(Move::new(MoveKind::Rapid, from, s, None));
            out.push(Move::new(MoveKind::Feed, s, b0, Some(plunge_feed)));
        }
        None => out.push(Move::new(MoveKind::Plunge, start, b0, Some(plunge_feed))),
    }
    for (k, run) in runs.iter().enumerate() {
        if k > 0
            && let (Some(a), Some(b)) = (out.last().map(|m| m.to), run.first().map(|m| m.from))
        {
            // #1015: a link refused part-way hands back the passes
            // linked so far (planning frame; the caller carries it out).
            let s = infeed(run, b).map_err(|e| e.with_ghost_moves(&out))?;
            link_into(&mut out, a, b, r, hop, feed, plunge_feed, s).map_err(|e| e.with_ghost_moves(&out))?;
        }
        out.extend(run.iter().cloned());
    }
    let exit = lift(end, r, clear).map_err(|e| e.with_ghost_moves(&out))?;
    out.push(Move::new(MoveKind::Rapid, end, exit, None));
    let mut path = Toolpath::new(out);
    path.notes = base.notes.clone();
    if ramp.is_some() {
        path.notes.push(format!(
            "Rotary Parallel: {ramped} infeed(s) ramp in along the pass from the retract cylinder; {kept} with no proved room plunge radially"
        ));
    }
    Ok(path)
}

/// The ramp's top for an infeed onto tip `b` at the head of `run`: the
/// point on the `hop` cylinder in `b`'s meridian, backed off along the
/// axis against the pass's first axial travel by the ramp's run, so
/// the straight ramp to `b` descends at `rp.deg` while advancing with
/// the pass. Some only when the ball centre's whole ramp is proved
/// clear of the part ([`super::entry_ramp::segment_clear`]); None keeps
/// the radial plunge (no axial travel, nothing to drop, or no proof).
fn ramp_start(run: &[Move], b: Point3, r: f64, hop: f64, rp: Ramp<'_>) -> Result<Option<Point3>, GenError> {
    use super::entry_ramp::{boxes, ramp_run, segment_clear};
    let Some(dx) = run.iter().map(|m| m.to.x - m.from.x).find(|d| d.abs() > 1e-9) else {
        return Ok(None);
    };
    let ub = lift(b, r, hop)?;
    let rho_b = b.y.hypot(b.z + r);
    let Some(l) = ramp_run(hop + r - rho_b, rp.deg) else { return Ok(None) };
    let s = Point3::new(ub.x - dx.signum() * l, ub.y, ub.z);
    let up = |q: Point3| Point3::new(q.x, q.y, q.z + r);
    let bx = boxes(rp.facets);
    Ok(segment_clear(up(s), up(b), r, rp.tol, rp.facets, &bx).then_some(s))
}

/// Retract from tip `a` along its radial to the `hop` cylinder, index
/// on it, then plunge along tip `b`'s radial into `b` - or, with a
/// proved ramp top `ramp` on the cylinder, index to it and ramp down
/// into `b`.
#[allow(clippy::too_many_arguments)] // one link's flat read
fn link_into(
    out: &mut Vec<Move>,
    a: Point3,
    b: Point3,
    r: f64,
    hop: f64,
    feed: f64,
    plunge_feed: f64,
    ramp: Option<Point3>,
) -> Result<(), GenError> {
    let ua = lift(a, r, hop)?;
    let ub = ramp.map_or_else(|| lift(b, r, hop), Ok)?;
    out.push(Move::new(MoveKind::Feed, a, ua, Some(feed)));
    let rc = hop + r;
    let (pa, pb) = (polar(ua, r), polar(ub, r));
    let dphi = (pb.1 - pa.1 + std::f64::consts::PI).rem_euclid(std::f64::consts::TAU) - std::f64::consts::PI;
    let cap = 2.0 * (1.0 - (ARC_FIT_TOL / rc).min(2.0)).clamp(-1.0, 1.0).acos();
    let n = if cap > 1e-12 { (dphi.abs() / cap).ceil() as usize } else { 1 };
    let n = n.clamp(1, 8192);
    let mut at = ua;
    for j in 1..=n {
        let t = j as f64 / n as f64;
        let to = if j == n {
            ub
        } else {
            let (x, ph) = (pa.0 + (pb.0 - pa.0) * t, pa.1 + dphi * t);
            Point3::new(x, rc * ph.cos(), rc * ph.sin() - r)
        };
        if to != at {
            out.push(Move::new(MoveKind::Rapid, at, to, None));
            at = to;
        }
    }
    let kind = if ramp.is_some() { MoveKind::Feed } else { MoveKind::Plunge };
    out.push(Move::new(kind, ub, b, Some(plunge_feed)));
    Ok(())
}

/// The tip whose ball center stands on tip `p`'s center radial, `rho`
/// (tip radius) out from the local x axis.
fn lift(p: Point3, r: f64, rho: f64) -> Result<Point3, GenError> {
    let (cy, cz) = (p.y, p.z + r);
    let l = (cy * cy + cz * cz).sqrt();
    if !(l.is_finite() && l > 1e-9) {
        return Err(GenError::Invalid(format!(
            "a rotary pass ends with its ball center on the axis line at {p:?} - no radius to retract along"
        )));
    }
    let k = (rho + r) / l;
    Ok(Point3::new(p.x, cy * k, cz * k - r))
}

/// (station, azimuth) of tip `p`'s ball center.
fn polar(p: Point3, r: f64) -> (f64, f64) {
    (p.x, (p.z + r).atan2(p.y))
}
