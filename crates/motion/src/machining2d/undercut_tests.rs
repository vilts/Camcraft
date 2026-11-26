//! Unit tests of the slot undercut generator (#1444): the Z law (every Z
//! move at the entry station), the pass band, the profile contact and
//! the refusals by name.
//!
//! Gate map: `camcraft_motion::machining2d::undercut`.

use super::*;
use camcraft_tools::tools::ToolKind;
use camcraft_tools::undercut::cutter;

fn head(kind: ToolKind) -> Undercut {
    let t = match kind {
        ToolKind::Dovetail => cutter(kind, 20.0, 5.0, 8.0, 30.0, 60.0),
        ToolKind::Lollipop => cutter(kind, 10.0, 5.0, 4.0, 30.0, 0.0),
        _ => cutter(kind, 20.0, 6.0, 8.0, 30.0, 0.0),
    };
    Undercut::of(&t).unwrap()
}

fn params(kind: ToolKind, width: f64) -> UndercutParams {
    UndercutParams {
        cutter: head(kind),
        width,
        bulge: 0.0,
        run_in: 15.0,
        neck_slot: 0.0,
        clearance_z: 10.0,
        retract_z: Some(2.0),
        bottom_z: -12.0,
        stepover: 2.0,
        feed: 120.0,
        plunge_feed: 60.0,
        climb: true,
    }
}

const A: Point2 = Point2 { x: 0.0, y: 0.0 };
const B: Point2 = Point2 { x: 60.0, y: 0.0 };

fn why(r: Result<Toolpath, GenError>) -> String {
    r.unwrap_err().to_string()
}

/// Largest distance of a feed move's ends from the segment A-B.
fn max_off_axis(t: &Toolpath) -> f64 {
    t.moves
        .iter()
        .filter(|m| m.kind == MoveKind::Feed && m.to.x >= -1e-9)
        .flat_map(|m| [m.from, m.to])
        .map(|q| q.y.abs())
        .fold(0.0, f64::max)
}

#[test]
fn every_z_move_stands_at_the_entry_station() {
    for kind in [ToolKind::TSlot, ToolKind::Keyseat, ToolKind::Dovetail, ToolKind::Lollipop] {
        for bulge in [0.0, 0.3, -0.3] {
            let p = UndercutParams { bulge, ..params(kind, 26.0) };
            let t = undercut(A, B, &p).unwrap();
            let s = undercut_station(A, B, &p).unwrap();
            let z_moves: Vec<_> = t.moves.iter().filter(|m| m.from.z != m.to.z).collect();
            assert!(z_moves.len() >= 2, "{kind:?}: a way down and a way up");
            for m in z_moves {
                for q in [m.from, m.to] {
                    assert!((q.x - s.x).hypot(q.y - s.y) < 1e-9, "{kind:?} bulge {bulge}: Z move at {q:?}, station {s:?}");
                }
            }
            // Everything that cuts stands on the floor plane.
            assert!(t.moves.iter().filter(|m| m.kind == MoveKind::Feed).all(|m| m.from.z == -12.0 && m.to.z == -12.0));
            // The path ends where it went down, lifted.
            let end = t.end().unwrap();
            assert!((end.x - s.x).hypot(end.y - s.y) < 1e-9 && end.z == 10.0);
            assert!(t.is_chained());
        }
    }
}

#[test]
fn the_straight_station_is_behind_a_on_the_axis() {
    let s = undercut_station(A, B, &params(ToolKind::TSlot, 26.0)).unwrap();
    assert!((s.x + 15.0).abs() < 1e-12 && s.y.abs() < 1e-12);
}

#[test]
fn the_passes_widen_to_the_wall_by_the_profile_contact_radius() {
    // T-slot: 26 wide, 20 head: the centre shifts 3 mm each side.
    let t = undercut(A, B, &params(ToolKind::TSlot, 26.0)).unwrap();
    assert!((max_off_axis(&t) - 3.0).abs() < 1e-9, "{}", max_off_axis(&t));
    // Lollipop: the 10 mm ball cuts the wall at its equator, 5 mm out
    // and 5 mm above the tip - not at the tip, where its radius is 0.
    let p = params(ToolKind::Lollipop, 14.0);
    assert_eq!(p.cutter.contact(), (5.0, 5.0));
    let t = undercut(A, B, &p).unwrap();
    assert!((max_off_axis(&t) - 2.0).abs() < 1e-9, "{}", max_off_axis(&t));
    // A head exactly as wide as the undercut cuts one centre pass.
    let t = undercut(A, B, &params(ToolKind::Dovetail, 20.0)).unwrap();
    assert!(max_off_axis(&t) < 1e-12);
}

#[test]
fn a_curved_slot_rides_concentric_arcs() {
    let p = UndercutParams { bulge: 0.5, ..params(ToolKind::TSlot, 26.0) };
    let t = undercut(A, B, &p).unwrap();
    let arcs: Vec<_> = t.moves.iter().filter_map(|m| m.arc.map(|a| (m, a))).collect();
    assert_eq!(arcs.len(), 2 + 2 * 2, "centre out and back, two widenings of two passes");
    let c = arcs[0].1.center;
    // bulge 0.5 over a 60 chord: radius 37.5, centre left of the chord.
    assert!((c.x - 30.0).abs() < 1e-6 && (c.y - 22.5).abs() < 1e-6, "{c:?}");
    for (m, a) in &arcs {
        assert_eq!(a.center, c);
        let r = |q: Point3| (q.x - c.x).hypot(q.y - c.y);
        assert!((r(m.from) - r(m.to)).abs() < 1e-6);
        assert!((r(m.to) - 37.5).abs() <= 3.0 + 1e-6, "inside the band: {}", r(m.to));
    }
}

#[test]
fn an_entry_slot_too_narrow_for_the_neck_is_refused_by_name() {
    // Neck 8 mm; the 3 mm shift each side needs a 14 mm slot.
    let p = UndercutParams { neck_slot: 12.0, ..params(ToolKind::TSlot, 26.0) };
    let w = why(undercut(A, B, &p));
    assert!(w.contains("entry slot 12 mm is too narrow for the 8 mm cutter neck") && w.contains("needs 14.000 mm"), "{w}");
    // Under the bare neck is refused too.
    let w = why(undercut(A, B, &UndercutParams { neck_slot: 7.0, ..params(ToolKind::TSlot, 20.0) }));
    assert!(w.contains("entry slot 7 mm is too narrow for the 8 mm cutter neck"), "{w}");
    assert!(undercut(A, B, &UndercutParams { neck_slot: 14.0, ..params(ToolKind::TSlot, 26.0) }).is_ok());
}

#[test]
fn bad_input_is_refused_by_name() {
    let p = params(ToolKind::TSlot, 26.0);
    let cases: [(UndercutParams, &str); 8] = [
        (UndercutParams { width: 18.0, ..p }, "narrower than the 20 mm cutter head"),
        (UndercutParams { run_in: 0.0, ..p }, "run-in must be positive"),
        (UndercutParams { stepover: 0.0, ..p }, "stepover must be positive"),
        (UndercutParams { feed: 0.0, ..p }, "feeds must be positive"),
        (UndercutParams { bulge: 1.5, ..p }, "more than a half circle"),
        (UndercutParams { clearance_z: -12.0, ..p }, "clearance plane must be above"),
        (UndercutParams { retract_z: Some(-20.0), ..p }, "retract plane must be between"),
        (UndercutParams { width: f64::NAN, ..p }, "must be finite"),
    ];
    for (bad, want) in cases {
        let w = why(undercut(A, B, &bad));
        assert!(w.contains(want), "{want}: {w}");
    }
    assert!(why(undercut(A, A, &p)).contains("endpoints must be distinct"));
    // A tight arc the inward shift would collapse.
    let tight = UndercutParams { bulge: 1.0, width: 26.0, ..p };
    let w = why(undercut(A, Point2::new(5.0, 0.0), &tight));
    assert!(w.contains("slot arc radius"), "{w}");
    // A stepover no ladder can lay is refused, never allocated.
    assert!(undercut(A, B, &UndercutParams { stepover: 1e-9, ..p }).is_err());
}

#[test]
fn conventional_swaps_the_side_of_the_first_widening() {
    let first = |climb: bool| {
        let t = undercut(A, B, &UndercutParams { climb, ..params(ToolKind::TSlot, 26.0) }).unwrap();
        t.moves.iter().find(|m| m.kind == MoveKind::Feed && m.to.y != 0.0).map(|m| m.to.y).unwrap()
    };
    assert!(first(true) > 0.0 && first(false) < 0.0);
}

#[test]
fn the_band_is_the_slot_outline() {
    let sq = undercut_band(A, B, 0.0, 13.0).unwrap();
    assert_eq!(sq, vec![Point2::new(0.0, 13.0), Point2::new(60.0, 13.0), Point2::new(60.0, -13.0), Point2::new(0.0, -13.0)]);
    // bulge 0.5: centre (30, 22.5), radius 37.5; the left edge is the inner one.
    let arc = undercut_band(A, B, 0.5, 3.0).unwrap();
    let r = |q: &Point2| (q.x - 30.0).hypot(q.y - 22.5);
    let n = arc.len() / 2;
    assert!(arc[..n].iter().all(|q| (r(q) - 34.5).abs() < 1e-9) && arc[n..].iter().all(|q| (r(q) - 40.5).abs() < 1e-9));
    assert!((arc[0].x - (30.0 - 34.5 * (30.0 / 37.5))).abs() < 1e-9, "{:?}", arc[0]);
    assert!(undercut_band(A, B, 0.0, 0.0).is_err() && undercut_band(A, Point2::new(5.0, 0.0), 1.0, 3.0).is_err());
}
