//! #1449 (first slice): the 2-D pocket enters down holes the caller has
//! proved open. A level served by a hole is entered by one straight
//! plunge at the hole centre - no helix, no ramp; a level below every
//! proved floor, a hole outside the cutter-centre area, or a tool that
//! is not centre-cutting keeps the asked entry and says why by name.
//!
//! Gate map (#1449): covers camcraft_motion::machining2d (predrilled,
//! descent, levels, pocket).

use camcraft_motion::approach::Approach;
use camcraft_motion::cl::{MoveKind, Toolpath};
use camcraft_motion::entry::Entry3D;
use camcraft_motion::geo2d::{Loop, Point2, Region};
use camcraft_motion::links::Links;
use camcraft_motion::machining2d::{PocketParams, PocketStrategy, PreHole, pocket_entered};

const HOLE: Point2 = Point2 { x: 20.0, y: 15.0 };

fn square() -> Region {
    let pts = vec![Point2::new(0.0, 0.0), Point2::new(40.0, 0.0), Point2::new(40.0, 30.0), Point2::new(0.0, 30.0)];
    match Loop::new(pts).and_then(|l| Region::new(vec![l])) {
        Ok(r) => r,
        Err(e) => panic!("region: {e:?}"),
    }
}

fn pp(predrilled: bool, center_cutting: bool, strategy: PocketStrategy) -> PocketParams {
    PocketParams {
        blend_radius: None,
        tool_diameter: 6.0,
        stepover: 2.5,
        stock_to_leave: 0.0,
        clearance_z: 25.0,
        top_z: 0.0,
        bottom_z: -8.0,
        stepdown: 4.0,
        feed: 500.0,
        plunge_feed: 100.0,
        climb: true,
        strategy,
        retract_z: Some(15.0),
        entry: Entry3D::Helix,
        helix_radius: 1.5,
        optimal_load: camcraft_motion::load::DEFAULT_OPTIMAL_LOAD,
        blend_turns: false,
        links: Links {
            approach: Approach { predrilled, center_cutting, ..Approach::DEFAULT },
            ..Links::PLANAR
        },
    }
}

fn run(p: &PocketParams, holes: &[PreHole]) -> Toolpath {
    match pocket_entered(&square(), p, holes) {
        Ok(t) => t,
        Err(e) => panic!("pocket: {e:?}"),
    }
}

/// Z of every straight axial plunge at `xy`.
fn plunges_at(t: &Toolpath, xy: Point2) -> Vec<f64> {
    t.moves
        .iter()
        .filter(|m| m.kind != MoveKind::Rapid && m.arc.is_none())
        .filter(|m| m.from.x == xy.x && m.from.y == xy.y && m.to.x == xy.x && m.to.y == xy.y && m.to.z < m.from.z)
        .map(|m| m.to.z)
        .collect()
}

/// Descending arcs: the helix's half turns.
fn helix_moves(t: &Toolpath) -> usize {
    t.moves.iter().filter(|m| m.arc.is_some() && m.to.z < m.from.z - 1e-9).count()
}

#[test]
fn a_proved_hole_is_plunged_at_every_level_with_no_helix() {
    for s in [PocketStrategy::Zigzag, PocketStrategy::Waterline] {
        let t = run(&pp(true, true, s), &[PreHole { at: HOLE, floor: -8.5 }]);
        assert_eq!(plunges_at(&t, HOLE), vec![0.0, -4.0, -8.0], "{s:?}: one plunge per level (the top level included) down the hole");
        assert_eq!(helix_moves(&t), 0, "{s:?}: no helix where the hole serves");
        assert!(t.notes.iter().any(|n| n.contains("down the pre-drilled hole at (20.000, 15.000)")), "{s:?}: {:?}", t.notes);
    }
}

#[test]
fn without_a_hole_or_the_flag_the_helix_stands() {
    // The control: the same pocket helixes when no hole is handed over,
    // and when the flag is off a hole handed over is never read.
    for (on, holes) in [(true, vec![]), (false, vec![PreHole { at: HOLE, floor: -8.5 }])] {
        let t = run(&pp(on, true, PocketStrategy::Zigzag), &holes);
        assert!(helix_moves(&t) > 0, "flag {on}: the asked helix");
        assert!(plunges_at(&t, HOLE).is_empty());
        assert!(!t.notes.iter().any(|n| n.contains("pre-drilled")), "{:?}", t.notes);
    }
}

#[test]
fn a_shallow_hole_serves_only_the_levels_it_reaches() {
    // Open only to Z -5: the 0 and -4 levels plunge down it, the -8 level is
    // below the hole's floor and keeps the helix, said with the numbers.
    let t = run(&pp(true, true, PocketStrategy::Zigzag), &[PreHole { at: HOLE, floor: -5.0 }]);
    assert_eq!(plunges_at(&t, HOLE), vec![0.0, -4.0]);
    assert!(helix_moves(&t) > 0, "the level below the hole helixes");
    assert!(
        t.notes.iter().any(|n| n.contains("open only to Z -5.000, above this level at Z -8.000")),
        "{:?}",
        t.notes
    );
}

#[test]
fn a_hole_outside_the_cutter_area_is_never_plunged() {
    // 1 mm from the wall: the tool's centre cannot stand there.
    let at = Point2::new(1.0, 15.0);
    let t = run(&pp(true, true, PocketStrategy::Zigzag), &[PreHole { at, floor: -9.0 }]);
    assert!(plunges_at(&t, at).is_empty());
    assert!(helix_moves(&t) > 0);
    assert!(t.notes.iter().any(|n| n.contains("no hole open to Z")), "{:?}", t.notes);
}

#[test]
fn a_tool_that_is_not_centre_cutting_keeps_the_asked_entry() {
    let t = run(&pp(true, false, PocketStrategy::Zigzag), &[PreHole { at: HOLE, floor: -9.0 }]);
    assert!(plunges_at(&t, HOLE).is_empty());
    assert!(t.notes.iter().any(|n| n.contains("not centre-cutting")), "{:?}", t.notes);
}

#[test]
fn a_non_finite_hole_is_ignored() {
    let bad = PreHole { at: Point2::new(f64::NAN, 15.0), floor: -9.0 };
    let t = run(&pp(true, true, PocketStrategy::Zigzag), &[bad]);
    assert!(helix_moves(&t) > 0);
}

