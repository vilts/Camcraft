//! Issue #983: every level the HSM planner hands to the Offset rings is
//! named in the notes - on the rotary wrap by its developed radius, on
//! the plane by its Z - with the planner's reason. Our own regression;
//! no vendor material consulted.

use super::{Seam, plan_level, rings_tag};
use crate::cl::{MoveKind, PathBuilder};
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
        pass_start: Default::default(),
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
    let lp = match plan_level(&mut b, &channel(), &params(z), z, z + 1.0, false, false, seam, None) {
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

/// Issue #3035: the refused level counts as one fallback, so the op can
/// raise its warning; a level the HSM plan keeps counts none.
#[test]
fn a_refused_level_counts_as_one_rings_fallback() {
    let (notes, hsm, _) = plan(-2.0, None);
    assert!(!hsm, "{notes:?}");
    assert_eq!(crate::machining2d::rings_fallbacks(&notes), 1, "{notes:?}");
    let kept = ["adaptive: the level is entered by a plunge in the air outside the stock".to_owned()];
    assert_eq!(crate::machining2d::rings_fallbacks(&kept), 0);
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

/// Issue #2468: the air-entry note must follow the tool entry law.
#[test]
fn air_entry_note_matches_center_cutting_tool() {
    let pt = |x, y| Point2::new(x, y);
    let stock = Region::new(vec![Loop::new(vec![
        pt(-10.0, -10.0), pt(10.0, -10.0), pt(10.0, 10.0), pt(-10.0, 10.0),
    ]).expect("stock loop")]).expect("stock region");
    let center = stock.offset(5.01).expect("air band for the tool radius");
    for center_cutting in [true, false] {
        let mut p = params(-2.0);
        p.links.approach.center_cutting = center_cutting;
        let mut b = PathBuilder::approached(Point3::new(0.0, 0.0, p.clearance_z));
        let lp = plan_level(&mut b, &center, &p, -2.0, 1.0, false, false, None, Some(&stock))
            .expect("air-side HSM plan");
        assert!(lp.hsm && lp.air_entry, "fixture must plan an air entry");
        b.push(MoveKind::Plunge, Point3::new(0.0, 0.0, -2.0), Some(p.plunge_feed));
        let path = b.finish();
        assert_eq!(path.moves[0].kind, MoveKind::Rapid, "air-entry fixture must obey the approach law");
        assert_eq!(path.moves[0].from, Point3::new(0.0, 0.0, p.clearance_z));
        assert_eq!(path.moves[0].to, Point3::new(0.0, 0.0, p.clearance_z));
        assert!(path.is_chained());
        let notes = path.notes;
        assert_eq!(notes.iter().any(|n| n == super::AIR_ENTRY), center_cutting,
            "air-entry note must match the entry law for center_cutting={center_cutting}: {notes:?}");
    }
}
