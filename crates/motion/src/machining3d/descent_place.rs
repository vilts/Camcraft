//! Where the 3-D ladder's descent lands on a level (split out of
//! `descent.rs`, issue #1950): the dive's margin off the wall, the spots
//! along the first pass that link back to its start, the helix placed to
//! close on the pass start (else at the floor's widest room), and the
//! plain plunge's spot. `descent` posts the motion.
//!
//! Provenance: standard CNC entry motions (plunge, helical
//! interpolation) per public G-code manuals and machining handbooks; the
//! legality laws are M8.1's superset law and the shell's tip law. Ours.

use super::descent::{Ask, from};
use super::level_ctx::{LevelCtx, bore_meets_lifted_leg};

use crate::cl::lattice;
use crate::entry::{chord_hits_plan, disc_clears, dive_room};
use crate::geo2d::Point2;
use crate::links::helix_turns;
use crate::machining2d::chord_clears;

/// The margin a dive at the pass start keeps off the wall.
pub(super) fn wall_margin(lvl: &LevelCtx) -> f64 {
    (lvl.p.tool_diameter * 0.05).max(0.01)
}

/// Points along the first pass past its start (and a wall margin to
/// either side), every half millimetre up to four tool diameters, that
/// link back to the start at depth (inside the center, crossing no
/// planned pass) - where an entry lands when the start sits on the wall
/// (M11.3: one entry per level, no second dive).
pub(super) fn along_pass(lvl: &LevelCtx, a: &Ask) -> Vec<Point2> {
    const STEP: f64 = 0.5;
    let reach = (lvl.p.tool_diameter * 4.0).max(4.0);
    let off = wall_margin(lvl) * 1.5;
    let e = a.entry;
    let mut out = Vec::new();
    let mut run = 0.0;
    let mut prev = e;
    for &q in a.first_pass {
        let d = (q.x - prev.x).hypot(q.y - prev.y);
        if !(d.is_finite() && d > 0.0) {
            prev = q;
            continue;
        }
        let (nx, ny) = (-(q.y - prev.y) / d * off, (q.x - prev.x) / d * off);
        let mut t = STEP;
        while t <= d && run + t <= reach && out.len() < 1200 {
            let f = t / d;
            let (px, py) = (prev.x + (q.x - prev.x) * f, prev.y + (q.y - prev.y) * f);
            for (dx, dy) in [(0.0, 0.0), (nx, ny), (-nx, -ny)] {
                let c = Point2::new(lattice(px + dx), lattice(py + dy));
                if chord_clears(lvl.cen, c, e) && !chord_hits_plan(c, e, lvl.passes) {
                    out.push(c);
                }
            }
            t += STEP;
        }
        run += d;
        prev = q;
        if run > reach {
            break;
        }
    }
    out
}

/// A placed helix: its near-side start, center, turns, radius.
pub(super) struct Hel {
    pub start: Point2,
    pub center: Point2,
    pub turns: f64,
    pub r: f64,
}

/// The helix: closing ON the pass start where a center a radius off it
/// has the orbit's room (M11.3), else at the floor's widest room; radii
/// from the op's down to the user's minimum.
pub(super) fn helix(lvl: &LevelCtx, a: &Ask) -> Result<Hel, &'static str> {
    let p = lvl.p;
    let ap = &p.links.approach;
    let crosses = |c: Point2| match (lvl.field, a.prev_cut) {
        (Some(_), Some(pz)) => bore_meets_lifted_leg(lvl, pz, c),
        _ => false,
    };
    let mut why = "the widened disc fits nowhere at this level";
    for r in ap.helix_radii(p.helix_radius) {
        let pitch = if a.transition { p.links.helix_pitch } else { ap.entry_pitch(r) };
        let Some(t) = helix_turns(from(lvl, a, false) - lvl.z, r, pitch) else {
            why = "the helix pitch asks too many turns";
            continue;
        };
        for q in std::iter::once(a.entry).chain(along_pass(lvl, a)) {
            for k in 0..8 {
                let (sn, cs) = (f64::from(k) * std::f64::consts::FRAC_PI_4).sin_cos();
                let c = Point2::new(lattice(q.x + r * cs), lattice(q.y + r * sn));
                if disc_clears(lvl.cen, c, r + 1e-3) && !crosses(c) {
                    return Ok(Hel { start: q, center: c, turns: t, r });
                }
            }
        }
        if let Some(c) = dive_room(lvl.cen, r + 1e-3) {
            if crosses(c) {
                why = "the shell lifts legs the bore would cross";
                continue;
            }
            let s = Point2::new(lattice(c.x - r), lattice(c.y));
            return Ok(Hel { start: s, center: Point2::new(lattice(s.x + r), lattice(s.y)), turns: t, r });
        }
    }
    Err(why)
}

/// Where the plain plunge lands: the pass start off the wall, else the
/// floor's room.
pub(super) fn plunge_at(lvl: &LevelCtx, a: &Ask) -> Option<Point2> {
    let m = wall_margin(lvl);
    if disc_clears(lvl.cen, a.entry, m) {
        return Some(a.entry);
    }
    along_pass(lvl, a).into_iter().find(|&q| disc_clears(lvl.cen, q, m)).or_else(|| dive_room(lvl.cen, 0.0))
}
