//! Round 4's laws (Gate map (M11.4 C): covers
//! motion::machining2d::adaptive::{lift, lift_link, lift_split, lift_swept, route,
//! star::wide_enough}): a microlift return is legal only over cleared
//! stock, inside the area, off the orbit, never crossing another return
//! on its plane - and a crossing one climbs to the next lift; the width
//! test admits the star path only 1.4 D wide; a piece takes longer
//! chords only where they leave no sliver (`lift_coarse`, issue #982),
//! and the lifted plan is kept only at or under the spiral's reading.

use super::lift::{LiftSpec, keeps};
use super::lift_coarse::cut;
use super::lift_link::{LinkLaw, Return, legs};
use super::lift_split::{Piece, split, thin_turn};
use super::raster::Raster;
use super::star::{STAR_WIDTH_PER_R, wide_enough};
use crate::geo2d::{Loop, Point2, Region};

fn rect(x0: f64, y0: f64, x1: f64, y1: f64) -> Region {
    let pts = vec![Point2::new(x0, y0), Point2::new(x1, y0), Point2::new(x1, y1), Point2::new(x0, y1)];
    let Ok(l) = Loop::new(pts) else { panic!("loop") };
    let Ok(r) = Region::new(vec![l]) else { panic!("region") };
    r
}

/// A 60 x 40 block of stock with a 50 mm channel cleared along y = 20
/// (a D10 tool walked down it), and its center area.
fn channel() -> (Raster, Region) {
    let reach = rect(0.0, 0.0, 60.0, 40.0);
    let Ok(mut ras) = Raster::from_region(&reach, 0.1, 5.0) else { panic!("raster") };
    let rim = crate::quality::rim_for(10.0, 0.1);
    ras.walk(&[Point2::new(5.0, 20.0), Point2::new(55.0, 20.0)], 5.0, rim, 0.5);
    ras.walk(&[Point2::new(55.0, 22.0), Point2::new(5.0, 22.0)], 5.0, rim, 0.5);
    (ras, rect(5.0, 5.0, 55.0, 35.0))
}

/// No swept stock: these laws are the cells' (`lift_swept` has its own).
static NO_SWEEP: std::sync::LazyLock<super::lift_swept::Swept> = std::sync::LazyLock::new(|| super::lift_swept::Swept::new(5.0));

fn law<'a>(ras: &'a Raster, center: &'a Region, links: &'a [Vec<super::lift_link::Leg>], orbit: (Point2, f64)) -> LinkLaw<'a> {
    let rim = crate::quality::rim_for(10.0, 0.1);
    LinkLaw { ras, center, links, swept: &NO_SWEEP, r: 5.0, rim, step: 0.5, bite: crate::load::deg_at_rim(0.005, 10.0, rim), orbit }
}

fn ret(lift: f64, down: Point2) -> Return {
    Return { lift, high: false, up: Vec::new(), via: Vec::new(), down: vec![down] }
}

#[test]
fn a_microlift_over_cleared_stock_is_legal_and_one_over_standing_stock_is_not() {
    let (ras, center) = channel();
    let none = (Point2::new(0.0, 0.0), 0.0);
    let l = law(&ras, &center, &[], none);
    // Along the cleared channel: legal.
    assert!(l.legal(Point2::new(10.0, 21.0), &ret(0.3, Point2::new(40.0, 21.0))));
    // Out of it, across standing stock: refused.
    assert!(!l.legal(Point2::new(10.0, 21.0), &ret(0.3, Point2::new(40.0, 30.0))));
    // Out of the center area (the tool would leave the pocket): refused.
    assert!(!l.legal(Point2::new(10.0, 21.0), &ret(0.3, Point2::new(-2.0, 21.0))));
    // Across the entry orbit's circle: refused (the next level's helix
    // sweeps it at the return's heights).
    let orbit = law(&ras, &center, &[], (Point2::new(25.0, 21.0), 1.5));
    assert!(!orbit.legal(Point2::new(10.0, 21.0), &ret(0.3, Point2::new(40.0, 21.0))));
}

#[test]
fn a_return_crossing_another_on_its_plane_is_refused_and_climbs_over_it() {
    let (ras, center) = channel();
    let none = (Point2::new(0.0, 0.0), 0.0);
    let first = legs(Point2::new(10.0, 20.5), &ret(0.3, Point2::new(40.0, 21.5)));
    let links = vec![first];
    let l = law(&ras, &center, &links, none);
    let (a, b) = (Point2::new(10.0, 21.5), Point2::new(40.0, 20.5));
    assert!(!l.legal(a, &ret(0.3, b)), "the X on one plane is the verifier's crossing");
    assert!(l.legal(a, &ret(0.6, b)), "at twice the lift it passes over");
    // The posted legs climb, cross at the lift and land on the plane.
    let lg = legs(a, &ret(0.6, b));
    assert_eq!(lg.first().map(|g| g.2), Some(0.0));
    assert_eq!(lg.last().map(|g| g.3), Some(0.0));
    assert!(lg.iter().any(|g| g.2 == 0.6 && g.3 == 0.6), "a planar traverse at the lift: {lg:?}");
}

#[test]
fn a_walk_splits_at_its_long_air_only_and_its_pieces_keep_their_leads() {
    // Marks: bite, 20 mm air, bite, 2 mm air, bite.
    let walk: Vec<Point2> = (0..=60).map(|k| Point2::new(k as f64, 0.0)).collect();
    let marks: Vec<f64> = (0..60).map(|k| if (10..30).contains(&k) || (40..42).contains(&k) { 0.0 } else { 40.0 }).collect();
    let pieces: Vec<Piece> = split(&walk, &marks, (5.0, 10.0), true);
    assert_eq!(pieces.len(), 2, "the 2 mm air is ridden, the 20 mm air dropped");
    assert_eq!(pieces[0].pts[0], walk[0], "anchored: the first piece starts where the walk does");
    let gap_end = pieces[1].pts[0];
    assert!((gap_end.x - 29.5).abs() < 1e-9, "the next piece leaves its pad early: {gap_end:?}");
    assert!(pieces[1].lead.last().is_some_and(|q| (q.x - 29.5).abs() < 1e-9), "its lead ends on it");
    assert!(pieces[0].tail.first().is_some_and(|q| (q.x - 10.5).abs() < 1e-9), "the first runs its pad on");
}

#[test]
fn the_turn_thinning_holds_four_degrees_a_joint() {
    let arc: Vec<Point2> = (0..=400).map(|k| {
        let a = k as f64 * std::f64::consts::PI / 400.0;
        Point2::new(2.0 * a.cos(), 2.0 * a.sin())
    }).collect();
    let thin = thin_turn(&arc, 4f64.to_radians(), 0.01);
    assert!(thin.len() < arc.len() / 4, "{} chords", thin.len());
    for w in thin.windows(3) {
        let (u, v) = ((w[1].x - w[0].x, w[1].y - w[0].y), (w[2].x - w[1].x, w[2].y - w[1].y));
        let turn = (u.0 * v.1 - u.1 * v.0).atan2(u.0 * v.0 + u.1 * v.1).abs().to_degrees();
        assert!(turn <= 4.0 + 1e-9, "a {turn} degree joint");
    }
}

#[test]
fn the_star_path_admits_only_areas_one_point_four_d_wide() {
    let half = 0.5 * STAR_WIDTH_PER_R * 5.0;
    // Center areas of a D10: a 22 x 60 channel (12 wide) is refused, the
    // 60 x 40 rectangle (30 wide) and the user's R12.5 bore (15) admitted.
    let narrow = rect(5.0, 5.0, 55.0, 17.0);
    assert!(matches!(wide_enough(&narrow, half), Ok(false)));
    let wide = rect(5.0, 5.0, 55.0, 35.0);
    assert!(matches!(wide_enough(&wide, half), Ok(true)));
    let disc: Vec<Point2> = (0..256).map(|k| {
        let a = k as f64 * std::f64::consts::TAU / 256.0;
        Point2::new(7.5 * a.cos(), 7.5 * a.sin())
    }).collect();
    let Ok(l) = Loop::new(disc) else { panic!("disc") };
    let Ok(bore) = Region::new(vec![l]) else { panic!("bore") };
    assert!(matches!(wide_enough(&bore, half), Ok(true)));
    // A wide room with a 12 mm stub: the stub's far end is out of reach
    // of the opening - refused.
    let Ok(stub) = rect(5.0, 5.0, 35.0, 35.0).union(&rect(30.0, 14.0, 60.0, 26.0)) else { panic!("union") };
    assert!(matches!(wide_enough(&stub, half), Ok(false)));
}

/// A D10 biting 1 mm into the stock round a cleared R16 disc, along a
/// quarter of the R12 circle: its own chords (one degree each) and what
/// reads them.
fn quarter_bite() -> (Raster, Vec<Point2>, LiftSpec) {
    let Ok(mut ras) = Raster::from_region(&rect(0.0, 0.0, 60.0, 40.0), 0.1, 5.0) else { panic!("raster") };
    let rim = crate::quality::rim_for(10.0, 0.1);
    let ring = |rad: f64, a0: usize, a1: usize| -> Vec<Point2> {
        (a0..=a1).map(|k| (k as f64).to_radians()).map(|a| Point2::new(30.0 + rad * a.cos(), 20.0 + rad * a.sin())).collect()
    };
    ras.walk(&ring(11.0, 0, 360), 5.0, rim, 0.5);
    let spec = LiftSpec { r: 5.0, rim, step: 0.5, bite: crate::load::deg_at_rim(0.005, 10.0, rim), bound: 90.0, laps: (0, 0), orbit: (Point2::new(0.0, 0.0), 0.0) };
    (ras, ring(12.0, 200, 290), spec)
}

/// Issue #982: longer chords cut inside the arc they replace and leave a
/// sliver of stock on its outside, which the next piece would meet as
/// engagement the spiral never planned. Such a piece is cut on its own
/// chords, and the stock is what they leave; chords that leave nothing
/// (a straight run's) are kept, and so are the moves they save.
#[test]
fn a_piece_takes_longer_chords_only_where_they_leave_no_sliver() {
    let (ras0, fine, spec) = quarter_bite();
    let coarse = thin_turn(&fine, 4f64.to_radians(), 0.01);
    assert!(coarse.len() * 3 < fine.len(), "{} chords for {}", coarse.len(), fine.len());
    // What each version leaves: the longer chords stand cells over.
    let (mut own, mut long) = (ras0.clone(), ras0.clone());
    own.walk(&fine, spec.r, spec.rim, spec.step);
    let w = long.walk(&coarse, spec.r, spec.rim, spec.step);
    assert!(w.0 <= spec.bound && long.stands_over(&own), "the fixture's longer chords read {} and leave no sliver", w.0);
    let mut ras = ras0.clone();
    let mut p = Piece { pts: coarse, lead: Vec::new(), tail: Vec::new(), fine: Some(fine.clone()) };
    cut(&mut ras, &mut p, &spec);
    assert_eq!(p.pts.len(), fine.len(), "the piece is cut on its own chords");
    assert!(!ras.stands_over(&own) && !own.stands_over(&ras), "and leaves exactly what they leave");
    // A straight run a millimetre into a channel's side: two points cut
    // what its forty-one do.
    let (mut ras, _) = channel();
    let run: Vec<Point2> = (0..=40).map(|k| Point2::new(10.0 + k as f64, 23.0)).collect();
    let mut q = Piece { pts: vec![run[0], run[40]], lead: Vec::new(), tail: Vec::new(), fine: Some(run) };
    let w = cut(&mut ras, &mut q, &spec);
    assert!(w.0 > spec.bite && w.0 <= spec.bound, "the run bites: {}", w.0);
    assert_eq!(q.pts.len(), 2, "chords that leave no sliver are kept");
    // Over the bound the longer chords were never taken (round 4's law).
    let (mut ras, tight) = (ras0, LiftSpec { bound: 1.0, ..spec });
    let mut p = Piece { pts: thin_turn(&fine, 4f64.to_radians(), 0.01), lead: Vec::new(), tail: Vec::new(), fine: Some(fine.clone()) };
    cut(&mut ras, &mut p, &tight);
    assert_eq!(p.pts.len(), fine.len());
}

/// Issue #982: the acceptance has no tolerance. One rim sample (half a
/// degree) over the spiral's own reading - or over the bound, where the
/// spiral read under it - drops the lifted plan; the spiral's own named
/// overload is the most a lifted plan may read.
#[test]
fn the_lifted_plan_is_kept_only_at_or_under_the_spiral_reading() {
    let bound = 47.5014;
    assert!(keeps(47.5, 47.5, bound), "the spiral's own reading");
    assert!(keeps(47.5, 40.0, bound), "under the bound, over a lighter spiral");
    assert!(!keeps(48.0, 47.5, bound), "one sample over the bound");
    assert!(keeps(49.0, 49.0, bound), "the spiral's own named overload");
    assert!(!keeps(49.5, 49.0, bound), "one sample over that overload");
}

/// Issue #1169: a piece starting where the tool stands (the growth's
/// last piece handing over to the first easing lap) is ridden on to at
/// the floor - no retract and plunge on one spot; one starting anywhere
/// else, with no air tail leading to it, is not.
#[test]
fn a_piece_starting_where_the_tool_stands_is_ridden_on_to() {
    let at = Point2::new(5.135, 7.3118);
    let piece = |from: Point2| Piece { pts: vec![from, Point2::new(5.135, 6.8)], lead: Vec::new(), tail: Vec::new(), fine: None };
    assert_eq!(super::lift::ride(at, &[], &piece(at)), Some(Vec::new()));
    assert_eq!(super::lift::ride(at, &[], &piece(Point2::new(5.135, 7.2))), None);
}
