//! A pass restart inside a 3-D ladder level (#3670): the op's own entry,
//! the ramp along the pass, or a named plunge, with a full-disc clearance
//! proof separate from the non-centre-cutting plunge law (`plunge_law`).
//!
//! Provenance: standard CNC entry motions (helix, ramp, plunge) per public
//! G-code manuals and machining handbooks; the cleared-column test is this
//! crate's own geometry. No vendor material.

use super::descent::{Ask, descend, fallback, mat_top, push_zig3, zig_fallback};
use super::level_ctx::LevelCtx;
use super::params::Entry3D;

use crate::cl::{Move, MoveKind, PathBuilder};
use crate::geo2d::Point2;

/// A straight cut by this same tool clears its full disc at every point
/// of its centre line. Require the restart axis on that line, rather
/// than merely near it (the core law permits an offset of half a radius).
/// This sufficient proof can miss clearance made by several cuts. That
/// costs an entry, never a plunge into stock. Arcs do not prove a line:
/// their tessellation chords can lie outside the actual cleared sweep.
fn disc_into_solid(moves: &[Move], xy: Point2, z: f64, top: f64) -> bool {
    if ![xy.x, xy.y, z, top].iter().all(|v| v.is_finite()) {
        return true;
    }
    if z >= top - 1e-6 {
        return false;
    }
    !moves.iter().rev().any(|m| {
        if m.kind == MoveKind::Rapid
            || m.arc.is_some()
            || !m.from.z.is_finite()
            || !m.to.z.is_finite()
            || m.from.z.max(m.to.z) > z + 1e-6
        {
            return false;
        }
        let (ax, ay) = (xy.x - m.from.x, xy.y - m.from.y);
        let (dx, dy) = (m.to.x - m.from.x, m.to.y - m.from.y);
        let len2 = dx * dx + dy * dy;
        let along = ax * dx + ay * dy;
        // No XY slack: an offset centre line does not contain this disc.
        // Explicit endpoint equality also covers a stationary plunge.
        (xy.x == m.from.x && xy.y == m.from.y)
            || (xy.x == m.to.x && xy.y == m.to.y)
            || (len2.is_finite() && len2 > 0.0 && along >= 0.0 && along <= len2 && ax * dy == ay * dx)
    })
}

/// A pass start inside a level, reached by a hop (#3670): a straight
/// plunge stands only over a cleared column, or when the user's entry is
/// itself the plunge on a centre-cutting tool. A tool that is not
/// centre-cutting takes [`fallback`] (M11.3): over an uncut core it can
/// refuse; over a cleared core (#3885) a clean helix or ramp is kept,
/// else the plunge that the core law permits, named. Any other tool takes
/// the op's own `entry` (helix, ramp, arc) where that descent posts no
/// plunge into solid itself, else the ramp along the pass, else the
/// plunge, named. `None`: plunge as before; `Some(dive start)`: posted.
pub(super) fn reenter(b: &mut PathBuilder, lvl: &LevelCtx, a: &Ask, entry: Entry3D) -> Option<Option<Point2>> {
    const WHY: &str = "a pass start inside the level";
    let (top, tip) = (mat_top(lvl, a), lvl.tip(a.entry, 0.0));
    if !disc_into_solid(b.moves(), a.entry, tip, top) {
        return None;
    }
    // Each candidate runs on a copy and is kept only if it is clean: no
    // plunge into solid (a helix that fits only far off the pass start
    // dives again there) and no crossing of a move already posted (the
    // verifier's Crossing law, which a lifted shell's legs can meet).
    let n0 = b.moves().len();
    let clean = |t: &PathBuilder| {
        let mv = t.moves();
        !crate::machining2d::moves_cross_prior(&mv[n0..], &mv[..n0])
            && !(n0..mv.len()).any(|i| {
                let m = &mv[i];
                m.kind == MoveKind::Plunge
                    && disc_into_solid(&mv[..i], Point2::new(m.to.x, m.to.y), m.to.z, top)
            })
    };
    if !lvl.p.links.approach.center_cutting {
        if crate::plunge_law::into_solid(b, a.entry, tip, top, lvl.p.tool_diameter * 0.5) {
            return Some(fallback(b, lvl, a, WHY, false));
        }
        let mut trial = b.clone();
        let dove = fallback(&mut trial, lvl, a, WHY, false);
        // #3885: the core law permits the plunge here, so the fallback's
        // refusal does not stand; only a posted, clean helix or ramp wins.
        let refused = trial.notes().iter().any(|n| n.starts_with(crate::approach::CENTER_CUT_REFUSAL));
        if !refused && clean(&trial) {
            *b = trial;
            return Some(dove);
        }
        b.note_once("entry: a pass start inside the level has no room for a helix or ramp - the tool is not centre-cutting, plunged over its cleared core");
        return None;
    }
    if entry == Entry3D::Plunge {
        return None;
    }
    let mut trial = b.clone();
    let dove = descend(&mut trial, lvl, &Ask { arm: entry, ..*a });
    if clean(&trial) {
        *b = trial;
        return Some(dove);
    }
    if let Ok((pre, n, zf)) = zig_fallback(lvl, a) {
        let mut trial = b.clone();
        push_zig3(&mut trial, lvl, a, &pre, n, zf);
        if clean(&trial) {
            *b = trial;
            b.note_once("entry: a pass start inside the level has no room for the entry - ramp along the pass instead");
            return Some(None);
        }
    }
    b.note_once("entry: a pass start inside the level has no room for the entry or a ramp - plunged there");
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::Point3;

    #[test]
    fn cleared_core_does_not_prove_the_outer_disc_clear() {
        let cut = Move::new(MoveKind::Feed, Point3::new(-5.0, 0.0, 0.0), Point3::new(5.0, 0.0, 0.0), Some(100.0));
        let restart = Point2::new(0.0, 1.0);
        assert!(!crate::plunge_law::into_solid_at(&[cut], restart, 0.0, 2.0, 3.0), "the core law still permits this offset");
        // (0, 3.5) lies 2.5 mm from the restart axis, but 3.5 mm
        // from the earlier cut: the outer disc contains uncut stock.
        assert!(disc_into_solid(&[cut], restart, 0.0, 2.0));
        assert!(!disc_into_solid(&[cut], Point2::new(0.0, 0.0), 0.0, 2.0));
    }

    #[test]
    fn an_arc_chord_does_not_prove_a_cleared_disc() {
        let arc = Move::new(MoveKind::Feed, Point3::new(-5.0, 0.0, 0.0), Point3::new(5.0, 0.0, 0.0), Some(100.0))
            .with_arc(Point2::new(0.0, 0.0), true);
        assert!(disc_into_solid(&[arc], Point2::new(0.0, 0.0), 0.0, 2.0));
    }

    /// #3885: a pass restart at the origin of level Z -1 (material top
    /// Z 0) for a radius-3 tool that is not centre-cutting. One earlier
    /// cut at the level ran at `y = prior_y`. `room` gives the helix and
    /// the ramp space; without it, neither fits.
    fn non_cc_restart(prior_y: f64, room: bool) -> (Option<Option<Point2>>, crate::cl::Toolpath, usize) {
        let mut p = crate::machining3d::Pocket3DParams {
            pass_start: Default::default(),
            tool_diameter: 6.0,
            stepover: 2.4,
            stock_to_leave: 0.0,
            clearance_z: 5.0,
            top_z: 0.0,
            bottom_z: -1.0,
            stepdown: 1.0,
            feed: 1000.0,
            plunge_feed: 300.0,
            climb: true,
            strategy: crate::machining2d::PocketStrategy::Zigzag,
            retract_z: None,
            entry: Entry3D::Helix,
            helix_radius: 1.5,
            optimal_load: crate::load::DEFAULT_OPTIMAL_LOAD,
            links: crate::links::Links::ROUGH3D,
        };
        p.links.approach.center_cutting = false;
        p.validate().expect("valid params");
        let s = if room { 20.0 } else { 0.2 };
        let sq = crate::geo2d::Loop::new(vec![
            Point2::new(-s, -s),
            Point2::new(s, -s),
            Point2::new(s, s),
            Point2::new(-s, s),
        ])
        .expect("loop");
        let cen = crate::geo2d::Region::new(vec![sq]).expect("region");
        let pass = if room { vec![Point2::new(0.0, 0.0), Point2::new(15.0, 0.0)] } else { vec![Point2::new(0.0, 0.0), Point2::new(0.1, 0.0)] };
        let lvl = LevelCtx {
            cen: &cen,
            passes: &[],
            z: -1.0,
            hop_z: 2.0,
            p: &p,
            field: None,
            tz: 2.0,
            kept: false,
            settle: std::cell::Cell::new(0.0),
            arcs: None,
        };
        let a = Ask {
            arm: Entry3D::Plunge,
            transition: false,
            rest: false,
            entry: Point2::new(0.0, 0.0),
            first_pass: &pass,
            prev_z: 0.0,
            prev_cut: Some(0.0),
            air: false,
            restart: true,
        };
        let mut b = PathBuilder::new(Point3::new(-10.0, prior_y, -1.0));
        b.push(MoveKind::Feed, Point3::new(10.0, prior_y, -1.0), Some(p.feed));
        b.push(MoveKind::Rapid, Point3::new(10.0, prior_y, 2.0), None);
        let n0 = b.moves().len();
        let re = reenter(&mut b, &lvl, &a, p.entry);
        (re, b.finish(), n0)
    }

    /// The core law of M11.3 (`plunge_law`) permits this plunge: the
    /// earlier cut runs 1 mm from the axis, inside the half-radius core
    /// reach. The outer disc is not proven clear. With no room for a
    /// helix or a ramp, the restart plunges by name, as before #3857. It
    /// is not refused.
    #[test]
    fn a_non_centre_cutting_restart_over_a_cleared_core_plunges_without_room() {
        let (re, path, n0) = non_cc_restart(1.0, false);
        assert_eq!(crate::approach::refusal(&path), None, "a legal plunge is not refused: {:?}", path.notes);
        assert_eq!(re, None, "the caller posts the plunge");
        assert_eq!(path.moves.len(), n0, "no helix or ramp is posted: {:?}", &path.moves[n0..]);
        assert!(path.notes.iter().any(|n| n.contains("plunged over its cleared core")), "the plunge is named: {:?}", path.notes);
    }

    /// With room, the same restart takes the helix or the ramp of the
    /// non-centre-cutting fallback: the outer disc is not proven clear.
    #[test]
    fn a_non_centre_cutting_restart_over_a_cleared_core_prefers_an_entry() {
        let (re, path, n0) = non_cc_restart(1.0, true);
        assert_eq!(crate::approach::refusal(&path), None, "{:?}", path.notes);
        assert!(re.is_some(), "an entry is posted, not a plunge");
        assert!(path.moves.len() > n0, "the entry moves are posted");
        assert!(!path.moves[n0..].iter().any(|m| m.kind == MoveKind::Plunge && m.to.z < 0.0), "no plunge into the material: {:?}", &path.moves[n0..]);
    }

    /// The core law is unchanged: over an uncleared core (the earlier cut
    /// runs 2 mm from the axis, outside the 1.5 mm core reach) and with no
    /// room, the operation is refused by name.
    #[test]
    fn a_non_centre_cutting_restart_over_solid_core_is_refused_without_room() {
        let (_, path, _) = non_cc_restart(2.0, false);
        let why = crate::approach::refusal(&path).expect("refused by the centre-cutting law");
        assert!(why.contains("a pass start inside the level"), "{why}");
    }
}
