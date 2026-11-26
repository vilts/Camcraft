//! Pins of #3355 on a square boundary (a sketch 96 mm wide) over a
//! 70 x 70 stock with a 30 x 30 part in its middle, one level: Outside
//! cuts the open band from the air, Inside starts on the part wall, and
//! Auto ignores the stock outline (byte for byte). Ours.

use super::*;
use crate::cl::{MoveKind, Toolpath};
use crate::entry::Entry3D;
use crate::geo2d::{Loop, Point2};
use crate::geometry::Point3;
use crate::load::DEFAULT_OPTIMAL_LOAD;
use crate::machining2d::{CornerRelief, StartSide, pocket_started, pocket_started_on};

const R: f64 = 5.0;

fn square(s: f64, ccw: bool) -> Loop {
    let mut pts = vec![Point2::new(-s, -s), Point2::new(s, -s), Point2::new(s, s), Point2::new(-s, s)];
    if !ccw {
        pts.reverse();
    }
    let Ok(l) = Loop::new(pts) else { panic!("loop") };
    l
}

fn reg(loops: Vec<Loop>) -> Region {
    let Ok(r) = Region::new(loops) else { panic!("region") };
    r
}

fn params() -> PocketParams {
    PocketParams {
        blend_radius: None,
        tool_diameter: 2.0 * R,
        stepover: 4.0,
        stock_to_leave: 0.0,
        clearance_z: 40.0,
        retract_z: None,
        top_z: 10.0,
        bottom_z: 5.0,
        stepdown: 5.0,
        feed: 1200.0,
        plunge_feed: 300.0,
        climb: true,
        strategy: PocketStrategy::Adaptive,
        entry: Entry3D::Helix,
        helix_radius: 2.0,
        optimal_load: DEFAULT_OPTIMAL_LOAD,
        blend_turns: false,
        links: crate::links::Links::PLANAR,
    }
}

/// The band: the sketch less the part.
fn band() -> Region {
    reg(vec![square(48.0, true), square(15.0, false)])
}

fn stock() -> Region {
    reg(vec![square(35.0, true)])
}

fn cut(region: &Region, start: PassStart, stock: Option<&Region>) -> Toolpath {
    let side = StartSide { start, stock: stock.cloned(), around_part: false };
    match pocket_started_on(region, &params(), &[], &CornerRelief::default(), &side) {
        Ok(t) => t,
        Err(e) => panic!("{start:?}: {e}"),
    }
}

/// Where the first planar feed ends and where the last one ends.
fn first_last(t: &Toolpath) -> (Point3, Point3) {
    let mut planar = t.moves.iter().filter(|m| m.kind == MoveKind::Feed && (m.from.z - m.to.z).abs() < 1e-9);
    let Some(first) = planar.next().map(|m| m.to) else { panic!("no planar feed") };
    (first, planar.next_back().map_or(first, |m| m.to))
}

/// How far `p` stands outside the axis square of half-width `s`.
fn outside(p: Point3, s: f64) -> f64 {
    let (dx, dy) = ((p.x.abs() - s).max(0.0), (p.y.abs() - s).max(0.0));
    dx.hypot(dy)
}

#[test]
fn outside_cuts_the_band_from_the_air_and_ends_on_the_part_wall() {
    let t = cut(&band(), PassStart::Outside, Some(&stock()));
    assert!(t.notes.iter().any(|n| n.contains("air plan")), "the air plan cut it: {:?}", t.notes);
    let (first, last) = first_last(&t);
    assert!(outside(first, 35.0) >= R, "the first pass starts in the stock: {first:?}");
    assert!((outside(last, 15.0) - R).abs() < 0.05, "the level ends off the part wall: {last:?}");
}

#[test]
fn outside_without_a_stock_outline_rides_the_rings_from_the_outer_edge_said() {
    let t = cut(&band(), PassStart::Outside, None);
    assert!(t.notes.iter().any(|n| n.contains("Pass starts at Outside") && n.contains("no stock outline")), "{:?}", t.notes);
    let (first, last) = first_last(&t);
    assert!((first.x.abs().max(first.y.abs()) - (48.0 - R)).abs() < 0.05, "the first ring is off the outer edge: {first:?}");
    assert!((outside(last, 15.0) - R).abs() < 0.05, "the level ends off the part wall: {last:?}");
}

/// A closed pocket inside the stock has no air side: the growth spiral
/// would start in its core, so Outside rides the rings from its wall in.
#[test]
fn outside_on_a_closed_pocket_rides_the_rings_from_its_wall_said() {
    let pocket = reg(vec![square(30.0, true)]);
    let t = cut(&pocket, PassStart::Outside, Some(&stock()));
    assert!(t.notes.iter().any(|n| n.contains("Pass starts at Outside") && n.contains("in the air")), "{:?}", t.notes);
    let (first, _) = first_last(&t);
    assert!((first.x.abs().max(first.y.abs()) - (30.0 - R)).abs() < 0.05, "the first ring is off the pocket wall: {first:?}");
}

#[test]
fn inside_with_holes_starts_on_the_part_wall_said() {
    let t = cut(&band(), PassStart::Inside, Some(&stock()));
    assert!(t.notes.iter().any(|n| n.contains("Pass starts at Inside") && n.contains("hole wall")), "{:?}", t.notes);
    let (first, _) = first_last(&t);
    assert!((outside(first, 15.0) - R).abs() < 0.05, "the first ring is off the part wall: {first:?}");
}

#[test]
fn inside_without_holes_is_the_growth_plan_and_auto_ignores_the_stock() {
    let pocket = reg(vec![square(30.0, true)]);
    let auto = cut(&pocket, PassStart::Auto, None);
    assert_eq!(cut(&pocket, PassStart::Inside, Some(&stock())), auto, "Inside on a pocket is the growth plan");
    let Ok(old) = pocket_started(&band(), &params(), &[], &CornerRelief::default(), PassStart::Auto) else { panic!("auto") };
    assert_eq!(cut(&band(), PassStart::Auto, Some(&stock())), old, "Auto reads no stock outline");
}

#[test]
fn trochoidal_refuses_a_forced_start_by_name() {
    assert_eq!(trochoidal_pass_start_refusal(AdaptiveMethod::Spiral, PassStart::Outside), None);
    assert_eq!(trochoidal_pass_start_refusal(AdaptiveMethod::Trochoidal, PassStart::Auto), None);
    let Some(why) = trochoidal_pass_start_refusal(AdaptiveMethod::Trochoidal, PassStart::Inside) else { panic!("refused") };
    assert!(why.contains("Pass starts at Inside") && why.contains("Trochoidal"), "{why}");
}
