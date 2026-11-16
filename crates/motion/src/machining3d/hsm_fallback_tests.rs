//! Issue #983: every level the HSM planner hands to the Offset rings is
//! named in the notes - on the rotary wrap by its developed radius, on
//! the plane by its Z - with the planner's reason. Our own regression;
//! no vendor material consulted.

use super::{Seam, plan_level, rings_tag};
use crate::cl::PathBuilder;
use crate::entry::Entry3D;
use crate::geo2d::{Loop, Point2, Region};
use crate::geometry::Point3;
use crate::machining2d::PocketStrategy;
use crate::machining3d::Pocket3DParams;

/// A 4 mm channel round a 70 x 60 core (adaptive_band's user band
/// less the cutter's reach): a center area the planner refuses up front
/// as too narrow and long for one spiral.
fn channel() -> Region {
    let pt = |x: f64, y: f64| Point2::new(x, y);
    let Ok(outer) = Loop::new(vec![pt(-9.0, -9.0), pt(69.0, -9.0), pt(69.0, 59.0), pt(-9.0, 59.0)]) else { panic!("outer") };
    let Ok(hole) = Loop::new(vec![pt(-5.0, -5.0), pt(-5.0, 55.0), pt(65.0, 55.0), pt(65.0, -5.0)]) else { panic!("hole") };
    let Ok(r) = Region::new(vec![outer, hole]) else { panic!("channel") };
    r
}

fn params(z: f64) -> Pocket3DParams {
    Pocket3DParams {
        tool_diameter: 10.0,
        stepover: 4.0,
        stock_to_leave: 0.0,
        clearance_z: z + 5.0,
        top_z: z,
        bottom_z: z,
        stepdown: 1.0,
        feed: 1000.0,
        plunge_feed: 300.0,
        climb: true,
        strategy: PocketStrategy::Adaptive,
        retract_z: None,
        entry: Entry3D::Plunge,
        helix_radius: 1.0,
        optimal_load: crate::load::DEFAULT_OPTIMAL_LOAD,
        links: crate::links::Links::ROUGH3D,
    }
}

/// The level's notes and whether the HSM plan planned it.
fn plan(z: f64, seam: Option<&Seam>) -> (Vec<String>, bool, usize) {
    let mut b = PathBuilder::new(Point3::new(0.0, 0.0, z + 5.0));
    let lp = match plan_level(&mut b, &channel(), &params(z), z, z + 1.0, false, false, seam) {
        Ok(lp) => lp,
        Err(e) => panic!("the level is cut: {e}"),
    };
    (b.finish().notes, lp.hsm, lp.passes.len())
}

/// A developed level (the seam far off, so only the planner decides)
/// the planner refuses is cut by rings, named by its radius and why.
#[test]
fn a_developed_level_the_planner_refuses_is_named_by_its_radius() {
    let sm = Seam { lo: -1000.0, hi: 1000.0, period: 1.0e6, reach: 0.0 };
    let (notes, hsm, passes) = plan(5.6, Some(&sm));
    assert!(!hsm && passes > 0, "rings cut the level: {notes:?}");
    let tag = rings_tag("the developed level at radius 5.600");
    assert!(notes.iter().any(|n| n.ends_with(&tag) && n.contains("channel")), "named with its reason: {notes:?}");
}

/// The planar ladder's level, refused the same way, is named by its Z.
#[test]
fn a_planar_level_the_planner_refuses_is_named_by_its_z() {
    let (notes, hsm, passes) = plan(-2.0, None);
    assert!(!hsm && passes > 0, "rings cut the level: {notes:?}");
    let tag = rings_tag("the level at Z -2.000");
    assert!(notes.iter().any(|n| n.ends_with(&tag) && n.contains("channel")), "named with its reason: {notes:?}");
}

/// A developed level the wrap's seam cuts goes through the same door.
#[test]
fn a_developed_level_the_seam_cuts_is_named_by_its_radius() {
    let sm = Seam { lo: -10.0, hi: 200.0, period: 400.0, reach: 7.0 };
    let (notes, hsm, _) = plan(6.0, Some(&sm));
    assert!(!hsm, "rings cut the level: {notes:?}");
    let tag = rings_tag("the developed level at radius 6.000");
    assert!(notes.iter().any(|n| n.starts_with(super::SEAM_RINGS) && n.ends_with(&tag)), "{notes:?}");
}
