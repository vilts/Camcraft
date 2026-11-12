//! Issue #96: law (d) judges a vertical rapid DOWN by its column - a
//! drill's rapid onto an R plane under the stock top crosses uncut
//! stock and is a rapid in stock; a peck's re-entry above its own
//! earlier plunge, a rapid stopping above the stock top, and a retract
//! up the tool's own kerf stay clean.
//!
//! Provenance: our own fixtures, specced from the Prime Directive and the
//! shop rule "never traverse through uncut stock"; no vendor material.
//!
//! Issue #106: the column is read under the tool's footprint (the disc
//! of radius r - gouge_tol), not its centre alone - a dive or a lateral
//! rapid whose centre stands off the stock but whose rim meets it, and
//! a re-entry beside an earlier plunge whose far side is uncut, convict.
//!
//! Each footprint sample is judged at the tip's own height at its
//! radius (#106 review): a ball or bull whose lifted rim clears the
//! wall is air, one whose lifted rim still meets the stock is not.
//!
//! Gate map (#96, #106): covers camcraft_sim::verify (rapids, rest).

use camcraft_sim::cl::{Move, MoveKind, Toolpath};
use camcraft_sim::geo2d::{Loop, Point2, Region};
use camcraft_sim::geometry::Point3;
use camcraft_sim::verify::{Cutter, Mode, ToolGeom, VerifyContext, Warning, verify};

fn rect() -> Region {
    let l = Loop::new(vec![
        Point2::new(-40.0, -40.0),
        Point2::new(40.0, -40.0),
        Point2::new(40.0, 40.0),
        Point2::new(-40.0, 40.0),
    ]);
    match l.and_then(|l| Region::new(vec![l])) {
        Ok(r) => r,
        Err(e) => panic!("rect: {e:?}"),
    }
}

fn mv(kind: MoveKind, z0: f64, z1: f64) -> Move {
    at(kind, (0.0, 0.0, z0), (0.0, 0.0, z1))
}

fn at(kind: MoveKind, a: (f64, f64, f64), b: (f64, f64, f64)) -> Move {
    Move {
        rotary: None,
        kind,
        from: Point3::new(a.0, a.1, a.2),
        to: Point3::new(b.0, b.1, b.2),
        feed: if kind == MoveKind::Rapid { None } else { Some(80.0) },
        arc: None,
        axis: None,
    }
}

/// `moves` on a drill over a stock box whose top is Z 25, clearance 35.
fn judge(moves: Vec<Move>) -> Vec<Warning> {
    judge_tip(moves, 2.0, None)
}

/// `moves` on a tool of radius `r` whose tip has corner radius
/// `corner` (`Some(r)` a ball, between a bull, `None` no stated tip).
fn judge_tip(moves: Vec<Move>, r: f64, corner: Option<f64>) -> Vec<Warning> {
    let p = Toolpath { moves, cycles: Vec::new(), notes: Vec::new(), comps: Vec::new(), threads: Vec::new(), probes: Vec::new(), breaks: Vec::new(), plasma: None };
    let region = rect();
    let tool = ToolGeom {
        radius: r,
        nose_radius: 0.0,
        flute_len: 30.0,
        shank_radius: r,
        shank_len: 20.0,
        holder_radius: r,
        holder_len: 1.0,
    };
    let cutter = corner.map(|c| Cutter { corner_r: c, ..Cutter::flat(&tool) });
    let cx = VerifyContext {
        region: &region,
        stock: Some(&region),
        model: None,
        tool: &tool,
        cutter,
        mode: Mode::Area,
        stock_top_z: 25.0,
        clearance_z: 35.0,
        stock_to_leave: 0.0,
        gouge_tol: 0.01,
        z_tol: 1e-3,
        finish_tol: None,
        fixtures: &[],
        spindle: None,
        ..Default::default()
    };
    verify(&p, &cx)
}

#[test]
fn a_vertical_rapid_down_to_an_r_plane_under_the_stock_top_is_a_rapid_in_stock() {
    let w = judge(vec![
        mv(MoveKind::Rapid, 35.0, 20.0),
        mv(MoveKind::Plunge, 20.0, 12.0),
        mv(MoveKind::Rapid, 12.0, 35.0),
    ]);
    assert_eq!(w, vec![Warning::RapidInStock { at: 0 }], "{w:?}");
}

#[test]
fn the_same_drill_with_its_r_plane_over_the_stock_top_is_clean() {
    let w = judge(vec![
        mv(MoveKind::Rapid, 35.0, 26.0),
        mv(MoveKind::Plunge, 26.0, 12.0),
        mv(MoveKind::Rapid, 12.0, 35.0),
    ]);
    assert!(w.is_empty(), "{w:?}");
}

#[test]
fn a_peck_re_entry_above_its_own_earlier_plunge_is_clean() {
    let w = judge(vec![
        mv(MoveKind::Rapid, 35.0, 26.0),
        mv(MoveKind::Plunge, 26.0, 18.0),
        mv(MoveKind::Rapid, 18.0, 26.0),
        mv(MoveKind::Rapid, 26.0, 18.5),
        mv(MoveKind::Plunge, 18.5, 12.0),
        mv(MoveKind::Rapid, 12.0, 35.0),
    ]);
    assert!(w.is_empty(), "{w:?}");
}

#[test]
fn a_peck_re_entry_past_its_own_earlier_plunge_is_a_rapid_in_stock() {
    let w = judge(vec![
        mv(MoveKind::Rapid, 35.0, 26.0),
        mv(MoveKind::Plunge, 26.0, 18.0),
        mv(MoveKind::Rapid, 18.0, 26.0),
        mv(MoveKind::Rapid, 26.0, 16.0),
        mv(MoveKind::Plunge, 16.0, 12.0),
        mv(MoveKind::Rapid, 12.0, 35.0),
    ]);
    assert_eq!(w, vec![Warning::RapidInStock { at: 3 }], "{w:?}");
}

/// The stock wall is x = 40 and the tool radius 2: a dive centred at
/// x = 41 (r/2 off the wall) lands its rim 1 mm into the stock.
#[test]
fn a_dive_centred_half_a_radius_off_the_stock_wall_is_a_rapid_in_stock() {
    let w = judge(vec![
        at(MoveKind::Rapid, (41.0, 0.0, 35.0), (41.0, 0.0, 20.0)),
        at(MoveKind::Plunge, (41.0, 0.0, 20.0), (41.0, 0.0, 12.0)),
        at(MoveKind::Rapid, (41.0, 0.0, 12.0), (41.0, 0.0, 35.0)),
    ]);
    // The plunge off the region is law (a)'s Gouge; the dive is ours.
    assert!(w.contains(&Warning::RapidInStock { at: 0 }), "{w:?}");
    assert!(!w.contains(&Warning::RapidInStock { at: 2 }), "the retract rides its kerf: {w:?}");
}

#[test]
fn a_dive_whose_whole_disc_clears_the_stock_wall_is_clean() {
    // Centre at x = 42.5: the rim stops 0.5 mm short of the wall.
    let w = judge(vec![
        at(MoveKind::Rapid, (42.5, 0.0, 35.0), (42.5, 0.0, 20.0)),
        at(MoveKind::Plunge, (42.5, 0.0, 20.0), (42.5, 0.0, 12.0)),
        at(MoveKind::Rapid, (42.5, 0.0, 12.0), (42.5, 0.0, 35.0)),
    ]);
    assert!(w.is_empty(), "{w:?}");
}

#[test]
fn a_dive_touching_the_wall_within_the_gouge_tolerance_is_clean() {
    // Centre exactly r off the wall: the disc r - gouge_tol never reaches it.
    let w = judge(vec![
        at(MoveKind::Rapid, (42.0, 0.0, 35.0), (42.0, 0.0, 20.0)),
        at(MoveKind::Plunge, (42.0, 0.0, 20.0), (42.0, 0.0, 12.0)),
        at(MoveKind::Rapid, (42.0, 0.0, 12.0), (42.0, 0.0, 35.0)),
    ]);
    assert!(w.is_empty(), "{w:?}");
}

#[test]
fn a_re_entry_beside_an_earlier_plunge_with_its_far_side_uncut_is_a_rapid_in_stock() {
    // The plunge at x = 0 cleared the column at x = 1.5 (within r), but
    // the re-entry's far rim at x = 3.5 stands on the stock top.
    let w = judge(vec![
        mv(MoveKind::Rapid, 35.0, 26.0),
        mv(MoveKind::Plunge, 26.0, 18.0),
        mv(MoveKind::Rapid, 18.0, 26.0),
        at(MoveKind::Rapid, (0.0, 0.0, 26.0), (1.5, 0.0, 26.0)),
        at(MoveKind::Rapid, (1.5, 0.0, 26.0), (1.5, 0.0, 18.5)),
        at(MoveKind::Plunge, (1.5, 0.0, 18.5), (1.5, 0.0, 12.0)),
        at(MoveKind::Rapid, (1.5, 0.0, 12.0), (1.5, 0.0, 35.0)),
    ]);
    assert_eq!(w, vec![Warning::RapidInStock { at: 4 }], "{w:?}");
}

#[test]
fn a_lateral_rapid_centred_off_the_stock_wall_with_its_rim_in_stock_is_a_rapid_in_stock() {
    let w = judge(vec![
        at(MoveKind::Rapid, (41.0, -10.0, 35.0), (41.0, -10.0, 20.0)),
        at(MoveKind::Rapid, (41.0, -10.0, 20.0), (41.0, 10.0, 20.0)),
        at(MoveKind::Rapid, (41.0, 10.0, 20.0), (41.0, 10.0, 35.0)),
    ]);
    assert!(w.contains(&Warning::RapidInStock { at: 1 }), "{w:?}");
}

#[test]
fn a_lateral_rapid_whose_whole_disc_clears_the_stock_wall_is_clean() {
    let w = judge(vec![
        at(MoveKind::Rapid, (42.5, -10.0, 35.0), (42.5, -10.0, 20.0)),
        at(MoveKind::Rapid, (42.5, -10.0, 20.0), (42.5, 10.0, 20.0)),
        at(MoveKind::Rapid, (42.5, 10.0, 20.0), (42.5, 10.0, 35.0)),
    ]);
    assert!(w.is_empty(), "{w:?}");
}

fn dive_at(x: f64, z: f64) -> Vec<Move> {
    vec![at(MoveKind::Rapid, (x, 0.0, 35.0), (x, 0.0, z))]
}

fn convicted(w: &[Warning]) -> bool {
    w.contains(&Warning::RapidInStock { at: 0 })
}

#[test]
fn a_ball_dive_beside_the_wall_with_its_rim_lifted_clear_is_clean() {
    // Ball r 3 at x 42.9 to Z 24.5: the rim over the wall (rho 2.9)
    // stands at 24.5 + 3 - sqrt(9 - 8.41) = 26.7, above the Z 25 top.
    let w = judge_tip(dive_at(42.9, 24.5), 3.0, Some(3.0));
    assert!(!convicted(&w), "{w:?}");
    // The same dive read as a square end meets the stock with its rim.
    assert!(convicted(&judge_tip(dive_at(42.9, 24.5), 3.0, None)));
}

#[test]
fn a_ball_dive_whose_lifted_footprint_still_meets_the_stock_is_a_rapid_in_stock() {
    // Ball r 3 at x 41 to Z 24: at rho 1.5 (x 39.5, in stock) the ball
    // stands at 24 + 3 - sqrt(9 - 2.25) = 24.4, under the Z 25 top.
    let w = judge_tip(dive_at(41.0, 24.0), 3.0, Some(3.0));
    assert!(convicted(&w), "{w:?}");
}

#[test]
fn a_bull_dive_whose_corner_band_meets_the_wall_is_a_rapid_in_stock() {
    // Bull r 3, corner 1 (flat to rho 2) at x 41.6 to Z 24.5 (PR #138
    // re-review): over the wall (rho 1.6..3) the corner stands only
    // 0.2 mm up at rho 2.6 - Z 24.7, in the stock. A rim sample
    // alone, lifted 0.86, read it as air.
    let w = judge_tip(dive_at(41.6, 24.5), 3.0, Some(1.0));
    assert!(convicted(&w), "{w:?}");
}

#[test]
fn a_ball_dive_with_the_wall_between_its_sample_rings_is_a_rapid_in_stock() {
    // The re-review's rows: the stock edge falls between the half ring
    // and the rim, where the ball still stands under the Z 25 top.
    for (x, z) in [(41.51, 22.5), (42.0, 24.0)] {
        let w = judge_tip(dive_at(x, z), 3.0, Some(3.0));
        assert!(convicted(&w), "ball at x {x} to Z {z}: {w:?}");
    }
}

#[test]
fn a_bull_dive_whose_flat_reaches_over_the_wall_is_a_rapid_in_stock() {
    // Bull r 3, corner 1 at x 42.2 to Z 24.5: the flat (rho 2) ends
    // 0.2 mm short of the wall, the corner rises only 0.02 by it.
    let w = judge_tip(dive_at(42.2, 24.5), 3.0, Some(1.0));
    assert!(convicted(&w), "{w:?}");
}

#[test]
fn a_bull_dive_deeper_than_its_corner_lift_is_a_rapid_in_stock() {
    // The same bull to Z 23.5: its rim stands at Z 24.36, in the stock.
    let w = judge_tip(dive_at(41.6, 23.5), 3.0, Some(1.0));
    assert!(convicted(&w), "{w:?}");
}
