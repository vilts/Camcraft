//! #1480: the back-bore cycle (RS-274/NGC G89 - NIST IR 6556, LinuxCNC's
//! G-code reference): feed in, dwell at the bottom, feed out at the same
//! feed. The chain carries the user's dwell and one feed, as the canned
//! word's single F does.

use super::tests::drill_tool;
use super::*;
use crate::cl::{CycleKind, MoveKind};
use crate::geo2d::Point2;

fn back_bore_params<'a>(tool: &'a crate::tools::Tool, holes: &'a [Hole], dwell: f64) -> CycleParams<'a> {
    CycleParams {
        through_clear: crate::drilling::THROUGH_CLEAR,
        break_through_from: BreakThroughFrom::default(),
        clearance_z: 15.0,
        r_z: 10.0,
        hop_z: None,
        bottom_z: None,
        cycle: CycleChoice::BackBore,
        peck: 0.0,
        dwell,
        feed: 500.0,
        plunge_feed: 100.0,
        tool,
        holes,
    }
}

fn hole() -> Hole {
    Hole {
        xy: Point2::new(10.0, 10.0),
        radius: 4.0,
        entry_z: 10.0,
        floor_z: 4.0,
        through: false,
        stepped: false,
    }
}

#[test]
fn back_bore_carries_the_dwell() {
    let tool = drill_tool(8.0);
    let hs = [hole()];
    let t = drill(&back_bore_params(&tool, &hs, 1.5)).unwrap();
    assert_eq!(t.cycles.len(), 1);
    let c = &t.cycles[0];
    assert_eq!(c.kind, CycleKind::BackBore);
    assert!(
        (c.dwell - 1.5).abs() < 1e-9,
        "G89 dwells at the bottom: the cycle record carries the user's 1.5 s, got {}",
        c.dwell
    );
    // No dwell asked, none carried: such a program posts as before.
    let t0 = drill(&back_bore_params(&tool, &hs, 0.0)).unwrap();
    assert_eq!(t0.cycles[0].dwell, 0.0);
}

#[test]
fn back_bore_feeds_in_and_out_at_the_plunge_feed() {
    let tool = drill_tool(8.0);
    let hs = [hole()];
    for dwell in [0.0, 1.5] {
        let t = drill(&back_bore_params(&tool, &hs, dwell)).unwrap();
        let c = &t.cycles[0];
        let g = &t.moves[c.first_move..];
        assert_eq!(g[0].kind, MoveKind::Plunge, "feed in");
        assert_eq!(g[0].feed, Some(100.0));
        assert!((g[0].to.z - 4.0).abs() < 1e-9, "bores to the floor");
        assert_eq!(g[1].kind, MoveKind::Feed, "feed out, never a rapid over the bored wall");
        assert_eq!(
            g[1].feed,
            Some(100.0),
            "the canned word has one F for both directions: the chain feeds out at the feed it fed in at"
        );
        assert!((g[1].to.z - 10.0).abs() < 1e-9, "feeds back to R");
    }
}
