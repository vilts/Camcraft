//! The growth rings' step search (Gate map (M11.4 C): covers
//! motion::machining2d::adaptive::grow).

use super::*;

/// Issue #845: two overloaded revolutions share their start - a 3 mm side
/// cut beside a slot, past the bound - and part after it: the larger step
/// turns into fresh stock, the smaller keeps the side cut. Walked to the
/// bound (the step search's tries) they read the same first exceedance,
/// and the old fallback kept the first, larger step on that tie. Walked
/// whole they do not tie: the least overload is the smaller step.
#[test]
fn the_overload_fallback_keeps_the_least_whole_reading_not_a_truncated_tie() {
    let Ok(sq) = Region::square(60.0) else { panic!("square") };
    let Ok(mut ras) = Raster::from_region(&sq, 0.1, 0.0) else { panic!("ras") };
    let (r, d) = (5.0, 10.0);
    let rim = crate::quality::rim_for(d, ras.h);
    let walk = (r, rim, r / 10.0);
    let bound = crate::load::deg_at_rim(crate::load::DEFAULT_OPTIMAL_LOAD * 1.05, d, rim);
    ras.walk(&[Point2::new(5.0, 30.0), Point2::new(55.0, 30.0)], r, rim, walk.2);
    let (a0, a1) = (Point2::new(10.0, 33.0), Point2::new(30.0, 33.0));
    let big = [a0, a1, Point2::new(30.0, 45.0)];
    let small = [a0, a1, Point2::new(50.0, 33.0)];
    // The tries' readings: truncated at the bound, a tie.
    let first = |rev: &[Point2]| ras.clone().walk_until(rev, walk, bound);
    let (tb, ts) = (first(&big), first(&small));
    assert!(tb.0 > bound && tb == ts, "truncated readings tie past the bound {bound}: {tb:?} {ts:?}");
    // Whole, the larger step overloads far more.
    let whole = |rev: &[Point2]| ras.clone().walk(rev, r, rim, walk.2).0;
    let (wb, ws) = (whole(&big), whole(&small));
    assert!(wb > ws * SLACK, "whole readings differ: {wb} vs {ws}");
    let Some((i, _, w)) = least_whole(&ras, [&big[..], &small[..]].into_iter(), walk) else { panic!("a revolution is kept") };
    assert_eq!(i, 1, "the smaller step's whole reading {ws} is the least, not the larger's {wb}");
    assert!((w.0 - ws).abs() < 1e-9, "the kept reading is its whole walk: {w:?} vs {ws}");
}

/// A near tie (within [`SLACK`]) keeps the larger step, the first tried;
/// nothing tried keeps nothing.
#[test]
fn a_near_tie_keeps_the_larger_step_and_nothing_tried_keeps_nothing() {
    let Ok(sq) = Region::square(60.0) else { panic!("square") };
    let Ok(mut ras) = Raster::from_region(&sq, 0.1, 0.0) else { panic!("ras") };
    let rim = crate::quality::rim_for(10.0, ras.h);
    let walk = (5.0, rim, 0.5);
    ras.walk(&[Point2::new(5.0, 30.0), Point2::new(55.0, 30.0)], 5.0, rim, 0.5);
    let rev = [Point2::new(10.0, 33.0), Point2::new(40.0, 33.0)];
    let Some((i, _, _)) = least_whole(&ras, [&rev[..], &rev[..]].into_iter(), walk) else { panic!("kept") };
    assert_eq!(i, 0);
    assert!(least_whole(&ras, std::iter::empty(), walk).is_none());
}
