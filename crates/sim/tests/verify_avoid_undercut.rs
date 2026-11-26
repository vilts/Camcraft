//! #1444 with #2055: the undercut heads against a protected 'Do not
//! touch' item. A dovetail frustum and a lollipop ball are read as their
//! circumscribed cylinder, so the law can over-report, never under-report.
//! Provenance: plane geometry of the handbook cutter side views; our own
//! code, per the Clean-Room Protocol.
//!
//! Gate map: covers the undercut profiles in
//! camcraft_sim::verify::swept::rho_expanded and
//! camcraft_sim::verify::{avoid, avoid_poly}.

use camcraft_sim::cl::{MoveKind, PathBuilder, Toolpath};
use camcraft_sim::geo2d::Point2;
use camcraft_sim::geometry::Point3;
use camcraft_sim::verify::{AVOID_PAD, AVOID_VERIFY_TOL, AvoidItem, AvoidSpec, Cutter, FixturePart, Warning, avoid_hits};
use camcraft_tools::tools::{Tool, ToolKind};
use camcraft_tools::undercut::cutter;

/// Rim radius 10 at the tip, head 5 high, 60 degree flank.
fn dovetail() -> Tool {
    cutter(ToolKind::Dovetail, 20.0, 5.0, 8.0, 30.0, 60.0)
}

/// Ball radius 5, centre 5 above the tip.
fn lollipop() -> Tool {
    cutter(ToolKind::Lollipop, 10.0, 5.0, 4.0, 30.0, 0.0)
}

/// A protected edge along Y at `x`, height `z`.
fn y_edge(x: f64, z: f64) -> AvoidItem {
    AvoidItem { label: "edge".into(), tris: Vec::new(), edges: vec![([x, -1.0, z], [x, 1.0, z])] }
}

/// A feed along Y through the origin, tip at Z 0.
fn pass() -> Toolpath {
    let mut pb = PathBuilder::new(Point3::new(0.0, -2.0, 0.0));
    pb.push(MoveKind::Feed, Point3::new(0.0, 2.0, 0.0), Some(500.0));
    pb.finish()
}

fn hits(tool: &Tool, clearance: f64, x: f64, z: f64) -> Vec<Warning> {
    let spec = AvoidSpec { items: vec![y_edge(x, z)], cutter: Cutter::of(tool), clearance };
    avoid_hits(&pass(), &spec, AVOID_VERIFY_TOL).expect("verify runs")
}

#[test]
fn a_dovetail_rim_keeps_its_clearance_from_a_protected_item() {
    let rim = hits(&dovetail(), 1.5, 11.4, 0.0);
    assert!(matches!(rim[..], [Warning::AvoidHit { part: FixturePart::Flute, .. }]), "rim: {rim:?}");
    assert!(hits(&dovetail(), 1.5, 11.6, 0.0).is_empty(), "clear of the rim and the clearance");
    // 1 mm below the tip plane the clearance ball of the rim reaches 1.118 mm out.
    assert_eq!(hits(&dovetail(), 1.5, 11.05, -1.0).len(), 1, "below the rim");
    assert!(hits(&dovetail(), 1.5, 11.2, -1.0).is_empty(), "clear below the rim");
    // The head's top face (Z 5, radius 7.11) 0.3 mm under a protected
    // item, 2 mm outside the neck: the cutting top gets no allowance here.
    assert_eq!(hits(&dovetail(), 0.5, 6.0, 5.3).len(), 1, "over the head's top face");
}

#[test]
fn a_lollipop_ball_keeps_its_clearance_from_a_protected_item() {
    let eq = hits(&lollipop(), 1.0, 5.9, 5.0);
    assert!(matches!(eq[..], [Warning::AvoidHit { part: FixturePart::Flute, .. }]), "equator: {eq:?}");
    assert!(hits(&lollipop(), 1.0, 6.1, 5.0).is_empty(), "clear of the equator and the clearance");
    assert_eq!(hits(&lollipop(), 1.0, 0.0, -0.5).len(), 1, "under the pole");
    // The upper cap, above the neck start (Z 5) and outside the 2 mm neck.
    assert_eq!(hits(&lollipop(), 0.0, 3.5, 6.5).len(), 1, "the upper cap");
    // The top of the ball touches a protected item: no cap allowance.
    assert_eq!(hits(&lollipop(), 0.5, 0.0, 10.2).len(), 1, "over the pole");
}

/// The generator keep-out covers the rim at each tip height of the Z
/// range, and the clearance ball below the lowest tip height.
#[test]
fn a_dovetail_keep_out_covers_the_rim_over_the_z_range() {
    let keep = |z: f64| {
        let spec = AvoidSpec { items: vec![y_edge(0.0, z)], cutter: Cutter::of(&dovetail()), clearance: 1.5 };
        spec.keepout_region(0.0, 8.0, AVOID_PAD).expect("keepout")
    };
    // The rim passes height 7 when the tip is at 7: the head top is not the rim.
    assert!(keep(7.0).contains(Point2::new(-11.4, 0.0)), "rim inside the Z range");
    // Below the range: 10 + sqrt(1.52^2 - 1) = 11.145.
    assert!(keep(-1.0).contains(Point2::new(-11.0, 0.0)), "clearance ball below the range");
    assert!(!keep(-1.0).contains(Point2::new(-11.6, 0.0)), "no keep-out past the ball");
}
