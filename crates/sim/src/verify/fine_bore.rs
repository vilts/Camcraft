//! The fine-bore reading of law (d) and the solid law (#1445, #1479): the
//! three rapids a fine-bore cycle makes with its spindle stopped - the
//! shift at the bottom, the rapid out, the shift back at R - are read
//! with the stopped bar, not with the turning tool's swept cylinder.
//!
//! Provenance: plane geometry of a round bar in a round bore, our own. The
//! cycle is the published G76 fine boring canned cycle (Haas Mill
//! Operator's Manual, G76): the spindle stops oriented, so the insert
//! points one way and the shift moves it off the wall. No vendor code.
//!
//! The other laws read the tool as the cylinder its turning edge sweeps
//! (radius `tool_r`). The feed in cuts exactly that cylinder about the
//! hole axis, from R to the bottom. Stopped and shifted by `s`, the bar
//! (radius `bar_r`) reaches `bar_r + s` from the hole axis, and the insert
//! tip, which points against the shift, reaches `tool_r - s`. Both are
//! inside the cut cylinder while `0 < s < tool_r - bar_r`, so the stopped
//! tool meets nothing that the feed in did not already remove. The swept
//! cylinder of a turning tool at the shifted XY would overlap the wall by
//! `s`: that is the conviction this reading takes back, and no other.
//!
//! What it trusts: that the tip points against the shift. That is the
//! user's statement, made true by the oriented stop; no geometry in the
//! program can check it. Fails closed: a group that is not exactly the
//! cycle's chain, a posed or rotary move, a number that is not finite, or
//! a shift with no room keeps every conviction.

use super::types::Warning;

use crate::cl::{Cycle, CycleKind, Move, MoveKind, Toolpath};

/// Two lattice numbers are the same (the chain is on a micrometre lattice).
const SAME: f64 = 1e-6;

/// `warns` without the `RapidInStock` convictions of the stopped-spindle
/// rapids of each fine-bore cycle whose stopped bar stays inside the
/// cylinder that its own feed in cut. `tool_r` is the cut radius, `bar_r`
/// the bar's (the shank's). Every other warning, and every other move,
/// keeps its verdict.
pub(super) fn acquit(path: &Toolpath, tool_r: f64, bar_r: f64, warns: &mut Vec<Warning>) {
    let cleared = stopped_rapids(path, tool_r, bar_r);
    if !cleared.is_empty() {
        warns.retain(|w| !matches!(*w, Warning::RapidInStock { at } if cleared.contains(&at)));
    }
}

/// The move indexes of the stopped-spindle rapids (shift, rapid out, shift
/// back) of each fine-bore cycle whose stopped bar stays inside the
/// cylinder that its own feed in cut. On these moves the cutting-edge zone
/// of the tool needs no reading of its own: the feed in was read over the
/// same cylinder. The shank and the holder are NOT covered by that
/// argument - they stand above the edge and move with the shift - so the
/// solid law still reads them at the shifted position
/// ([`super::solid`]). Empty for a path with no such cycle.
pub(super) fn stopped_rapids(path: &Toolpath, tool_r: f64, bar_r: f64) -> Vec<usize> {
    let mut v: Vec<usize> = Vec::new();
    for c in path.cycles.iter().filter(|c| c.kind == CycleKind::FineBore) {
        if stays_in_its_kerf(path, c, tool_r, bar_r) {
            v.extend((1..=3).filter_map(|k| c.first_move.checked_add(k)));
        }
    }
    v
}

/// True when the cycle's group is exactly feed in, shift, rapid out, shift
/// back, upright, and the shift leaves the stopped bar inside the feed
/// in's own cylinder.
fn stays_in_its_kerf(path: &Toolpath, c: &Cycle, tool_r: f64, bar_r: f64) -> bool {
    let Some(end) = c.first_move.checked_add(4) else { return false };
    let Some([cut, shift, out, back]) = path.moves.get(c.first_move..end) else { return false };
    let upright = |m: &Move| m.axis.is_none() && m.rotary.is_none() && m.arc.is_none();
    if ![cut, shift, out, back].into_iter().all(upright) {
        return false;
    }
    let same = |a: f64, b: f64| (a - b).abs() < SAME;
    let on_axis = |x: f64, y: f64| same(x, c.xy.x) && same(y, c.xy.y);
    // The feed in: straight down the hole axis, from R (or above) to the bottom.
    let fed = cut.kind != MoveKind::Rapid
        && on_axis(cut.from.x, cut.from.y)
        && on_axis(cut.to.x, cut.to.y)
        && cut.from.z >= c.r_z - SAME
        && same(cut.to.z, c.bottom_z);
    // The shift at the bottom, the rapid out at the shifted XY up to R (never
    // above the top of the feed in), and the shift back onto the axis at R.
    let (sx, sy) = (shift.to.x, shift.to.y);
    let shifted = shift.kind == MoveKind::Rapid && same(shift.to.z, c.bottom_z);
    let lifted = out.kind == MoveKind::Rapid && same(out.to.x, sx) && same(out.to.y, sy) && same(out.to.z, c.r_z);
    let returned = back.kind == MoveKind::Rapid && on_axis(back.to.x, back.to.y) && same(back.to.z, c.r_z);
    if !(fed && shifted && lifted && returned) {
        return false;
    }
    let s = (sx - c.xy.x).hypot(sy - c.xy.y);
    let room = tool_r - bar_r;
    s.is_finite() && bar_r.is_finite() && room.is_finite() && bar_r > 0.0 && s > 0.0 && s < room
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geo2d::Point2;
    use crate::geometry::Point3;

    const R: f64 = 12.0;
    const BOTTOM: f64 = -1.0;

    /// Rapid to R, then the fine-bore group with a shift of (`dx`, `dy`), then the hop.
    fn fine_bore(dx: f64, dy: f64) -> Toolpath {
        let pts = [
            (MoveKind::Rapid, 0.0, 0.0, R),
            (MoveKind::Plunge, 0.0, 0.0, BOTTOM),
            (MoveKind::Rapid, dx, dy, BOTTOM),
            (MoveKind::Rapid, dx, dy, R),
            (MoveKind::Rapid, 0.0, 0.0, R),
            (MoveKind::Rapid, 0.0, 0.0, 25.0),
        ];
        let mut at = Point3::new(0.0, 0.0, 25.0);
        let mut moves = Vec::new();
        for (kind, x, y, z) in pts {
            let to = Point3::new(x, y, z);
            moves.push(Move {
                rotary: None,
                kind,
                from: at,
                to,
                feed: if kind == MoveKind::Rapid { None } else { Some(60.0) },
                arc: None,
                axis: None,
            });
            at = to;
        }
        Toolpath {
            moves,
            cycles: vec![Cycle {
                kind: CycleKind::FineBore,
                xy: Point2::new(0.0, 0.0),
                r_z: R,
                bottom_z: BOTTOM,
                entry_z: 10.0,
                peck: 0.0,
                dwell: 0.0,
                first_move: 1,
            }],
            notes: Vec::new(),
            comps: Vec::new(),
            threads: Vec::new(),
            probes: Vec::new(),
            breaks: Vec::new(),
            plasma: None,
        }
    }

    fn convicted() -> Vec<Warning> {
        (0..6).map(|at| Warning::RapidInStock { at }).collect()
    }

    fn left(path: &Toolpath, tool_r: f64, bar_r: f64) -> Vec<usize> {
        let mut w = convicted();
        w.push(Warning::Gouge { at: 2 });
        acquit(path, tool_r, bar_r, &mut w);
        assert!(w.contains(&Warning::Gouge { at: 2 }), "only RapidInStock is ever re-read");
        w.iter().filter_map(|w| if let Warning::RapidInStock { at } = *w { Some(at) } else { None }).collect()
    }

    #[test]
    fn the_stopped_rapids_of_a_fitting_shift_are_acquitted_and_no_other_move() {
        // Dia. 20 cut, dia. 16 bar: 2 of room, a 0.5 shift.
        assert_eq!(left(&fine_bore(0.3, -0.4), 10.0, 8.0), vec![0, 1, 5]);
    }

    /// The solid law's own verdicts on `path`: a dia. 20 edge 5 long, a
    /// dia. 16 bar 20 long, a dia. 44 holder above them.
    fn solid(path: &Toolpath, facets: &[crate::geometry::Triangle]) -> Vec<Warning> {
        use super::super::types::ToolGeom;
        let geom = ToolGeom {
            radius: 10.0,
            nose_radius: 0.0,
            flute_len: 5.0,
            shank_radius: 8.0,
            shank_len: 20.0,
            holder_radius: 22.0,
            holder_len: 30.0,
        };
        let law = super::super::solid::SolidLaw { step: 0.25, allow: 0.01, hole_walls: false };
        super::super::solid::solid_hits(path, facets, &super::super::probe::Cutter::flat(&geom), &law)
    }

    #[test]
    fn the_solid_law_skips_the_stopped_edge_and_still_reads_the_shifted_holder() {
        let cube = |a: (f64, f64, f64), b: (f64, f64, f64)| {
            crate::geometry::cube(Point3::new(a.0, a.1, a.2), Point3::new(b.0, b.1, b.2))
        };
        // A wall 0.2 beside the cutting edge, over the height of the bore.
        let wall = cube((10.2, -5.0, BOTTOM), (15.0, 5.0, 3.0));
        // The turning tool's cylinder, shifted 0.5 toward it, would enter it.
        let mut bore = fine_bore(0.5, 0.0);
        bore.cycles[0].kind = CycleKind::Bore;
        let w = solid(&bore, &wall.facets);
        assert!(w.contains(&Warning::RapidInPart { at: 2 }), "a turning tool shifted into the wall: {w:?}");
        // The stopped bar of a fine bore stays in its own kerf.
        assert_eq!(solid(&fine_bore(0.5, 0.0), &wall.facets), Vec::new());

        // A block 0.3 beside the holder, where the holder stands at the bottom.
        let block = cube((22.3, -5.0, 30.0), (30.0, 5.0, 40.0));
        assert_eq!(solid(&fine_bore(-0.5, 0.0), &block.facets), Vec::new(), "shifted away: clear");
        let w = solid(&fine_bore(0.5, 0.0), &block.facets);
        assert!(
            w.contains(&Warning::RapidInPart { at: 2 }) && w.contains(&Warning::RapidInPart { at: 3 }),
            "the holder moves with the shift and is still read: {w:?}"
        );
        assert!(!w.contains(&Warning::Gouge { at: 1 }), "the feed in itself is clear: {w:?}");
    }

    #[test]
    fn the_full_verifier_keeps_the_shifted_holder_collisions() {
        use super::super::types::{ToolGeom, VerifyContext};

        let block = crate::geometry::cube(
            Point3::new(22.3, -5.0, 30.0),
            Point3::new(30.0, 5.0, 40.0),
        );
        let tool = ToolGeom {
            radius: 10.0,
            nose_radius: 0.0,
            flute_len: 5.0,
            shank_radius: 8.0,
            shank_len: 20.0,
            holder_radius: 22.0,
            holder_len: 30.0,
        };
        let cx = VerifyContext {
            model: Some(&block.facets),
            tool: &tool,
            gouge_tol: 0.01,
            z_tol: 0.01,
            ..Default::default()
        };
        let toward = super::super::verify(&fine_bore(0.5, 0.0), &cx);
        for at in 2..=4 {
            assert!(toward.contains(&Warning::RapidInPart { at }), "holder collision at {at}: {toward:?}");
        }
        let away = super::super::verify(&fine_bore(-0.5, 0.0), &cx);
        for at in 2..=4 {
            assert!(!away.contains(&Warning::RapidInPart { at }), "shifted away: {away:?}");
        }
    }

    #[test]
    fn a_shift_with_no_room_for_the_bar_keeps_every_conviction() {
        let all: Vec<usize> = (0..6).collect();
        assert_eq!(left(&fine_bore(2.0, 0.0), 10.0, 8.0), all, "the bar touches the far wall");
        assert_eq!(left(&fine_bore(0.3, 0.0), 10.0, 10.0), all, "a bar as wide as the cut");
        assert_eq!(left(&fine_bore(0.3, 0.0), 10.0, 0.0), all, "no bar radius stated");
        assert_eq!(left(&fine_bore(0.3, 0.0), 10.0, f64::NAN), all, "a number that is not one");
        assert_eq!(left(&fine_bore(0.0, 0.0), 10.0, 8.0), all, "no shift at all");
    }

    #[test]
    fn a_group_that_is_not_the_cycles_chain_keeps_every_conviction() {
        let all: Vec<usize> = (0..6).collect();
        // The same moves under a plain bore record.
        let mut bore = fine_bore(0.3, 0.0);
        bore.cycles[0].kind = CycleKind::Bore;
        assert_eq!(left(&bore, 10.0, 8.0), all);
        // The rapid out leaves the shifted XY.
        let mut wander = fine_bore(0.3, 0.0);
        wander.moves[3].to.x = 5.0;
        assert_eq!(left(&wander, 10.0, 8.0), all);
        // The shift is made above the bottom.
        let mut high = fine_bore(0.3, 0.0);
        high.moves[2].to.z = 3.0;
        assert_eq!(left(&high, 10.0, 8.0), all);
        // The rapid out stops below R.
        let mut short = fine_bore(0.3, 0.0);
        short.moves[3].to.z = 5.0;
        assert_eq!(left(&short, 10.0, 8.0), all);
        // A posed move.
        let mut posed = fine_bore(0.3, 0.0);
        posed.moves[2].axis = Some([0.0, 0.6, 0.8]);
        assert_eq!(left(&posed, 10.0, 8.0), all);
        // A record that points past the end of the chain.
        let mut past = fine_bore(0.3, 0.0);
        past.cycles[0].first_move = usize::MAX;
        assert_eq!(left(&past, 10.0, 8.0), all);
    }
}
