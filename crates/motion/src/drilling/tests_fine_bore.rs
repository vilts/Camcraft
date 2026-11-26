//! #1445, #1479: the fine-boring cycle (the published G76 fine boring
//! canned cycle, Haas Mill Operator's Manual): feed in, dwell, oriented
//! spindle stop, shift off the wall, rapid out, shift back. The chain
//! makes the shift, and the doors refuse a shift that is missing or that
//! the bar has no room for.

use super::tests::drill_tool;
use super::*;
use crate::cl::{CycleKind, MoveKind};
use crate::geo2d::Point2;
use crate::tools::ToolKind;

fn hole(stepped: bool) -> Hole {
    Hole { xy: Point2::new(10.0, 10.0), radius: 10.0, entry_z: 10.0, floor_z: 4.0, through: false, stepped }
}

/// A dia. 20 boring bar on a dia. `bar` shank.
fn bar_tool(bar: f64) -> crate::tools::Tool {
    let mut t = drill_tool(20.0);
    t.kind = ToolKind::Bore;
    t.shank_d = bar;
    t
}

fn params<'a>(tool: &'a crate::tools::Tool, holes: &'a [Hole], cycle: CycleChoice) -> CycleParams<'a> {
    CycleParams {
        through_clear: crate::drilling::THROUGH_CLEAR,
        break_through_from: BreakThroughFrom::default(),
        clearance_z: 15.0,
        r_z: 11.0,
        hop_z: None,
        bottom_z: None,
        cycle,
        peck: 0.0,
        dwell: 0.5,
        feed: 500.0,
        plunge_feed: 80.0,
        tool,
        holes,
    }
}

#[test]
fn fine_bore_shifts_off_the_wall_before_the_rapid_out() {
    let tool = bar_tool(16.0);
    let hs = [hole(false)];
    let cycle = CycleChoice::fine_bore(0.3, -0.4).unwrap();
    let t = drill(&params(&tool, &hs, cycle)).unwrap();
    assert!(t.is_chained() && t.all_finite());
    assert_eq!(t.cycles.len(), 1);
    let c = &t.cycles[0];
    assert_eq!(c.kind, CycleKind::FineBore);
    assert!((c.dwell - 0.5).abs() < 1e-9, "G76 dwells at the bottom: the record carries the dwell");
    let g = &t.moves[c.first_move..];
    let at = |i: usize| (g[i].kind, g[i].to.x, g[i].to.y, g[i].to.z);
    assert_eq!(at(0), (MoveKind::Plunge, 10.0, 10.0, 4.0), "feed in on the hole axis");
    assert_eq!(g[0].feed, Some(80.0));
    assert_eq!(at(1), (MoveKind::Rapid, 10.3, 9.6, 4.0), "the shift, at the bottom, before any lift");
    assert_eq!(at(2), (MoveKind::Rapid, 10.3, 9.6, 11.0), "rapid out at the shifted XY, never on the axis");
    assert_eq!(at(3), (MoveKind::Rapid, 10.0, 10.0, 11.0), "shift back at R");
    // No move lifts the tool on the hole axis from the bottom.
    assert!(
        !g.windows(2).any(|w| w[0].to.z < 10.0 && w[1].to.z > w[0].to.z && w[1].to.x == 10.0 && w[1].to.y == 10.0),
        "the tip never drags up the finished wall"
    );
}

#[test]
fn the_fine_bore_choice_has_no_default_shift() {
    assert!(CycleChoice::fine_bore(0.0, 0.0).unwrap_err().contains("needs a shift"));
    assert!(CycleChoice::fine_bore(0.0004, 0.0).unwrap_err().contains("needs a shift"), "under one micrometre is zero");
    assert!(CycleChoice::fine_bore(f64::NAN, 0.1).unwrap_err().contains("finite"));
    assert!(CycleChoice::fine_bore(0.1, f64::INFINITY).unwrap_err().contains("finite"));
    assert!(CycleChoice::fine_bore(5.001, 0.0).unwrap_err().contains("larger than 5"));
    let c = CycleChoice::fine_bore(-0.25, 0.1).unwrap();
    assert_eq!(c, CycleChoice::FineBore { shift_x_um: -250, shift_y_um: 100 });
    assert_eq!(c.fine_bore_shift(), Some([-0.25, 0.1]));
    assert_eq!(CycleChoice::Bore.fine_bore_shift(), None);
}

#[test]
fn a_hand_made_zero_shift_is_refused_by_the_generator() {
    let tool = bar_tool(16.0);
    let hs = [hole(false)];
    let zero = CycleChoice::FineBore { shift_x_um: 0, shift_y_um: 0 };
    let e = drill(&params(&tool, &hs, zero)).unwrap_err().to_string();
    assert!(e.contains("fine bore shift") && e.contains("more than zero"), "{e}");
}

#[test]
fn a_shift_the_bar_has_no_room_for_is_refused_by_name() {
    let hs = [hole(false)];
    // Dia. 20 cut on a dia. 19 bar: 0.5 of room. A 0.5 shift touches.
    let thick = bar_tool(19.0);
    for (dx, dy) in [(0.5, 0.0), (0.4, 0.4)] {
        let e = drill(&params(&thick, &hs, CycleChoice::fine_bore(dx, dy).unwrap())).unwrap_err().to_string();
        assert!(e.contains("does not fit") && e.contains("bar dia. 19") && e.contains("0.5 of room"), "{e}");
    }
    assert!(drill(&params(&thick, &hs, CycleChoice::fine_bore(0.3, 0.0).unwrap())).is_ok(), "0.3 fits in 0.5");
    // A bar as wide as the cut has no room at all.
    let full = bar_tool(20.0);
    let e = drill(&params(&full, &hs, CycleChoice::fine_bore(0.1, 0.0).unwrap())).unwrap_err().to_string();
    assert!(e.contains("does not fit") && e.contains("0 of room"), "{e}");
    // No bar diameter stated: the room is unknown.
    let none = bar_tool(0.0);
    let e = drill(&params(&none, &hs, CycleChoice::fine_bore(0.1, 0.0).unwrap())).unwrap_err().to_string();
    assert!(e.contains("states no bar"), "{e}");
}

#[test]
fn fine_bore_rides_the_bore_door() {
    let tool = bar_tool(16.0);
    let cycle = CycleChoice::fine_bore(0.3, 0.0).unwrap();
    let stepped = [hole(true)];
    let e = drill(&params(&tool, &stepped, cycle)).unwrap_err().to_string();
    assert!(e.contains("counterbore"), "{e}");
    let small = [Hole { radius: 9.0, ..hole(false) }];
    let e = drill(&params(&tool, &small, cycle)).unwrap_err().to_string();
    assert!(e.contains("will not enter"), "{e}");
    // A bar is not centre-cutting: the cycle reads the hole as pre-drilled.
    let mut blind = bar_tool(16.0);
    blind.center_cutting = false;
    assert!(drill(&params(&blind, &[hole(false)], cycle)).is_ok());
}

#[test]
fn the_choice_resolves_to_the_fine_bore_kind() {
    assert!(matches!(
        resolve_choice(&params(&bar_tool(16.0), &[hole(false)], CycleChoice::fine_bore(0.1, 0.0).unwrap())),
        CycleKind::FineBore
    ));
}
