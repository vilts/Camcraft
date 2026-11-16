//! #261: the moves a verified path's warnings name, as the viewport
//! paints them - pinned on a planted gouge ring that `verify` convicts
//! move by move, and on a clean ring it leaves silent.
//!
//! Gate map (M4.4): covers app::warned_moves (over sim::verify's output).

use camcraft_app::cl::{Move, MoveKind, Toolpath};
use camcraft_app::geo2d::{Loop, Point2, Region};
use camcraft_app::geometry::Point3;
use camcraft_app::verify::{Mode, ToolGeom, VerifyContext, verify};
use camcraft_app::warned_moves::{first_move, move_focus, warned_mask};

fn p(x: f64, y: f64, z: f64) -> Point3 {
    Point3::new(x, y, z)
}

fn plate() -> Option<Region> {
    let l = Loop::new(vec![Point2::new(0.0, 0.0), Point2::new(20.0, 0.0), Point2::new(20.0, 20.0), Point2::new(0.0, 20.0)]).ok()?;
    Region::new(vec![l]).ok()
}

fn tool() -> ToolGeom {
    ToolGeom {
        radius: 4.0,
        nose_radius: 0.0,
        flute_len: 30.0,
        shank_radius: 9.0,
        shank_len: 40.0,
        holder_radius: 20.0,
        holder_len: 60.0,
    }
}

/// A plunge, then a square ring `inset` mm in from the 20x20 plate's
/// walls (an r = 4 tool is tangent at 4.0), at z -1; then a rapid out.
fn ring(inset: f64) -> Toolpath {
    let (a, b) = (inset, 20.0 - inset);
    let mut moves = vec![Move::new(MoveKind::Plunge, p(a, a, 5.0), p(a, a, -1.0), Some(100.0))];
    for (f, t) in [((a, a), (b, a)), ((b, a), (b, b)), ((b, b), (a, b)), ((a, b), (a, a))] {
        moves.push(Move::new(MoveKind::Feed, p(f.0, f.1, -1.0), p(t.0, t.1, -1.0), Some(500.0)));
    }
    moves.push(Move::new(MoveKind::Rapid, p(a, a, -1.0), p(a, a, 5.0), None));
    Toolpath { moves, ..Toolpath::default() }
}

fn judge(t: &Toolpath, region: &Region, tool: &ToolGeom) -> Vec<camcraft_app::verify::Warning> {
    let cx = VerifyContext {
        region,
        stock: None,
        model: None,
        tool,
        cutter: None,
        mode: Mode::Boundary,
        stock_top_z: 0.0,
        clearance_z: 5.0,
        stock_to_leave: 0.0,
        gouge_tol: 0.01,
        z_tol: 1e-6,
        finish_tol: None,
        fixtures: &[],
        spindle: None,
        ..Default::default()
    };
    verify(t, &cx)
}

#[test]
fn a_gouged_ring_marks_exactly_its_gouging_moves() {
    let (Some(r), tool) = (plate(), tool()) else { panic!("plate fixture") };
    let t = ring(3.5);
    let warns = judge(&t, &r, &tool);
    assert!(!warns.is_empty(), "the planted ring gouges");
    let mask = warned_mask(&warns, t.moves.len());
    // The plunge and the four ring sides sit 0.5 mm inside tangency;
    // the retract rapid above the stock is no offender.
    assert_eq!(mask, vec![true, true, true, true, true, false], "{warns:?}");
    // The first folded line frames move 0: the plunge's midpoint.
    let at = warns.first().and_then(first_move);
    assert_eq!(at, Some(0));
    let f = at.and_then(|i| move_focus(&t.moves, i)).map(|q| (q.x, q.y, q.z));
    assert_eq!(f, Some((3.5, 3.5, 2.0)));
}

#[test]
fn a_clean_ring_marks_nothing() {
    let (Some(r), tool) = (plate(), tool()) else { panic!("plate fixture") };
    let t = ring(4.5);
    let warns = judge(&t, &r, &tool);
    assert!(warns.is_empty(), "{warns:?}");
    assert!(warned_mask(&warns, t.moves.len()).is_empty());
}
