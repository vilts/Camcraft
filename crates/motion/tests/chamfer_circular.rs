//! The M2.4 B kernels' own laws, stated where they live: the chamfer
//! ride posts exactly the corner depth `top - width * tan(angle)` -
//! the derived floor is a computation, never an input - and the
//! circular's rings nest ring-major, standing at or above the bowl
//! everywhere the vault runs. The congruence doors that need a Tool
//! stand in the app; the doors these two kernels can state alone
//! (their numbers) refuse by message here.
//!
//! Gate map (M4.4): covers camcraft_motion::cl, camcraft_motion::geo2d,
//! camcraft_motion::machining2d (chamfer, circular).

use camcraft_motion::cl::MoveKind;
use camcraft_motion::entry::Entry3D;
use camcraft_motion::geo2d::{Loop, Point2, Region};
use camcraft_motion::machining2d::{
    chamfer, circular, ChamferParams, CircularParams, GenError,
};

fn rect(x0: f64, y0: f64, x1: f64, y1: f64) -> Region {
    Region::new(vec![Loop::new(vec![
        Point2::new(x0, y0),
        Point2::new(x1, y0),
        Point2::new(x1, y1),
        Point2::new(x0, y1),
    ])
    .unwrap()])
    .unwrap()
}

fn chamfer_params() -> ChamferParams {
    ChamferParams {
        width: 2.0,
        angle_deg: 45.0,
        clearance_z: 25.0,
        retract_z: None,
        top_z: 20.0,
        feed: 500.0,
        plunge_feed: 100.0,
        climb: true,
        inside: false,
    }
}

fn circular_params(vault: bool, stepover: f64, radius: f64) -> CircularParams {
    CircularParams {
        links: camcraft_motion::links::Links::PLANAR,
        center: Point2::new(0.0, 0.0),
        radius,
        vault,
        bottom_z: 10.0,
        top_z: 20.0,
        tool_diameter: 8.0,
        stock_to_leave: 0.0,
        stepover,
        stepdown: 5.0,
        clearance_z: 30.0,
        retract_z: None,
        feed: 800.0,
        plunge_feed: 200.0,
        climb: true,
        entry: Entry3D::Plunge,
        helix_radius: 1.0,
    }
}

fn err<T>(r: Result<T, GenError>) -> String {
    match r {
        Ok(_) => panic!("a door was meant to refuse"),
        Err(e) => e.to_string(),
    }
}

#[test]
fn chamfer_rides_the_derived_corner() {
    // The one level the ride owns: every feed move stands at
    // top - width * tan(angle), the law's number, not a carried one.
    let p = chamfer_params();
    let t = chamfer_path(&p);
    let z = 20.0 - 2.0 * 45f64.to_radians().tan();
    assert!((p.corner_z() - z).abs() < 1e-12, "corner_z states the law");
    let mut circuits = 0usize;
    for m in &t.moves {
        if m.kind == MoveKind::Feed {
            assert!((m.from.z - z).abs() < 1e-12 && (m.to.z - z).abs() < 1e-12, "a feed move off the corner: {m:?}");
            circuits += 1;
        }
    }
    // One square loop, closed with its start repeated: 4 chords.
    assert_eq!(circuits, 4, "one closed circuit of four chords per picked loop");
    assert!(t.is_chained() && t.all_finite());
    assert_eq!(t.moves.last().unwrap().kind, MoveKind::Rapid);
    assert_eq!(t.moves.last().unwrap().to.z, 25.0, "the ride ends by lifting to clearance");
}

fn chamfer_path(p: &ChamferParams) -> camcraft_motion::cl::Toolpath {
    chamfer(&rect(0.0, 0.0, 60.0, 60.0), p).expect("a well-made ride")
}

#[test]
fn the_corner_depth_is_not_45_only() {
    // The tan law for a slope the shop also cuts: 3 wide at 30
    // degrees stands 3 * tan(30) under the top plane.
    let mut p = chamfer_params();
    p.width = 3.0;
    p.angle_deg = 30.0;
    let z = 20.0 - 3.0 * 30f64.to_radians().tan();
    let t = chamfer_path(&p);
    assert!((z - p.corner_z()).abs() < 1e-12);
    assert!(t.moves.iter().filter(|m| m.kind == MoveKind::Feed).all(|m| (m.to.z - z).abs() < 1e-12));
}

#[test]
fn the_chamfer_declares_the_break_it_was_asked_to_cut() {
    // #303: the ride states its break beside the moves - the picked
    // loop at the top plane, the face's depth and slope from width and
    // angle - so the program sim judges the cut against the promise.
    let mut p = chamfer_params();
    p.width = 3.0;
    p.angle_deg = 30.0;
    let t = chamfer_path(&p);
    assert_eq!(t.breaks.len(), 1, "one picked loop, one break: {:?}", t.breaks);
    let b = &t.breaks[0];
    let tan = 30f64.to_radians().tan();
    assert!((b.depth - 3.0 * tan).abs() < 1e-12 && (b.slope - tan).abs() < 1e-12, "{b:?}");
    assert!((b.width() - 3.0).abs() < 1e-9, "the bevel meets the top plane at the stated width");
    assert!(b.edge.iter().all(|q| q.z == 20.0), "the edge stands at the top plane: {b:?}");
    assert_eq!(b.edge.first(), b.edge.last(), "the loop closes on its start");
    // The bevel itself: at the wall it is the corner depth, `width` in
    // it is the top plane, and past the width nothing is declared.
    let (wall, _) = b.floor_at(0.0, 30.0).expect("on the wall");
    assert!((wall - p.corner_z()).abs() < 1e-9, "{wall}");
    let (mid, edge_z) = b.floor_at(1.5, 30.0).expect("inside the width");
    assert!((mid - (p.corner_z() + 1.5 * tan)).abs() < 1e-9 && edge_z == 20.0, "{mid}");
    assert!(b.floor_at(3.5, 30.0).is_none(), "past the stated width nothing is the op's");
}

#[test]
fn chamfer_refuses_its_own_numbers_by_name() {
    // The kernel's doors - the four the Tool cannot be blamed for.
    let mut p = chamfer_params();
    p.width = 0.0;
    assert!(err(chamfer(&rect(0.0, 0.0, 1.0, 1.0), &p)).contains("chamfer width must be positive"));
    p = chamfer_params();
    p.angle_deg = 90.0;
    assert!(err(chamfer(&rect(0.0, 0.0, 1.0, 1.0), &p)).contains("between 0 and 90 degrees"));
    p = chamfer_params();
    p.clearance_z = 15.0;
    assert!(err(chamfer(&rect(0.0, 0.0, 1.0, 1.0), &p)).contains("clearance plane below the chamfer's top plane"));
    p = chamfer_params();
    p.retract_z = Some(15.0);
    assert!(
        err(chamfer(&rect(0.0, 0.0, 1.0, 1.0), &p)).contains("retract plane sits inside the material"),
        "between the corner and the top the wall still stands"
    );
}

#[test]
fn flat_rings_step_inward_ring_major() {
    // 25 R with a 8 mm cutter: the first ring rides 21, and each
    // next steps in by the authored 3, to the last one that still
    // stands off the axis; every ring runs the whole ladder before
    // its successor starts (ring-major - a disk's rings cannot
    // interfere, so the order is free and the test can SEE it).
    let p = circular_params(false, 3.0, 25.0);
    let t = circular(&rect(-1.0, -1.0, 1.0, 1.0), &p).expect("a wide-enough circle");
    let mut seen: Vec<f64> = Vec::new();
    for m in &t.moves {
        if m.kind != MoveKind::Feed {
            continue;
        }
        let rho = (m.to.x.hypot(m.to.y)).round();
        if seen.last() != Some(&rho) {
            seen.push(rho);
        }
        assert!([20.0, 15.0, 10.0].iter().any(|z| (m.to.z - z).abs() < 1e-12), "flat ladder levels only: {m:?}");
    }
    assert_eq!(seen, vec![21.0, 18.0, 15.0, 12.0, 9.0, 6.0, 3.0], "rings step in, never back out");
    assert!(t.is_chained() && t.all_finite());
}

#[test]
fn the_vault_never_stands_below_its_own_bowl() {
    // The never-gouge law: the ring at `rho` cuts at planes at or
    // above the bowl at its band's outer edge, which is at or above
    // the bowl under the cutter center it rides - so no feed move
    // dips below `bowl_z` of its own column.
    let p = circular_params(true, 3.0, 25.0);
    let t = circular(&rect(-1.0, -1.0, 1.0, 1.0), &p).expect("a valid dome ride");
    let mut last_floor = f64::MAX;
    for m in &t.moves {
        if m.kind != MoveKind::Feed {
            continue;
        }
        let rho = m.to.x.hypot(m.to.y);
        assert!(m.to.z >= p.bowl_z(rho) - 1e-9, "a cut below the bowl at rho {rho}: {m:?}");
        if m.to.z < last_floor - 1e-12 {
            last_floor = m.to.z;
        }
    }
    // The deepest plane is the last ring's floor: the bowl at its
    // band's outer edge, rho_min + r - and it stands ABOVE the
    // dome's own floor at the axis, honestly (the scallop's truth).
    let rho_min = 3.0f64;
    let want = p.bowl_z((rho_min + 4.0).min(25.0));
    assert!((last_floor - want).abs() < 1e-9, "deepest plane {last_floor}, bowl at band edge {want}");
    assert!(last_floor > 10.0, "the ring ladder stops above the axis floor - the rest scallop, stated not hidden");
}

#[test]
fn circular_refuses_no_room_and_the_uncut_core() {
    // The two reach doors the disk states alone.
    let mut p = circular_params(false, 5.0, 25.0);
    assert!(
        err(circular(&rect(-1.0, -1.0, 1.0, 1.0), &p))
            .contains("stepover must stay within the cutter radius - the last ring could leave an uncut core"),
        "5 mm of step on a 8 mm cutter is past the radius"
    );
    p = circular_params(false, 3.0, 3.0);
    assert!(err(circular(&rect(-1.0, -1.0, 1.0, 1.0), &p)).contains("the cutter has no room inside the circle"));
    // Leave a wall so thick that even the rim ring has no center to
    // ride: the same door, its other number.
    p = circular_params(false, 3.0, 6.0);
    p.stock_to_leave = 2.5;
    assert!(err(circular(&rect(-1.0, -1.0, 1.0, 1.0), &p)).contains("the cutter has no room inside the circle"));
}
