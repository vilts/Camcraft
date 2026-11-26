//! The start and exit picks of the two sinker kinds (#3333, part of
//! #1448). A Sinker Burn reads a pick as an angle round its burn
//! position: the start pick is where the orbit starts, the exit pick is
//! where the orbit ends before it returns to the burn position. A Sinker
//! Side Burn reads a pick as one of the two ends of its side move: the
//! burn position, or the wall end (the burn position moved by the amount
//! toward the wall). A pick that the orbit or the move cannot hold is
//! refused by name; no pick is moved without a note on the path.
//!
//! Provenance: our own design over the orbit patterns of `sinker_orbit`
//! and the side move of `sinker_side`, whose module heads cite their
//! public sources (orbital and lateral die-sinking in public EDM
//! machining texts). No vendor material.
//!
//! What a pick never changes:
//! - Every orbit vertex stays on or inside the circle of the stated
//!   radius (`sinker_orbit::on_disc`), so the undersize law and the
//!   cavity limits that count the radius hold at every angle.
//! - A circle keeps its own vertices (the four axis directions among
//!   them) and makes one full turn from the start; the start and the exit
//!   are added vertices. The walls move out by exactly the radius.
//! - A vector orbit keeps its directions; a pick chooses the order only.
//! - A side burn visits the same two positions at the same tip, under the
//!   same allowance law; a pick chooses the order and the retract place.
//! - With no pick the moves and the notes are those of every earlier file.

use std::f64::consts::TAU;

use crate::cl::{MoveKind, PathBuilder, Toolpath};
use crate::geometry::Point3;
use crate::machining2d::GenError;
use crate::path_picks::{PathPicks, PickRole};
use crate::sinker::SinkerParams;
use crate::sinker_orbit::{chords_per_turn, on_disc, orbit_offsets, Orbit, OrbitKind};
use crate::sinker_side::{side_burn_worn, side_refusal, SideBurn};
use crate::sinker_wear::SinkerWear;

/// Slack under which a pick is "on" a position, mm.
const ON_POINT: f64 = 1e-9;
/// Slack under which an angle is "on" a vertex or a vector, rad.
const ON_ANGLE: f64 = 1e-6;

/// The angles at which an orbit starts and ends, counter-clockwise from
/// +X in `[0, TAU)`. `None` is the automatic choice: the start at +X,
/// and the end where the pattern closes.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct OrbitEnds {
    pub start: Option<f64>,
    pub exit: Option<f64>,
}

impl OrbitEnds {
    /// No pick of either role: the orbit of every earlier file.
    #[must_use]
    pub fn is_auto(&self) -> bool {
        self.start.is_none() && self.exit.is_none()
    }

    /// What the picked orbit says of itself on its path; `None` with no pick.
    #[must_use]
    pub fn note(&self) -> Option<String> {
        let mut parts = Vec::new();
        if let Some(a) = self.start {
            parts.push(format!("starts at the start point's angle ({:.3} degrees from +X)", a.to_degrees()));
        }
        if let Some(a) = self.exit {
            parts.push(format!("ends at the exit point's angle ({:.3} degrees from +X)", a.to_degrees()));
        }
        (!parts.is_empty()).then(|| format!("sinker orbit: {}; it returns to the burn position before the retract", parts.join(" and ")))
    }
}

/// The smallest angle between two directions, rad.
fn apart(a: f64, b: f64) -> f64 {
    let d = (a - b).rem_euclid(TAU);
    d.min(TAU - d)
}

/// The orbit's ends from the picks, by the angle of each pick round the
/// burn position `centre`.
///
/// # Errors
/// By name: a pick that is not a readable point, a pick with no orbit (a
/// straight plunge has no free start or exit), a pick on the burn
/// position (no angle), and on a vector orbit a pick on no vector or an
/// exit that the order from the start cannot end on.
pub fn orbit_ends(centre: (f64, f64), orbit: Option<&Orbit>, picks: &PathPicks) -> Result<OrbitEnds, String> {
    if picks.is_empty() {
        return Ok(OrbitEnds::default());
    }
    if let Some(why) = picks.refusal() {
        return Err(format!("Sinker Burn: {why}"));
    }
    let mut ends = OrbitEnds::default();
    for role in [PickRole::Start, PickRole::Exit] {
        let Some(p) = picks.first(role) else { continue };
        let what = role.word();
        if orbit.is_none() {
            return Err(format!(
                "Sinker Burn: a {what} point is not taken without an orbit - a straight plunge starts and ends at the burn position; state an orbit or clear the {what} point"
            ));
        }
        let (dx, dy) = (p.xy.x - centre.0, p.xy.y - centre.1);
        let d = dx.hypot(dy);
        if !d.is_finite() || d <= ON_POINT {
            return Err(format!(
                "Sinker Burn: the {what} point is on the burn position - it gives the orbit no angle; move it off the burn position or clear it"
            ));
        }
        let a = dy.atan2(dx).rem_euclid(TAU);
        let a = if a < TAU { a } else { 0.0 };
        match role {
            PickRole::Start => ends.start = Some(a),
            PickRole::Exit => ends.exit = Some(a),
        }
    }
    if let Some(o) = orbit.filter(|o| o.kind == OrbitKind::Vector) {
        o.check().map_err(|e| e.to_string())?;
        vector_order(o, &ends)?;
    }
    Ok(ends)
}

/// The order in which a vector orbit burns its directions (indices from
/// +X, counter-clockwise) under `ends`. The start is the first vector.
/// The exit is the last one: with a start it is the vector before the
/// start (the order runs counter-clockwise) or the one after it (the
/// order runs clockwise).
fn vector_order(o: &Orbit, ends: &OrbitEnds) -> Result<Vec<u32>, String> {
    let m = o.vectors.max(1);
    let step = TAU / f64::from(m);
    let index = |a: f64, what: &str| -> Result<u32, String> {
        let k = (a / step).round();
        if apart(a, k * step) > ON_ANGLE {
            return Err(format!(
                "Sinker Burn: the {what} point is on no vector of the orbit - it is at {:.3} degrees from +X and the {m} vector(s) leave the burn position every {:.3} degrees from +X; move the point onto a vector or clear it",
                a.to_degrees(),
                step.to_degrees()
            ));
        }
        Ok((k as u32) % m)
    };
    let s = ends.start.map(|a| index(a, "start")).transpose()?;
    let e = ends.exit.map(|a| index(a, "exit")).transpose()?;
    let ccw = |from: u32| (0..m).map(|j| (from + j) % m).collect::<Vec<_>>();
    Ok(match (s, e) {
        (None, None) => ccw(0),
        (Some(s), None) => ccw(s),
        (None, Some(e)) => ccw((e + 1) % m),
        (Some(s), Some(e)) if (s + m - 1) % m == e => ccw(s),
        (Some(s), Some(e)) if (s + 1) % m == e => (0..m).map(|j| (s + m - j) % m).collect(),
        (Some(s), Some(e)) => {
            let at = |k: u32| (f64::from(k) * step).to_degrees();
            return Err(format!(
                "Sinker Burn: the exit point is on the vector at {:.3} degrees, but the vectors run in order from the start at {:.3} degrees and end at {:.3} or {:.3} degrees; move the exit point there or clear it",
                at(e),
                at(s),
                at((s + m - 1) % m),
                at((s + 1) % m)
            ));
        }
    })
}

/// A circle of radius `r` from the angle `a0`: the start vertex, the
/// circle's own `n` vertices counter-clockwise for one full turn, the
/// start vertex again, and with an exit the own vertices on to the exit
/// angle and the exit vertex. A pick on an own vertex is that vertex.
fn ring(out: &mut Vec<(f64, f64)>, r: f64, n: u32, a0: f64, exit: Option<f64>) {
    let step = TAU / f64::from(n);
    let grid = |k: u64| {
        let k = (k % u64::from(n)) as u32;
        if k == 0 { (r, 0.0) } else { on_disc(r, TAU * f64::from(k) / f64::from(n), r) }
    };
    let snap = |a: f64| {
        let k = (a / step).round();
        (apart(a, k * step) <= ON_ANGLE).then_some(k as u64)
    };
    let vertex = |a: f64| snap(a).map_or_else(|| on_disc(r, a, r), grid);
    let (first, own) = match snap(a0) {
        Some(k) => (k + 1, n - 1),
        None => ((a0 / step).floor() as u64 + 1, n),
    };
    out.push(vertex(a0));
    out.extend((0..u64::from(own)).map(|j| grid(first + j)));
    out.push(vertex(a0));
    let Some(a1) = exit else { return };
    let delta = (a1 - a0).rem_euclid(TAU);
    if delta <= ON_ANGLE || TAU - delta <= ON_ANGLE {
        return;
    }
    out.extend((first..first + u64::from(n)).take_while(|k| *k as f64 * step - a0 < delta - ON_ANGLE).map(grid));
    out.push(vertex(a1));
}

/// [`orbit_offsets`] under the picked ends: the orbit's XY offsets from
/// the burn position, in order, ending back on it. With no pick these
/// are exactly [`orbit_offsets`]'s. Every offset lies within the radius.
///
/// # Errors
/// `Orbit::check`'s refusals, and on a vector orbit [`orbit_ends`]'s.
pub fn orbit_offsets_between(o: &Orbit, ends: &OrbitEnds) -> Result<Vec<(f64, f64)>, GenError> {
    if ends.is_auto() {
        return orbit_offsets(o);
    }
    o.check()?;
    let r = o.radius;
    let n = chords_per_turn(r);
    let a0 = ends.start.unwrap_or(0.0);
    let mut out = Vec::new();
    match o.kind {
        OrbitKind::Circular => ring(&mut out, r, n, a0, ends.exit),
        OrbitKind::Vector => {
            for k in vector_order(o, ends).map_err(GenError::Invalid)? {
                out.push(if k == 0 { (r, 0.0) } else { on_disc(r, TAU * f64::from(k) / f64::from(o.vectors), r) });
                out.push((0.0, 0.0));
            }
            return Ok(out);
        }
        OrbitKind::Planetary => {
            // The spiral turns from the start angle, so it reaches the
            // radius there; the closing circle starts at that vertex.
            let steps = n.saturating_mul(o.turns);
            for k in 1..steps {
                let f = f64::from(k) / f64::from(steps);
                out.push(on_disc(r * f, a0 + TAU * f64::from(k) / f64::from(n), r));
            }
            ring(&mut out, r, n, a0, ends.exit);
        }
    }
    out.push((0.0, 0.0));
    Ok(out)
}

/// One end of a side burn's lateral move.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SideEnd {
    /// The burn position: where the plunge of every earlier file is made.
    #[default]
    BurnPosition,
    /// The burn position moved by the amount toward the wall.
    Wall,
}

impl SideEnd {
    fn word(self) -> &'static str {
        match self {
            Self::BurnPosition => "the burn position",
            Self::Wall => "the wall end",
        }
    }
}

/// The ends at which a side burn's lateral move starts and ends. The
/// default is the side burn of every earlier file: the plunge at the
/// burn position, the move to the wall end, and the return.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SideEnds {
    pub start: SideEnd,
    pub exit: SideEnd,
}

/// The side move's ends from the picks: a pick on the wall side of the
/// middle of the move chooses the wall end, a pick on the other side
/// chooses the burn position `centre`.
///
/// # Errors
/// By name: a pick that is not a readable point, the side's own
/// structural refusal, and a pick at the middle of the move (no end).
pub fn side_ends(centre: (f64, f64), s: &SideBurn, picks: &PathPicks) -> Result<SideEnds, String> {
    if picks.is_empty() {
        return Ok(SideEnds::default());
    }
    if let Some(why) = picks.refusal() {
        return Err(format!("Sinker Side Burn: {why}"));
    }
    if let Some(why) = side_refusal(s) {
        return Err(why);
    }
    let (ux, uy) = s.direction.unit();
    let half = s.amount / 2.0;
    let end = |role: PickRole| -> Result<SideEnd, String> {
        let Some(p) = picks.first(role) else { return Ok(SideEnd::default()) };
        let t = (p.xy.x - centre.0) * ux + (p.xy.y - centre.1) * uy;
        if !t.is_finite() || (t - half).abs() <= ON_POINT {
            return Err(format!(
                "Sinker Side Burn: the {} point is at the middle of the side move - it chooses no end; move it toward the burn position or toward the {} wall, or clear it",
                role.word(),
                s.direction.word()
            ));
        }
        Ok(if t > half { SideEnd::Wall } else { SideEnd::BurnPosition })
    };
    Ok(SideEnds { start: end(PickRole::Start)?, exit: end(PickRole::Exit)? })
}

/// `sinker_side::side_burn_worn` under the picked ends: the plunge (its
/// layers and its wear compensation) at the start end, the lateral move
/// to the other end, the move back when the exit is the start end, and
/// the retract straight up at the exit end. With the default ends the
/// moves and the notes are exactly `side_burn_worn`'s.
///
/// # Errors
/// Every refusal of `side_burn_worn`.
pub fn side_burn_between(p: &SinkerParams, s: &SideBurn, w: &SinkerWear, ends: &SideEnds) -> Result<Toolpath, GenError> {
    // The plain side burn first: its doors refuse before a pick is read.
    let plain = side_burn_worn(p, s, w)?;
    if *ends == SideEnds::default() {
        return Ok(plain);
    }
    // Its feeds are the plunge layers, the move out and the move back.
    let feeds: Vec<_> = plain.moves.iter().filter(|m| m.kind == MoveKind::Feed).collect();
    let Some(layers) = feeds.len().checked_sub(2).filter(|n| *n >= 1) else {
        return Err(GenError::Invalid("sinker side burn: the side move has no plunge to start from".into()));
    };
    let Some(tip) = feeds.get(layers).map(|m| m.to.z) else {
        return Err(GenError::Invalid("sinker side burn: the side move has no tip height".into()));
    };
    let (ux, uy) = s.direction.unit();
    let at = |e: SideEnd| match e {
        SideEnd::BurnPosition => (p.x, p.y),
        SideEnd::Wall => (p.x + ux * s.amount, p.y + uy * s.amount),
    };
    let other = if ends.start == SideEnd::Wall { SideEnd::BurnPosition } else { SideEnd::Wall };
    let ((sx, sy), (ox, oy), (ex, ey)) = (at(ends.start), at(other), at(ends.exit));
    let mut b = PathBuilder::approached(Point3::new(sx, sy, p.approach_z));
    for m in feeds.iter().take(layers) {
        b.push(MoveKind::Feed, Point3::new(sx, sy, m.to.z), m.feed);
    }
    b.push(MoveKind::Feed, Point3::new(ox, oy, tip), Some(p.feed));
    if ends.exit != other {
        b.push(MoveKind::Feed, Point3::new(ex, ey, tip), Some(p.feed));
    }
    b.push(MoveKind::Rapid, Point3::new(ex, ey, p.approach_z), None);
    for n in &plain.notes {
        b.note(n.clone());
    }
    b.note(format!(
        "sinker side burn: the side move starts at {} and ends at {} (chosen by the start point and the exit point)",
        ends.start.word(),
        ends.exit.word()
    ));
    Ok(b.finish())
}

#[cfg(test)]
#[path = "sinker_picks_tests.rs"]
mod tests;
