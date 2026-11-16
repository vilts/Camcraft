//! #1313: the mid-turn sweep (law c3) reads the whole turn, not five
//! poses of it. A holder far up a long stack swings through a tall
//! post of uncut stock BETWEEN two of the poses the law used to sample
//! (20 and 40 degrees of an 80-degree turn): each sampled column stood
//! clear of the post, the band itself went through it.
//!
//! Provenance: our own fixture; the law is the Prime Directive's
//! ("never gouge, never collide"). No vendor material consulted.
//!
//! Gate map (#1313): covers camcraft_sim::verify (sweep, turn_cells,
//! reach).

use camcraft_sim::cl::{angles_to_axis, Move, MoveKind, Toolpath};
use camcraft_sim::geo2d::{Loop, Point2, Region};
use camcraft_sim::geometry::Point3;
use camcraft_sim::verify::{verify, Mode, ToolGeom, VerifyContext, Warning};

fn rect(x0: f64, y0: f64, x1: f64, y1: f64) -> Region {
    let pts = vec![Point2::new(x0, y0), Point2::new(x1, y0), Point2::new(x1, y1), Point2::new(x0, y1)];
    match Loop::new(pts).and_then(|l| Region::new(vec![l])) {
        Ok(r) => r,
        Err(e) => panic!("region: {e:?}"),
    }
}

fn path(moves: Vec<Move>) -> Toolpath {
    Toolpath { moves, cycles: Vec::new(), notes: Vec::new(), comps: Vec::new(), threads: Vec::new(), probes: Vec::new(), breaks: Vec::new(), plasma: None }
}

/// A slim 6 mm cutter with 100 mm of flute and no shank band: the one
/// band the reach laws judge is the holder (r 4, 4 long), standing
/// 100 mm above the tip.
fn long_reach() -> ToolGeom {
    ToolGeom { radius: 3.0, nose_radius: 0.0, flute_len: 100.0, shank_radius: 3.0, shank_len: 0.0, holder_radius: 4.0, holder_len: 4.0 }
}

/// The band stack's reports for a one-move path turning from home to
/// B `to_deg` while the tip sinks from z -95 to z -100 at the origin,
/// beside a post of uncut stock standing at x 48..52 up to z 0.
fn swing(to_deg: f64) -> Vec<Warning> {
    let post = rect(48.0, -10.0, 52.0, 10.0);
    let tool = long_reach();
    let cx = VerifyContext {
        region: &post,
        stock: Some(&post),
        tool: &tool,
        mode: Mode::Area,
        stock_top_z: 0.0,
        clearance_z: 10.0,
        gouge_tol: 0.01,
        z_tol: 1e-6,
        ..Default::default()
    };
    let mut m = Move::new(MoveKind::Feed, Point3::new(0.0, 0.0, -95.0), Point3::new(0.0, 0.0, -100.0), Some(300.0));
    m.axis = angles_to_axis([0.0, to_deg, 0.0]);
    assert!(m.axis.is_some());
    verify(&path(vec![m]), &cx)
        .into_iter()
        .filter(|w| matches!(w, Warning::ShankHit { .. } | Warning::SweepHit { .. }))
        .collect()
}

#[test]
fn a_holder_crossing_a_stock_post_between_two_sampled_poses_is_convicted() {
    // At B 29.4 the holder (102 mm up the axis) stands over x 50, its
    // bottom near z -11 - through the post. The poses the law sampled:
    // B 0 (over the tip), 20 (x 35), 40 (x 66), 60, 80 - none over
    // x 48..52. The end pose (law c) is far past the post and clear.
    let w = swing(80.0);
    assert_eq!(w.len(), 1, "one report for the one move: {w:?}");
    assert!(matches!(w[0], Warning::SweepHit { at: 0, .. }), "{w:?}");
}

#[test]
fn a_turn_that_stops_short_of_the_post_stays_clean() {
    // B 20: the holder's column reaches x 38.6 at most - ten
    // millimetres short. The wider reading convicts no clear swing.
    let w = swing(20.0);
    assert!(w.is_empty(), "{w:?}");
}
