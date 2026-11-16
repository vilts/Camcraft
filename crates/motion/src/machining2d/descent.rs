//! The planar ladder's descents, one plan per motion (moved out of
//! `levels.rs` by M11.2 C so a level change can take any of them, from
//! any plane; M11.3 B/D their shapes and feeds, `approach.rs`): plunge,
//! the ramp chord, the zig-zag ramp at a maximum angle along the level's
//! first cut, the helix at a radius and pitch, the vertical-plane arc.
//! Each is PLANNED first (so a keep-down link knows where the descent
//! starts) and POSTED second, from whatever plane the tool travels at.
//!
//! M11.3 (two user hand-tests): ONE entry per level, ending on the first
//! cut's start - a plunge dives at the pass start itself where that is
//! off the wall, a helix is placed so its orbit closes on the pass start,
//! so no second dive follows a good entry; where one cannot be avoided
//! it is named. And every descent starts just over the material (its top
//! plus the plunge height): above that the tool rapids, it never feeds
//! or spirals through air.
//!
//! Provenance: standard CNC entry motions - straight plunge, linear and
//! zig-zag ramp, helical interpolation, circular ramp - as public G-code
//! manuals and machining handbooks describe them; the certification laws
//! are this crate's (M8.1's superset law, `entry::dive_room`,
//! `entry_arc`). No vendor material consulted.

use crate::approach::{CENTER_CUT_REFUSAL, NOT_CENTER_CUTTING};
use crate::cl::{MoveKind, PathBuilder, lattice};
use crate::entry::{Entry3D, disc_clears, dive_room, helix_about};
use crate::entry_arc::{ArcLead, arc_lead_swept, pts_dir, push_arc_lead, seg_dir};
use crate::geo2d::{Point2, Region};
use crate::geometry::Point3;
use crate::links::{helix_turns, plan_zig_capped, push_zig};
use crate::ring_zig::push_zig_ring;

use super::arcs::{ARC_FIT_TOL, Seg};
use super::levels::{Cert, Motion};

use super::descent_dive::{along_pass, axial_why, links_to};
use super::descent_xy::{XyArc, push_xy, with_arc};
pub(super) use super::descent_dive::{dive_link, dive_point, feed_down, restart, substituted_dive};

/// One helix: the near-side start (the pass start where it fits there),
/// the orbit's center, the turns, the radius, the plane it starts on.
pub(super) struct Hel {
    pub start: Point2,
    pub center: Point2,
    pub turns: f64,
    pub r: f64,
    pub from: f64,
}

/// A planned descent onto the pass start `e` at plane `z`.
pub(super) enum Descent {
    /// Axial dive at `d` - the pass start itself when it stands off the
    /// wall, else the floor's room - then the dive link to `e`.
    Plunge(Point2),
    /// The ramp chord from `s` (the tool's own XY, or the point the leg
    /// limit puts on that chord) at plane `zf` down to `e`.
    Ramp(Point2, f64),
    /// The zig-zag ramp along the first cut (from `e`): `n` out-and-back
    /// legs over `pre`, descending from plane `zf`.
    Zig(Vec<Point2>, usize, f64),
    Helix(Hel),
    /// The vertical-plane arc onto `e`, from plane `zf`.
    Arc(ArcLead, f64),
    /// The asked motion was refused for the reason: the floor's axial
    /// dive stands in, said.
    Sub(&'static str),
    /// A fallback arm (M11.3): the inner descent, posted after the note
    /// that says why the asked one stood down.
    Named(Box<Descent>, String),
    /// No legal arm: the tool cannot plunge into solid and nothing else
    /// fits - the ladder refuses the operation by name.
    Refused(String),
    /// The pocket's horizontal arc entry (M11.3): the inner descent onto
    /// the arc's start, then the G17 arc onto the pass start.
    XyArc(Box<Descent>, Box<XyArc>),
}

/// The cuts a descent leaves at the level's plane (dive links, arcs):
/// what a later keep-down link there must not cross.
pub(super) type Cuts = Vec<Vec<Point2>>;

/// What a plan reads of the level: the certified area, the plan's
/// passes, the pass being entered and its fitted walk, and the top of
/// the material at this level (the plane above, or the top of the cut
/// on the first level).
pub(super) struct Lvl<'a> {
    pub cen: &'a Cert<'a>,
    pub passes: &'a [Vec<Point2>],
    pub pass: &'a [Point2],
    pub walk: Option<&'a [Seg]>,
    pub mat_top: f64,
}

/// The plane a descent starts from (M11.3): on the first level the top
/// of the cut plus the plunge height (the rest of the way down from the
/// traverse plane is a rapid, through air by the generator's contract -
/// nothing is engaged above the top of the cut); on later levels the
/// plane above, or for a ramp to depth the top of the cut again. Never
/// above the plane the tool descends from. Air by the superset law:
/// every plane above the destination holds its center area.
fn descent_from(m: &Motion, prev_z: f64, ramp: bool) -> f64 {
    let a = &m.links.approach;
    let first = prev_z >= m.clearance_z;
    if first {
        (m.top_z + a.plunge_height).min(prev_z)
    } else if ramp && a.ramp_to_depth {
        (m.top_z + a.plunge_height).min(m.clearance_z)
    } else {
        prev_z
    }
}

/// Plan `arm` onto `e = pass[0]` at `z` from `prev_z`, the tool standing
/// over `leg`, with M11.3's fallback order: a helix that fits nowhere
/// tries the angle-bounded ramp along the first cut before the plunge
/// stands in; a tool that is not centre-cutting never plunges into the
/// level's solid - it takes the helix, then the ramp, or the operation is
/// refused by name. `zig` selects the angle-bounded ramp (a level
/// transition the user set to Ramp) over the entry's single chord.
/// A proved pre-drilled hole (#1449) is taken ahead of every arm.
pub(super) fn plan(arm: Entry3D, zig: bool, l: &Lvl, leg: Point2, z: f64, prev_z: f64, m: &Motion) -> Descent {
    if let Some(d) = super::predrilled::entry(l, z, m) {
        return d;
    }
    let d = with_arc(arm, zig, l, leg, z, prev_z, m).unwrap_or_else(|| plan_core(arm, zig, l, leg, z, prev_z, m));
    super::predrilled::unserved(d, z, m)
}

/// [`plan`] onto the pass start itself, without the horizontal arc.
pub(super) fn plan_core(arm: Entry3D, zig: bool, l: &Lvl, leg: Point2, z: f64, prev_z: f64, m: &Motion) -> Descent {
    let d = plan_arm(arm, zig, l, leg, z, prev_z, m);
    if arm == Entry3D::Helix
        && let Descent::Sub(why) = d
        && let Some(r) = ramp_fallback(l, z, prev_z, m)
    {
        return Descent::Named(Box::new(r), format!("entry: {why} - the helix stands down to a ramp along the first cut"));
    }
    let Some(why) = axial_why(&d, l).filter(|_| !m.links.approach.center_cutting) else {
        return d;
    };
    if arm != Entry3D::Helix
        && let h @ Descent::Helix(..) = plan_arm(Entry3D::Helix, false, l, leg, z, prev_z, m)
    {
        return Descent::Named(Box::new(h), format!("{NOT_CENTER_CUTTING} ({why}) - helix entry instead"));
    }
    if let Some(r) = ramp_fallback(l, z, prev_z, m) {
        return Descent::Named(Box::new(r), format!("{NOT_CENTER_CUTTING} ({why}) - ramp along the first cut instead"));
    }
    Descent::Refused(format!("{CENTER_CUT_REFUSAL} - {why}, and no helix or ramp fits onto Z {z:.3}"))
}

/// The ramp fallback: the zig-zag along the first cut at the entry's
/// ramp angle (3 deg when the entry sets none) - every point
/// on the proved cut.
pub(super) fn ramp_fallback(l: &Lvl, z: f64, prev_z: f64, m: &Motion) -> Option<Descent> {
    let a = &m.links.approach;
    let angle = if a.ramp_zigs() { a.ramp_angle } else { crate::links::DEFAULT_RAMP_ANGLE };
    let zf = descent_from(m, prev_z, true);
    plan_zig_capped(l.pass, zf - z, angle, a.ramp_max_len).map(|(pre, n)| Descent::Zig(pre, n, zf))
}

/// The helix onto `e`: placed so its orbit closes ON `e` (M11.3 - no
/// link, no second dive) where some center a radius off `e` has the
/// orbit's room, else at the floor's widest room with a link to `e`.
/// Radii from the op's down to the user's minimum.
fn plan_helix(
    l: &Lvl,
    e: Point2,
    z: f64,
    from: f64,
    pitch_of: impl Fn(f64) -> f64,
    m: &Motion,
) -> Result<Hel, &'static str> {
    let room: &Region = l.cen.room();
    let mut why = "the widened disc fits nowhere at this level";
    for r in m.links.approach.helix_radii(m.helix_radius) {
        let Some(t) = helix_turns(from - z, r, pitch_of(r)) else {
            why = "the helix pitch asks too many turns";
            continue;
        };
        // On the pass start, else on the first pass a little along it
        // (linked back to the start at depth on the pass's own line).
        let starts = std::iter::once(e).chain(along_pass(l, m).into_iter().filter(|&q| links_to(l, q, e)));
        for q in starts {
            for k in 0..8 {
                let (sn, cs) = (f64::from(k) * std::f64::consts::FRAC_PI_4).sin_cos();
                let c = Point2::new(lattice(q.x + r * cs), lattice(q.y + r * sn));
                if disc_clears(room, c, r + 1e-3) {
                    return Ok(Hel { start: q, center: c, turns: t, r, from });
                }
            }
        }
        // The found point is the orbit's CENTER; the dive starts at its
        // near side, `rh` west - one radius of center-room certifies the
        // whole swept tube exactly (`entry::dive_room`).
        if let Some(c) = dive_room(room, r + 1e-3) {
            let s = Point2::new(lattice(c.x - r), lattice(c.y));
            let center = Point2::new(lattice(s.x + r), lattice(s.y));
            return Ok(Hel { start: s, center, turns: t, r, from });
        }
    }
    Err(why)
}

pub(super) fn plan_arm(arm: Entry3D, zig: bool, l: &Lvl, leg: Point2, z: f64, prev_z: f64, m: &Motion) -> Descent {
    let Some(&e) = l.pass.first() else {
        return Descent::Sub("the pass has no start");
    };
    let a = &m.links.approach;
    match arm {
        Entry3D::Ramp if zig => match plan_zig_capped(l.pass, prev_z - z, m.links.ramp_angle, a.ramp_max_len) {
            Some((pre, n)) => Descent::Zig(pre, n, prev_z),
            None => Descent::Sub("the first cut is too short to ramp at the angle"),
        },
        Entry3D::Ramp if a.ramp_zigs() => {
            // M11.3 B: the entry ramp at the user's angle, zig-zagging
            // along the first cut (every point on the cut the level law
            // proved), each leg within the leg limit.
            let zf = descent_from(m, prev_z, true);
            match plan_zig_capped(l.pass, zf - z, a.ramp_angle, a.ramp_max_len) {
                Some((pre, n)) => Descent::Zig(pre, n, zf),
                None => Descent::Sub("the first cut is too short for the entry ramp's angle and length"),
            }
        }
        // #1609: on a ring (the circular kernel) the chord from the
        // tool's XY is a steep dive or no move at all - the ring
        // returns to its own start, so a level change's chord has no
        // length. The ramp rides the ring at the ramp angle instead
        // (the entry's, 3 deg by default), as the helix's stand-
        // down does; a ring too short for it dives at the plunge feed,
        // named. #1665: the bore's chord runs from the last ring start
        // to the centre, one ring radius - the same steep dive.
        Entry3D::Ramp if m.ring_zig => ramp_fallback(l, z, prev_z, m)
            .unwrap_or(Descent::Sub("the ring is too short to ramp at the angle")),
        Entry3D::Ramp if l.cen.chord_ok(leg, e) => {
            // The leg limit takes the chord's last stretch: a piece of
            // the proved chord, re-proved after the lattice snap.
            let s = a.ramp_start(leg, e);
            let s = if l.cen.chord_ok(s, e) { s } else { leg };
            Descent::Ramp(s, descent_from(m, prev_z, true))
        }
        Entry3D::Ramp => Descent::Sub("the ramp's chord is blocked"),
        Entry3D::Helix => {
            let from = descent_from(m, prev_z, false);
            let pitch = |r: f64| if zig { m.links.helix_pitch } else { a.entry_pitch(r) };
            match plan_helix(l, e, z, from, pitch, m) {
                Ok(h) => Descent::Helix(h),
                Err(why) => Descent::Sub(why),
            }
        }
        Entry3D::Arc => {
            // The circular ramp: the ramp's chord law on its lead chord
            // is its whole proof (see `entry_arc`).
            let rest = l.pass.get(1..).unwrap_or(&[]);
            let walk = l.walk.and_then(|w| w.first());
            let dir = walk.map_or_else(|| pts_dir(e, rest), |s| seg_dir(e, s));
            let zf = descent_from(m, prev_z, false);
            let lead = dir.ok_or("the first cut has no direction to be tangent to").and_then(|d| {
                let r = a.arc_nominal(m.arc_radius);
                arc_lead_swept(e, d, zf, z, r, a.arc_sweep_rad(), ARC_FIT_TOL, |s, q| l.cen.chord_ok(s, q))
            });
            match lead {
                Ok(lead) => Descent::Arc(lead, zf),
                Err(why) => Descent::Sub(why),
            }
        }
        // M11.3: the plunge dives at the pass start itself where that
        // stands off the wall - one entry, no link - else at the floor's
        // room (M8.1: never dwelling on a wall).
        Entry3D::Plunge => match dive_point(l, e, m) {
            Some(d) => Descent::Plunge(d),
            None => Descent::Sub("the disc fits nowhere here"),
        },
    }
}

/// Where a planned descent starts in XY - the point a keep-down link
/// must reach at the level above.
pub(super) fn start(d: &Descent, l: &Lvl, leg: Point2, m: &Motion) -> Point2 {
    let e = l.pass.first().copied().unwrap_or(leg);
    match d {
        Descent::Plunge(p) => *p,
        Descent::Helix(h) => h.start,
        Descent::Ramp(s, _) => *s,
        Descent::Zig(..) => e,
        Descent::Arc(a, _) => a.start,
        Descent::Named(inner, _) => start(inner, l, leg, m),
        Descent::XyArc(inner, x) => start(inner, &x.lvl(l), leg, m),
        Descent::Sub(_) | Descent::Refused(_) => dive_point(l, e, m).unwrap_or(e),
    }
}

/// Post a planned descent. `tz` is the plane the tool traverses at to
/// reach the descent's start (clearance, the retract plane, or - after a
/// keep-down link - the level above itself); each planned arm carries the
/// plane it descends from. Returns the planar cuts the descent ends with
/// (a dive link `d -> e`, a horizontal arc) on the level's plane: what a
/// later keep-down link there must not cross (the verifier's Crossing
/// law).
#[allow(clippy::too_many_arguments)]
pub(super) fn push(b: &mut PathBuilder, d: &Descent, l: &Lvl, z: f64, tz: f64, m: &Motion) -> Cuts {
    let Some(&e) = l.pass.first() else {
        return Vec::new();
    };
    let link = |d: Option<Point2>| -> Cuts { d.map(|d| vec![d, e]).into_iter().collect() };
    let a = &m.links.approach;
    let fd = a.descent_feed(m.feed);
    match d {
        Descent::Plunge(dp) => {
            let cur = b.at();
            if cur.x != dp.x || cur.y != dp.y {
                b.push(MoveKind::Rapid, Point3::new(dp.x, dp.y, tz), None);
            }
            feed_down(b, *dp, z, l.mat_top, m, a.plunge(m.plunge_feed));
            link(dive_link(b, l, *dp, e, z, m))
        }
        Descent::Ramp(s, zf) => {
            let cur = b.at();
            if cur.x != s.x || cur.y != s.y {
                // The leg limit moved the start onto the chord: over to
                // it on the plane the tool stands at (at or above the
                // chord's own start, so air by the same superset law).
                b.push(MoveKind::Rapid, Point3::new(s.x, s.y, cur.z), None);
            }
            b.push(MoveKind::Rapid, Point3::new(s.x, s.y, *zf), None);
            b.push(MoveKind::Feed, Point3::new(e.x, e.y, z), Some(fd));
            Vec::new()
        }
        Descent::Zig(pre, n, zf) => {
            let cur = b.at();
            if cur.x != e.x || cur.y != e.y {
                b.push(MoveKind::Rapid, Point3::new(e.x, e.y, tz), None);
            }
            b.push(MoveKind::Rapid, Point3::new(e.x, e.y, *zf), None);
            // #1558: a ring's ramp rides the ring's own arcs.
            match l.walk.and_then(<[Seg]>::first) {
                Some(Seg::Arc { center, ccw, .. }) if m.ring_zig => push_zig_ring(b, pre, *center, *ccw, *n, *zf, z, fd),
                _ => push_zig(b, pre, *n, *zf, z, fd),
            }
            Vec::new()
        }
        Descent::Helix(h) => {
            let cur = b.at();
            if cur.x != h.start.x || cur.y != h.start.y {
                b.push(MoveKind::Rapid, Point3::new(h.start.x, h.start.y, tz), None);
            }
            if h.r < m.helix_radius {
                b.note_once(&format!(
                    "entry: the helix fits only under its radius - flown at {:.3} mm, within the minimum radius",
                    h.r
                ));
            }
            b.push(MoveKind::Rapid, Point3::new(h.start.x, h.start.y, h.from), None);
            helix_about(b, h.start, h.center, h.from, z, h.turns, fd, !a.helix_cw);
            link(dive_link(b, l, h.start, e, z, m))
        }
        Descent::Arc(lead, zf) => {
            push_arc_lead(b, lead, tz, *zf, fd, a.plunge(m.plunge_feed));
            Vec::new()
        }
        Descent::Sub(why) => link(substituted_dive(b, l, e, z, m, why, tz)),
        Descent::Named(inner, note) => {
            b.note_once(note);
            push(b, inner, l, z, tz, m)
        }
        Descent::Refused(why) => {
            // The ladder turns this note into the operation's refusal
            // (`approach::refusal`); nothing is posted for a descent no
            // tool law allows.
            b.note_once(why);
            Vec::new()
        }
        Descent::XyArc(inner, x) => push_xy(b, inner, x, l, z, tz, m),
    }
}

