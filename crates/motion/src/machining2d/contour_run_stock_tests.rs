//! Pins of the open run on the stock (#1928): the tool stands one radius
//! plus the stock to leave off the picked line, on the side Side names,
//! and a side that cannot be read is refused by name.

use super::*;
use crate::cl::MoveKind;
use crate::geo2d::Loop;
use crate::machining2d::Lead;

fn params() -> ContourParams {
    ContourParams {
        tool_diameter: 6.0,
        stock_to_leave: 0.0,
        clearance_z: 20.0,
        retract_z: 15.0,
        top_z: 10.0,
        bottom_z: 0.0,
        stepdown: 5.0,
        feed: 500.0,
        plunge_feed: 100.0,
        climb: true,
        side: Side::Auto,
        lead: Lead::None,
        tolerance: 0.01,
        tab_count: 0,
        tab_width: 0.0,
        tab_height: 0.0,
        entry: crate::entry::Entry3D::Plunge,
        helix_radius: 1.0,
        blend_turns: false, blend_radius: None,
        links: crate::links::Links::PLANAR,
        picks: crate::path_picks::PathPicks::default(),
    }
}

fn rect(x0: f64, y0: f64, x1: f64, y1: f64) -> Region {
    let l = Loop::new(vec![Point2::new(x0, y0), Point2::new(x1, y0), Point2::new(x1, y1), Point2::new(x0, y1)]);
    l.and_then(|l| Region::new(vec![l])).expect("rect")
}

/// A 70 x 70 stock centred on the origin.
fn stock(_z: f64) -> Result<Option<Region>, GenError> {
    Ok(Some(rect(-35.0, -35.0, 35.0, 35.0)))
}

/// A 40 x 40 part centred on the origin.
fn part(_z: f64) -> Result<Option<Region>, GenError> {
    Ok(Some(rect(-20.0, -20.0, 20.0, 20.0)))
}

fn none(_z: f64) -> Result<Option<Region>, GenError> {
    Ok(None)
}

/// The stock's north edge.
fn north() -> Vec<Point2> {
    vec![Point2::new(-35.0, 35.0), Point2::new(35.0, 35.0)]
}

/// Every feed point, both ends of each feed.
fn feeds(path: &Toolpath) -> Vec<crate::geometry::Point3> {
    path.moves.iter().filter(|m| m.kind == MoveKind::Feed).flat_map(|m| [m.from, m.to]).collect()
}

/// The (min, max) distance of the feeds to the line y = 35.
fn span(path: &Toolpath) -> (f64, f64) {
    let d: Vec<f64> = feeds(path).iter().map(|q| (q.y - 35.0).abs()).collect();
    (d.iter().copied().fold(f64::INFINITY, f64::min), d.iter().copied().fold(f64::NEG_INFINITY, f64::max))
}

#[test]
fn each_side_stands_the_tool_one_radius_off_the_line() {
    for run in [north(), north().into_iter().rev().collect()] {
        for (side, above) in [(Side::Outside, true), (Side::Auto, true), (Side::Inside, false)] {
            let path = contour_open_run_sided(&run, &params(), side, &stock, &part).expect("cut");
            let (lo, hi) = span(&path);
            assert!((lo - 3.0).abs() < 1e-3 && (hi - 3.0).abs() < 1e-3, "{side:?}: min {lo} max {hi}");
            assert!(feeds(&path).iter().all(|q| (q.y > 35.0) == above), "{side:?}");
            assert!(path.notes.iter().any(|n| n.contains("open run on the stock")), "{:?}", path.notes);
        }
    }
}

#[test]
fn stock_to_leave_adds_to_the_radius() {
    let mut p = params();
    p.stock_to_leave = 0.5;
    let path = contour_open_run_sided(&north(), &p, Side::Outside, &stock, &part).expect("cut");
    let (lo, hi) = span(&path);
    assert!((lo - 3.5).abs() < 1e-3 && (hi - 3.5).abs() < 1e-3, "min {lo} max {hi}");
}

#[test]
fn climb_keeps_the_finished_material_on_the_right() {
    // Off the stock the stock (south) is on the right: travel runs +x.
    let out = contour_open_run_sided(&north(), &params(), Side::Outside, &stock, &none).expect("cut");
    let f = feeds(&out);
    assert!(f.first().is_some_and(|a| f.last().is_some_and(|b| a.x < b.x)), "{f:?}");
    // Into the stock the stock beyond the tool (south) is on the right
    // too: the tool travels +x again.
    let inn = contour_open_run_sided(&north(), &params(), Side::Inside, &stock, &none).expect("cut");
    let f = feeds(&inn);
    assert!(f.first().is_some_and(|a| f.last().is_some_and(|b| a.x < b.x)), "{f:?}");
}

#[test]
fn auto_with_no_part_is_refused_by_name() {
    let e = contour_open_run_sided(&north(), &params(), Side::Auto, &stock, &none).expect_err("no part");
    let s = e.to_string();
    assert!(s.contains("Inside") && s.contains("Outside"), "{s}");
}

#[test]
fn auto_with_the_part_on_both_sides_is_refused_by_name() {
    let mid = vec![Point2::new(-35.0, 0.0), Point2::new(35.0, 0.0)];
    let e = contour_open_run_sided(&mid, &params(), Side::Auto, &stock, &part).expect_err("both sides");
    assert!(e.to_string().contains("both sides"), "{e}");
}

#[test]
fn inside_or_outside_with_no_stock_beside_the_run_is_refused_by_name() {
    let e = contour_open_run_sided(&north(), &params(), Side::Inside, &none, &part).expect_err("no stock");
    assert!(e.to_string().contains("no stock beside it"), "{e}");
}

#[test]
fn a_tool_that_would_enter_the_part_is_refused_by_name() {
    // The part's north wall stands 2 mm inside the stock's: one radius
    // (3 mm) in, the tool reaches it.
    let near = |_z: f64| -> Result<Option<Region>, GenError> { Ok(Some(rect(-20.0, -20.0, 20.0, 33.0))) };
    let e = contour_open_run_sided(&north(), &params(), Side::Inside, &stock, &near).expect_err("enters the part");
    assert!(e.to_string().contains("cuts the part"), "{e}");
}

#[test]
fn a_degenerate_run_is_refused_not_ridden() {
    let one = vec![Point2::new(1.0, 1.0), Point2::new(1.0, 1.0)];
    assert!(contour_open_run_sided(&one, &params(), Side::Outside, &stock, &part).is_err());
    let nan = vec![Point2::new(f64::NAN, 1.0), Point2::new(1.0, 1.0)];
    assert!(contour_open_run_sided(&nan, &params(), Side::Outside, &stock, &part).is_err());
}
