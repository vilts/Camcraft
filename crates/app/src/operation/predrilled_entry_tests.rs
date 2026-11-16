//! #1449: the hole proof, unit by unit - a hole is handed to the pocket
//! only when the drill is wide enough AND the prior grid shows it open;
//! its depth is the deeper of the grid's reading and the drill point's
//! cone at the tool's disc. Our own fixtures.

use super::predrilled_entry::{CLEARANCE, proved_holes};
use crate::cl::{Move, MoveKind};
use crate::geometry::Point3;
use crate::rest::DrillAhead;
use crate::stock_sim::Dexel;

const COT: f64 = 0.600_860_619_027_560_4;

fn mv(kind: MoveKind, from: Point3, to: Point3) -> Move {
    Move { rotary: None, kind, from, to, feed: Some(80.0), arc: None, axis: None }
}

/// A drill of `radius` at (0, 0), its tip down to `tip` from Z 25.
fn drill(radius: f64, tip: f64) -> DrillAhead {
    let o = |z| Point3::new(0.0, 0.0, z);
    DrillAhead {
        radius,
        cone: Some(COT),
        moves: vec![mv(MoveKind::Rapid, o(35.0), o(26.0)), mv(MoveKind::Feed, o(26.0), o(tip)), mv(MoveKind::Rapid, o(tip), o(35.0))],
    }
}

/// A 40 x 40 block, top Z 25, cell 0.1: `open` = Some((r, z)) cuts the
/// disc of radius `r` about the origin down to `z`.
fn grid(open: Option<(f64, f64)>) -> Dexel {
    let mut d = match Dexel::block(-20.0, -20.0, 20.0, 20.0, 25.0, 0.1) {
        Ok(d) => d,
        Err(e) => panic!("block: {e:?}"),
    };
    if let Some((r, z)) = open {
        for iy in 0..d.ny {
            for ix in 0..d.nx {
                let c = d.center(ix, iy);
                if c.x.hypot(c.y) <= r
                    && let Some(k) = d.col(c)
                {
                    d.lower(k, z);
                }
            }
        }
    }
    d
}

#[test]
fn an_open_wide_hole_is_handed_over_to_its_cone_depth() {
    let g = grid(Some((3.0, 5.0)));
    let (holes, notes) = proved_holes(&[drill(3.0, 5.0)], 2.0, Some(&g), false);
    assert!(notes.is_empty(), "{notes:?}");
    assert_eq!(holes.len(), 1);
    let want = 5.0 + (2.0 + CLEARANCE) * COT;
    assert!((holes[0].floor - want).abs() < 1e-9, "the point cone at the disc: {} vs {want}", holes[0].floor);
}

#[test]
fn a_hole_the_grid_does_not_show_is_never_air() {
    // The drill's moves say a hole; the stock the program ahead leaves
    // says solid: the hole is handed over at the stock's top, which no
    // level below it can use.
    let g = grid(None);
    let (holes, _) = proved_holes(&[drill(3.0, 5.0)], 2.0, Some(&g), false);
    assert_eq!(holes.len(), 1);
    assert!((holes[0].floor - 25.0).abs() < 1e-9, "{}", holes[0].floor);
    // Cut shallower than the drill's moves say: the grid's depth wins.
    let g = grid(Some((3.0, 18.0)));
    let (holes, _) = proved_holes(&[drill(3.0, 5.0)], 2.0, Some(&g), false);
    assert!((holes[0].floor - 18.0).abs() < 1e-9, "{}", holes[0].floor);
}

#[test]
fn an_unreadable_prior_proves_nothing() {
    let (holes, notes) = proved_holes(&[drill(3.0, 5.0)], 2.0, None, false);
    assert!(holes.is_empty());
    assert!(notes.iter().any(|n| n.contains("cannot be proved open")), "{notes:?}");
}

#[test]
fn a_drill_narrower_than_the_tool_plus_clearance_is_left_out_by_name() {
    let g = grid(Some((2.0, 5.0)));
    let (holes, notes) = proved_holes(&[drill(2.0, 5.0)], 2.0, Some(&g), false);
    assert!(holes.is_empty());
    assert!(notes.iter().any(|n| n.contains("4.000 mm across - under the 4.200 mm")), "{notes:?}");
}

#[test]
fn a_posed_pocket_and_no_drill_are_named() {
    let g = grid(Some((3.0, 5.0)));
    let (holes, notes) = proved_holes(&[drill(3.0, 5.0)], 2.0, Some(&g), true);
    assert!(holes.is_empty() && notes.iter().any(|n| n.contains("3+2")), "{notes:?}");
    let (holes, notes) = proved_holes(&[], 2.0, Some(&g), false);
    assert!(holes.is_empty() && notes.iter().any(|n| n.contains("no drilling operation ahead")), "{notes:?}");
}
