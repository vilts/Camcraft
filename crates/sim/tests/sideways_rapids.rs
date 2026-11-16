//! Issue #140: law (d) judges a lateral rapid under the clearance plane
//! along its whole length - a link at floor depth between two pockets
//! whose start, midpoint and end all stand in cut air still drives
//! through the uncut wall between them, and is a rapid in stock; a long
//! link over fully cut floor stays clean.
//!
//! Provenance: our own fixtures, specced from the Prime Directive and the
//! shop rule "never traverse through uncut stock"; no vendor material.
//!
//! Gate map (#140): covers camcraft_sim::verify (rapids, rest).

use camcraft_sim::cl::{Move, MoveKind, Toolpath};
use camcraft_sim::geo2d::{Loop, Point2, Region};
use camcraft_sim::geometry::Point3;
use camcraft_sim::verify::{Mode, ToolGeom, VerifyContext, Warning, verify};

/// Tool radius, mm: a slot chord along x = 0 clears |x| <= R.
const R: f64 = 3.0;
/// The pockets' floor, under the Z 25 stock top.
const FLOOR: f64 = 15.0;

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

/// A slot pocket along x = 0 whose tool centre runs y0 -> y1 at the
/// floor: it clears y in [y0 - R, y1 + R]. Enters from above the stock
/// top by a plunge, leaves by a retract to clearance.
fn slot(y0: f64, y1: f64) -> Vec<Move> {
    vec![
        at(MoveKind::Rapid, (0.0, y0, 35.0), (0.0, y0, 26.0)),
        at(MoveKind::Plunge, (0.0, y0, 26.0), (0.0, y0, FLOOR)),
        at(MoveKind::Feed, (0.0, y0, FLOOR), (0.0, y1, FLOOR)),
    ]
}

/// `pockets` (each a slot), then a feed back to (0, y_a) at the floor
/// and the link rapid (0, y_a) -> (0, y_b) at the floor - the judged
/// move, whose index is returned - and a retract.
fn link_after(pockets: &[(f64, f64)], y_a: f64, y_b: f64) -> (Vec<Warning>, usize) {
    let mut moves = Vec::new();
    for &(y0, y1) in pockets {
        moves.extend(slot(y0, y1));
        moves.push(at(MoveKind::Rapid, (0.0, y1, FLOOR), (0.0, y1, 35.0)));
    }
    let end = pockets.last().map_or(y_a, |p| p.1);
    moves.pop(); // the last pocket's retract: the link starts at its floor
    moves.push(at(MoveKind::Feed, (0.0, end, FLOOR), (0.0, y_a, FLOOR)));
    let link = moves.len();
    moves.push(at(MoveKind::Rapid, (0.0, y_a, FLOOR), (0.0, y_b, FLOOR)));
    moves.push(at(MoveKind::Rapid, (0.0, y_b, FLOOR), (0.0, y_b, 35.0)));
    (judge(moves), link)
}

fn judge(moves: Vec<Move>) -> Vec<Warning> {
    let p = Toolpath { moves, cycles: Vec::new(), notes: Vec::new(), comps: Vec::new(), threads: Vec::new(), probes: Vec::new(), breaks: Vec::new(), plasma: None };
    let region = rect();
    let tool = ToolGeom {
        radius: R,
        nose_radius: 0.0,
        flute_len: 30.0,
        shank_radius: R,
        shank_len: 20.0,
        holder_radius: R,
        holder_len: 1.0,
    };
    let cx = VerifyContext {
        region: &region,
        stock: Some(&region),
        model: None,
        tool: &tool,
        cutter: None,
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
fn a_floor_link_through_a_4mm_wall_between_two_pockets_is_a_rapid_in_stock() {
    // The issue's scenario: pockets cut over y in [-8, 30] then
    // [-30, -12] leave the wall y in [-12, -8]; the link (0, -20) ->
    // (0, 20) has its start, mid and end all in cut air.
    let (w, link) = link_after(&[(-5.0, 27.0), (-27.0, -15.0)], -20.0, 20.0);
    assert!(w.contains(&Warning::RapidInStock { at: link }), "{w:?}");
}

#[test]
fn a_floor_link_through_a_1mm_wall_is_a_rapid_in_stock() {
    // The wall y in [-12, -11], thinner than the tool radius and well
    // under the gap three samples leave.
    let (w, link) = link_after(&[(-8.0, 27.0), (-27.0, -15.0)], -20.0, 20.0);
    assert!(w.contains(&Warning::RapidInStock { at: link }), "{w:?}");
}

#[test]
fn a_long_floor_link_over_a_fully_cut_slot_is_clean() {
    // One slot clears y in [-40, 40]; the link runs 68 mm over its floor.
    let (w, link) = link_after(&[(-37.0, 37.0)], -34.0, 34.0);
    assert!(!w.contains(&Warning::RapidInStock { at: link }), "{w:?}");
    assert!(w.is_empty(), "{w:?}");
}
