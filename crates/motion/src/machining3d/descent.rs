//! The 3-D ladder's descents onto a level (moved out of `ladder.rs` by
//! M11.2 C; M11.3 gives them the approach parameters and the one-entry
//! law): ramp chord or zig-zag ramp, the vertical-plane arc, the helix
//! at its radius and pitch - placed to close ON the pass start where it
//! fits there - the axial plunge at the pass start off the wall or in
//! the floor's room, each starting just over the material and traversing
//! at the plane the user's level link chose. [`start`] answers where a
//! descent begins, so a keep-down link knows what to reach. Where an
//! entry lands (helix spot, plunge spot) is `descent_place`.
//!
//! Provenance: standard CNC entry motions (plunge, linear and zig-zag
//! ramp, helical interpolation, circular ramp) per public G-code manuals
//! and machining handbooks; the legality laws are M8.1's superset law and
//! the shell's tip law (`machining3d::shell`). No vendor material.

use super::descent_place::{Hel, along_pass, helix, plunge_at};
use super::level_ctx::{LevelCtx, bore_meets_lifted_leg, dive_link, feed_leg, substituted_dive, traverse};
use super::params::Entry3D;

use crate::approach::{CENTER_CUT_REFUSAL, NOT_CENTER_CUTTING};
use crate::cl::{MoveKind, PathBuilder, lattice};
use crate::entry::{disc_clears, dive_room, helix_about};
use crate::entry_arc::{arc_lead_swept, pts_dir, push_arc_lead};
use crate::geo2d::Point2;
use crate::geometry::Point3;
use crate::links::{helix_turns, plan_zig_capped, push_zig};
use crate::machining2d::{chord_clears, ARC_FIT_TOL};
use crate::plunge_law;

/// One level's descent: the motion and how it reads the level.
pub(super) struct Ask<'a> {
    pub arm: Entry3D,
    /// The angle-bounded zig-zag (a Ramp transition) and the pitched
    /// helix - a user's level transition, never the first entry.
    pub transition: bool,
    /// A rest pass: every arm stands down to the wall dive.
    pub rest: bool,
    pub entry: Point2,
    pub first_pass: &'a [Point2],
    pub prev_z: f64,
    /// The previous cut level's plane, `None` before any cut.
    pub prev_cut: Option<f64>,
    /// Issue #1950: `entry` is proved in the air outside the stock - a
    /// plain plunge there, for every arm (it bores into nothing).
    pub air: bool,
    /// Whether this ask is a pass restart inside a level (`reenter`) rather
    /// than an inter-level transition.
    pub restart: bool,
}

/// The plane a descent starts from (M11.3, #3671): on the first level the top
/// of the cut plus the plunge height (air above by the generator's
/// contract), on later levels the plane above plus the plunge height,
/// capped under the traverse plane - or, for a plain-ladder ramp to depth,
/// the top of the cut again plus the plunge height. Pass restarts inside
/// a level start at the cut plane above (`a.prev_z`).
pub(super) fn from(lvl: &LevelCtx, a: &Ask, ramp: bool) -> f64 {
    let p = lvl.p;
    let ap = &p.links.approach;
    if a.restart {
        if a.prev_cut.is_none() || (ramp && ap.ramp_to_depth && lvl.field.is_none()) {
            (p.top_z + ap.plunge_height).min(a.prev_z)
        } else {
            a.prev_z
        }
    } else {
        let top = if ramp && ap.ramp_to_depth && lvl.field.is_none() {
            p.top_z
        } else {
            mat_top(lvl, a)
        };
        (top + ap.plunge_height).min(lvl.tz)
    }
}

/// The material's top under a plunge at this level: the plane above on
/// the plain ladder; on a lifted shell (whose previous level may have cut
/// higher than its plane) and on the first level, the top of the cut.
pub(super) fn mat_top(lvl: &LevelCtx, a: &Ask) -> f64 {
    match (lvl.field, a.prev_cut) {
        (None, Some(pz)) => pz,
        _ => lvl.p.top_z,
    }
}

/// The axial feed onto `(xy, z_tip)`: a rapid to the plunge height over
/// the material first (held under the retract plane), then the feed.
pub(super) fn feed_down(b: &mut PathBuilder, lvl: &LevelCtx, a: &Ask, xy: Point2, z_tip: f64, feed: f64) {
    let h = lvl.p.links.approach.plunge_height;
    let top = mat_top(lvl, a);
    let hz = (top + h).min(lvl.hop_z.max(top));
    if h > 0.0 && hz.is_finite() && hz < b.at().z && hz > z_tip {
        b.push(MoveKind::Rapid, Point3::new(xy.x, xy.y, hz), None);
    }
    let ap = &lvl.p.links.approach;
    plunge_law::guard(b, ap.center_cutting, lvl.p.tool_diameter * 0.5, xy, z_tip, top, "a plunge");
    b.push(MoveKind::Plunge, Point3::new(xy.x, xy.y, z_tip), Some(feed));
}

/// The ramp's zig-zag along the first pass, where the plain ladder can
/// hold it (a lifted shell has no proof for it): the transition's angle
/// for a transition, the entry's own otherwise.
fn zig(lvl: &LevelCtx, a: &Ask, angle: f64, zf: f64) -> Result<(Vec<Point2>, usize), &'static str> {
    if lvl.field.is_some() {
        return Err("the zig-zag ramp is not proved onto a lifted shell");
    }
    plan_zig_capped(a.first_pass, zf - lvl.z, angle, lvl.p.links.approach.ramp_max_len)
        .ok_or("the first cut is too short to ramp at the angle")
}

/// A planned zig-zag: the stretch, the out-and-back count, its top plane.
type Zig = (Vec<Point2>, usize, f64);

/// The zig of a Ramp arm, if it zig-zags: the transition's, or the
/// entry's at its own angle (M11.3 B).
fn ramp_zig(lvl: &LevelCtx, a: &Ask) -> Option<Result<Zig, &'static str>> {
    let ap = &lvl.p.links.approach;
    if a.transition {
        Some(zig(lvl, a, lvl.p.links.ramp_angle, a.prev_z).map(|(p, n)| (p, n, a.prev_z)))
    } else if ap.ramp_zigs() {
        let zf = from(lvl, a, true);
        Some(zig(lvl, a, ap.ramp_angle, zf).map(|(p, n)| (p, n, zf)))
    } else {
        None
    }
}

/// The fallback zig (M11.3): along the first pass at the entry's angle,
/// the transition's when the entry sets none.
pub(super) fn zig_fallback(lvl: &LevelCtx, a: &Ask) -> Result<Zig, &'static str> {
    let ap = &lvl.p.links.approach;
    let angle = if ap.ramp_zigs() { ap.ramp_angle } else { lvl.p.links.ramp_angle };
    let zf = from(lvl, a, true);
    zig(lvl, a, angle, zf).map(|(p, n)| (p, n, zf))
}

fn arc(lvl: &LevelCtx, a: &Ask) -> Result<crate::entry_arc::ArcLead, &'static str> {
    let ap = &lvl.p.links.approach;
    match lvl.field {
        Some(_) => Err("the arc entry is not proved onto a lifted shell"),
        None => pts_dir(a.entry, a.first_pass)
            .ok_or("the first cut has no direction to be tangent to")
            .and_then(|d| {
                let ok = |s, q| chord_clears(lvl.cen, s, q);
                let r = ap.arc_nominal(lvl.p.tool_diameter);
                arc_lead_swept(a.entry, d, from(lvl, a, false), lvl.z, r, ap.arc_sweep_rad(), ARC_FIT_TOL, ok)
            }),
    }
}

/// Where the descent `a` begins in XY with the tool standing over `leg`.
pub(super) fn start(lvl: &LevelCtx, a: &Ask, leg: Point2) -> Point2 {
    let sub = || plunge_at(lvl, a).unwrap_or(a.entry);
    if a.rest || a.air {
        return a.entry;
    }
    let cc = lvl.p.links.approach.center_cutting;
    match a.arm {
        Entry3D::Ramp => match ramp_zig(lvl, a) {
            Some(Ok(_)) => a.entry,
            Some(Err(_)) => sub(),
            None if chord_clears(lvl.cen, leg, a.entry) => lvl.p.links.approach.ramp_start(leg, a.entry),
            None => sub(),
        },
        Entry3D::Arc => arc(lvl, a).map_or_else(|_| sub(), |l| l.start),
        Entry3D::Helix => helix(lvl, a).map_or_else(|_| if zig_fallback(lvl, a).is_ok() { a.entry } else { sub() }, |h| h.start),
        Entry3D::Plunge if !cc => helix(lvl, a).map_or_else(|_| a.entry, |h| h.start),
        Entry3D::Plunge => sub(),
    }
}

fn probe_helix(b: &PathBuilder, lvl: &LevelCtx, a: &Ask, h: &Hel, z0: f64) -> Option<(f64, f64)> {
    let ap = &lvl.p.links.approach;
    let z_hel = lvl.tip(h.start, h.r * 2.0);
    let pitch = if a.transition { lvl.p.links.helix_pitch } else { ap.entry_pitch(h.r) };
    let turns = helix_turns(z0 - z_hel, h.r, pitch).unwrap_or(h.turns);
    let mut probe = PathBuilder::new(Point3::new(h.start.x, h.start.y, z0));
    helix_about(&mut probe, h.start, h.center, z0, z_hel, turns, ap.descent_feed(lvl.p.feed), !ap.helix_cw);
    (!crate::machining2d::moves_cross_prior(probe.moves(), b.moves())).then_some((z_hel, turns))
}

fn find_clear_helix(b: &PathBuilder, lvl: &LevelCtx, a: &Ask, z0: f64) -> Option<(Hel, f64, f64)> {
    let ap = &lvl.p.links.approach;
    let crosses_lifted = |c: Point2| matches!((lvl.field, a.prev_cut), (Some(_), Some(pz)) if bore_meets_lifted_leg(lvl, pz, c));
    for r in ap.helix_radii(lvl.p.helix_radius) {
        let pitch = if a.transition { lvl.p.links.helix_pitch } else { ap.entry_pitch(r) };
        let Some(t) = helix_turns(z0 - lvl.z, r, pitch) else { continue };
        for q in std::iter::once(a.entry).chain(along_pass(lvl, a)) {
            for k in 0..8 {
                let (sn, cs) = (f64::from(k) * std::f64::consts::FRAC_PI_4).sin_cos();
                let c = Point2::new(lattice(q.x + r * cs), lattice(q.y + r * sn));
                let cand = Hel { start: q, center: c, turns: t, r };
                if disc_clears(lvl.cen, c, r + 1e-3) && !crosses_lifted(c)
                    && let Some((z_hel, turns)) = probe_helix(b, lvl, a, &cand, z0)
                {
                    return Some((cand, z_hel, turns));
                }
            }
        }
        if let Some(c) = dive_room(lvl.cen, r + 1e-3).filter(|&c| !crosses_lifted(c)) {
            let s = Point2::new(lattice(c.x - r), lattice(c.y));
            let cand = Hel { start: s, center: Point2::new(lattice(s.x + r), lattice(s.y)), turns: t, r };
            if let Some((z_hel, turns)) = probe_helix(b, lvl, a, &cand, z0) {
                return Some((cand, z_hel, turns));
            }
        }
    }
    None
}

/// Post a helix and its link (if it did not close on the pass start).
fn push_helix(b: &mut PathBuilder, lvl: &LevelCtx, a: &Ask, h: &Hel) -> Option<Point2> {
    let p = lvl.p;
    let ap = &p.links.approach;
    let z0 = from(lvl, a, false);
    let (chosen, z_hel, turns) = match probe_helix(b, lvl, a, h, z0) {
        Some((zh, t)) => (Hel { start: h.start, center: h.center, turns: h.turns, r: h.r }, zh, t),
        None => match find_clear_helix(b, lvl, a, z0) {
            Some(found) => found,
            None => return fallback(b, lvl, a, "the shell lifts legs the bore would cross", true),
        },
    };
    traverse(b, lvl, chosen.start);
    if chosen.r < p.helix_radius {
        b.note_once(&format!(
            "entry: the helix fits only under its radius - flown at {:.3} mm, within the minimum radius",
            chosen.r
        ));
    }
    let rapid_z = if a.restart { (z0 + ap.plunge_height).min(lvl.tz) } else { z0 };
    b.push(MoveKind::Rapid, Point3::new(chosen.start.x, chosen.start.y, rapid_z), None);
    if rapid_z > z0 {
        b.push(MoveKind::Plunge, Point3::new(chosen.start.x, chosen.start.y, z0), Some(ap.descent_feed(p.feed)));
    }
    helix_about(b, chosen.start, chosen.center, z0, z_hel, turns, ap.descent_feed(p.feed), !ap.helix_cw);
    if chosen.start.x == a.entry.x && chosen.start.y == a.entry.y {
        return None;
    }
    dive_link(b, lvl, chosen.start, a.entry);
    Some(chosen.start)
}

pub(super) fn push_zig3(b: &mut PathBuilder, lvl: &LevelCtx, a: &Ask, pre: &[Point2], n: usize, zf: f64) {
    traverse(b, lvl, a.entry);
    let ap = &lvl.p.links.approach;
    let rapid_z = if a.restart { (zf + ap.plunge_height).min(lvl.tz) } else { zf };
    b.push(MoveKind::Rapid, Point3::new(a.entry.x, a.entry.y, rapid_z), None);
    if rapid_z > zf {
        b.push(MoveKind::Plunge, Point3::new(a.entry.x, a.entry.y, zf), Some(ap.descent_feed(lvl.p.feed)));
    }
    push_zig(b, pre, n, zf, lvl.z, ap.descent_feed(lvl.p.feed));
}

/// The stand-in when the asked arm cannot be honored (M11.3's order): a
/// failed helix rides the zig along the first pass; a tool that is not
/// centre-cutting takes the helix, then the zig, or the op is refused by
/// name; otherwise M8.1's substituted dive.
pub(super) fn fallback(b: &mut PathBuilder, lvl: &LevelCtx, a: &Ask, why: &str, tried_helix: bool) -> Option<Point2> {
    let cc = lvl.p.links.approach.center_cutting;
    if !cc
        && !tried_helix
        && let Ok(h) = helix(lvl, a)
    {
        b.note_once(&format!("{NOT_CENTER_CUTTING} ({why}) - helix entry instead"));
        return push_helix(b, lvl, a, &h);
    }
    if (tried_helix || !cc)
        && let Ok((pre, n, zf)) = zig_fallback(lvl, a)
    {
        let note = if cc {
            format!("entry: {why} - the helix stands down to a ramp along the first cut")
        } else {
            format!("{NOT_CENTER_CUTTING} ({why}) - ramp along the first cut instead")
        };
        b.note_once(&note);
        push_zig3(b, lvl, a, &pre, n, zf);
        return None;
    }
    if !cc {
        b.note_once(&format!("{CENTER_CUT_REFUSAL} - {why}, and no helix or ramp fits onto Z {:.3}", lvl.z));
        return None;
    }
    substituted_dive(b, lvl, a.entry, plunge_at(lvl, a), why)
}

/// Post the descent onto `a.entry` at the level, traversing at `lvl.tz`.
pub(super) fn descend(b: &mut PathBuilder, lvl: &LevelCtx, a: &Ask) -> Option<Point2> {
    let p = lvl.p;
    let ap = &p.links.approach;
    let (entry, z) = (a.entry, lvl.z);
    let at = b.at();
    let leg = Point2::new(at.x, at.y);
    if a.rest || a.air {
        // The rest ladder's entry (and #1950's air entry, its tool disc
        // clear of the stock): it starts inside planes the coarse
        // run already swept, so it rides the vertical dive at the wall
        // entry the nesting law specced.
        traverse(b, lvl, entry);
        feed_down(b, lvl, a, entry, lvl.tip(entry, 0.0), p.plunge_feed);
        return None;
    }
    match a.arm {
        Entry3D::Ramp => match ramp_zig(lvl, a) {
            Some(Ok((pre, n, zf))) => {
                push_zig3(b, lvl, a, &pre, n, zf);
                None
            }
            Some(Err(why)) => fallback(b, lvl, a, why, false),
            None => {
                // The diagonal's legality is asked of THIS level's center:
                // islands only grow downward, so the destination's room is
                // a subset of every corridor above it - one chord test
                // covers the whole interpolated move (M8.1); the leg limit
                // takes the chord's last stretch.
                let run = (leg.x - entry.x).hypot(leg.y - entry.y);
                if !ap.center_cutting && run <= 1e-6 {
                    fallback(b, lvl, a, "the ramp has no run - it is a plunge", false)
                } else if chord_clears(lvl.cen, leg, entry) {
                    let s = ap.ramp_start(leg, entry);
                    let s = if chord_clears(lvl.cen, s, entry) { s } else { leg };
                    if s.x != leg.x || s.y != leg.y {
                        b.push(MoveKind::Rapid, Point3::new(s.x, s.y, at.z), None);
                    }
                    let z_from = from(lvl, a, true);
                    let rapid_z = if a.restart { (z_from + ap.plunge_height).min(lvl.tz) } else { z_from };
                    b.push(MoveKind::Rapid, Point3::new(s.x, s.y, rapid_z), None);
                    if rapid_z > z_from {
                        b.push(MoveKind::Plunge, Point3::new(s.x, s.y, z_from), Some(ap.descent_feed(p.feed)));
                    }
                    let fd = ap.descent_feed(p.feed);
                    if lvl.field.is_none() {
                        b.push(MoveKind::Feed, Point3::new(entry.x, entry.y, z), Some(fd));
                    } else {
                        // The shell's ramp rides strictly above the plane
                        // until one chord short of its destination, and
                        // only then descends to the destination's tip.
                        let cap = p.tool_diameter * 0.5;
                        let (dx, dy) = (entry.x - s.x, entry.y - s.y);
                        let l = (dx * dx + dy * dy).sqrt();
                        let t = if l > cap { 1.0 - cap / l } else { 0.0 };
                        if t > 0.0 {
                            let q1 = Point2::new(s.x + dx * t, s.y + dy * t);
                            b.push(MoveKind::Feed, Point3::new(q1.x, q1.y, z), Some(fd));
                            // The tail rides the plane this level's passes
                            // are cut at, so it answers the dive link's law:
                            // fed only when it crosses none of them, flown
                            // over to the pass start when it would.
                            dive_link(b, lvl, q1, entry);
                        } else {
                            feed_leg(b, lvl, entry, 0.0);
                        }
                    }
                    None
                } else {
                    fallback(b, lvl, a, "the ramp's chord is blocked", false)
                }
            }
        },
        // The circular ramp (M11.1): the ramp's chord law on the lead
        // chord behind the entry, the fan posted by `entry_arc`.
        Entry3D::Arc => match arc(lvl, a) {
            // A sweep-capped arc under the material's top ends in an
            // axial drop: not for a tool that may not plunge.
            Ok(l) if !ap.center_cutting && l.top < from(lvl, a, false) - 1e-6 && l.top < mat_top(lvl, a) - 1e-6 => {
                fallback(b, lvl, a, "the arc's capped sweep ends in an axial drop", false)
            }
            Ok(l) => {
                if lvl.kept && (leg.x != l.start.x || leg.y != l.start.y) {
                    traverse(b, lvl, l.start);
                }
                push_arc_lead(b, &l, lvl.tz, from(lvl, a, false), ap.descent_feed(p.feed), ap.plunge(p.plunge_feed));
                None
            }
            Err(why) => fallback(b, lvl, a, why, false),
        },
        Entry3D::Helix => match helix(lvl, a) {
            Ok(h) => push_helix(b, lvl, a, &h),
            Err(why) => fallback(b, lvl, a, why, true),
        },
        Entry3D::Plunge if !ap.center_cutting => fallback(b, lvl, a, "the plunge arm", false),
        // The axial arm dives at the pass start off the wall, else where
        // the disc has room - never on a wall (M8.1).
        Entry3D::Plunge => match plunge_at(lvl, a) {
            Some(d) => {
                traverse(b, lvl, d);
                feed_down(b, lvl, a, d, lvl.tip(d, 0.0), ap.plunge(p.plunge_feed));
                if d.x == entry.x && d.y == entry.y {
                    return None;
                }
                dive_link(b, lvl, d, entry);
                Some(d)
            }
            None => substituted_dive(b, lvl, entry, None, "the disc fits nowhere here"),
        },
    }
}
