//! #1445: the reaming cycle (RS-274/NGC G85 - NIST IR 6556, LinuxCNC's
//! G-code reference): feed in, feed out at the same feed, no dwell, and
//! the bore door's refusals (a reamer opens a pre-drilled hole).

use super::tests::drill_tool;
use super::*;
use crate::cl::{CycleKind, MoveKind};
use crate::geo2d::Point2;
use crate::tools::ToolKind;

fn hole(r: f64, stepped: bool) -> Hole {
    Hole {
        xy: Point2::new(10.0, 10.0),
        radius: r,
        entry_z: 10.0,
        floor_z: 4.0,
        through: false,
        stepped,
    }
}

fn ream_params<'a>(tool: &'a crate::tools::Tool, holes: &'a [Hole]) -> CycleParams<'a> {
    CycleParams {
        clearance_z: 15.0,
        r_z: 10.0,
        hop_z: None,
        bottom_z: None,
        cycle: CycleChoice::Ream,
        peck: 0.0,
        dwell: 2.0,
        feed: 500.0,
        plunge_feed: 100.0,
        tool,
        holes,
    }
}

#[test]
fn ream_feeds_in_and_out_at_the_plunge_feed_without_dwell() {
    let tool = drill_tool(8.0);
    let hs = [hole(4.0, false)];
    let t = drill(&ream_params(&tool, &hs)).unwrap();
    assert_eq!(t.cycles.len(), 1);
    let c = &t.cycles[0];
    assert_eq!(c.kind, CycleKind::Ream);
    assert!((c.bottom_z - 4.0).abs() < 1e-9, "reams to the floor");
    assert_eq!(c.dwell, 0.0, "G85 carries no dwell word, so the chain has none");
    let g = &t.moves[c.first_move..];
    assert_eq!(g[0].kind, MoveKind::Plunge, "feed in");
    assert_eq!(g[0].feed, Some(100.0));
    assert!((g[0].to.z - 4.0).abs() < 1e-9);
    assert_eq!(g[1].kind, MoveKind::Feed, "feed out, never a rapid over the reamed wall");
    assert_eq!(g[1].feed, Some(100.0), "one F word drives both directions");
    assert!((g[1].to.z - 10.0).abs() < 1e-9, "feeds back to R");
    // Nothing between the bottom and R moves at rapid.
    assert!(
        !g[..2].iter().any(|m| m.kind == MoveKind::Rapid),
        "no rapid inside the reamed hole"
    );
}

#[test]
fn ream_is_never_the_auto_answer() {
    let tool = drill_tool(8.0);
    let hs = [hole(4.0, false)];
    let mut p = ream_params(&tool, &hs);
    p.cycle = CycleChoice::Auto;
    let t = drill(&p).unwrap();
    assert_ne!(t.cycles[0].kind, CycleKind::Ream, "a reamer is the user's word");
}

#[test]
fn ream_refuses_a_tool_that_will_not_enter_and_a_stepped_hole() {
    let tool = drill_tool(8.0);
    let narrow = [hole(3.0, false)];
    let e = drill(&ream_params(&tool, &narrow)).unwrap_err();
    let m = format!("{e:?}");
    assert!(m.contains("ream tool dia. 8 will not enter"), "{m}");
    let stepped = [hole(4.0, true)];
    let e = drill(&ream_params(&tool, &stepped)).unwrap_err();
    let m = format!("{e:?}");
    assert!(m.contains("ream cycle") && m.contains("counterbore"), "{m}");
}

#[test]
fn ream_reads_the_hole_as_pre_drilled() {
    // A reamer is not a centre-cutting drill; the cycle opens a hole
    // that is already there, like bore and tap.
    let mut tool = drill_tool(8.0);
    tool.kind = ToolKind::Drill;
    tool.center_cutting = false;
    let hs = [hole(4.0, false)];
    assert!(drill(&ream_params(&tool, &hs)).is_ok());
    let mut p = ream_params(&tool, &hs);
    p.cycle = CycleChoice::Drill;
    assert!(drill(&p).is_err(), "the same tool still may not drill into solid");
}
