//! Pins of the open-run air side (#1777): an open edge of a part is cut
//! one tool radius off it on the side without material, and a run with
//! no clear air side is refused by name.

use super::*;
use crate::cl::MoveKind;
use crate::geo2d::Loop;
use crate::machining2d::{Lead, Side};

fn params() -> ContourParams {
    ContourParams {
        tool_diameter: 10.0,
        stock_to_leave: 0.0,
        clearance_z: 20.0,
        retract_z: 15.0,
        top_z: 10.0,
        bottom_z: 0.0,
        stepdown: 5.0,
        feed: 500.0,
        plunge_feed: 100.0,
        climb: true,
        side: Side::Inside,
        lead: Lead::None,
        tolerance: 0.01,
        tab_count: 0,
        tab_width: 0.0,
        tab_height: 0.0,
        entry: crate::entry::Entry3D::Plunge,
        helix_radius: 1.0,
        blend_turns: false,
        links: crate::links::Links::PLANAR,
    }
}

fn rect(x0: f64, y0: f64, x1: f64, y1: f64) -> Loop {
    Loop::new(vec![Point2::new(x0, y0), Point2::new(x1, y0), Point2::new(x1, y1), Point2::new(x0, y1)])
        .expect("rect")
}

/// A 40 x 40 block centred at the origin, the same at every height.
fn block(_z: f64) -> Result<Option<Region>, GenError> {
    Ok(Some(Region::new(vec![rect(-20.0, -20.0, 20.0, 20.0)])?))
}

fn north() -> Vec<Point2> {
    vec![Point2::new(-20.0, 20.0), Point2::new(20.0, 20.0)]
}

fn feeds(path: &Toolpath) -> Vec<crate::geometry::Point3> {
    path.moves.iter().filter(|m| m.kind == MoveKind::Feed).map(|m| m.to).collect()
}

fn plunges(path: &Toolpath) -> Vec<crate::geometry::Point3> {
    path.moves.iter().filter(|m| m.kind == MoveKind::Plunge).map(|m| m.to).collect()
}

#[test]
fn an_open_edge_is_cut_one_radius_off_on_its_air_side() {
    for run in [north(), north().into_iter().rev().collect()] {
        let path = contour_open_run(&run, &params(), &block).expect("air side");
        let f = feeds(&path);
        assert!(!f.is_empty());
        // The tool centre stands 5 mm north of the edge, never on it.
        assert!(f.iter().all(|q| (q.y - 25.0).abs() < 1e-3), "{f:?}");
        // Climb: the block (south) stays on the right - travel runs +x,
        // every level dives at the west end and leaves at the east end.
        let dives = plunges(&path);
        assert!(!dives.is_empty());
        assert!(dives.iter().all(|q| (q.x + 20.0).abs() < 1e-3 && (q.y - 25.0).abs() < 1e-3), "{dives:?}");
        assert!(f.iter().any(|q| (q.x - 20.0).abs() < 1e-3));
        assert!(path.notes.iter().any(|n| n.contains("air side")), "{:?}", path.notes);
    }
}

#[test]
fn stock_to_leave_moves_the_tool_further_off() {
    let mut p = params();
    p.stock_to_leave = 0.5;
    let path = contour_open_run(&north(), &p, &block).expect("air side");
    assert!(feeds(&path).iter().all(|q| (q.y - 25.5).abs() < 1e-3));
}

#[test]
fn conventional_travels_the_air_side_the_other_way() {
    let mut p = params();
    p.climb = false;
    let path = contour_open_run(&north(), &p, &block).expect("air side");
    let f = feeds(&path);
    assert!(f.iter().all(|q| (q.y - 25.0).abs() < 1e-3));
    let dives = plunges(&path);
    assert!(dives.iter().all(|q| (q.x - 20.0).abs() < 1e-3), "conventional dives at the east end: {dives:?}");
}

#[test]
fn a_run_with_material_on_both_sides_is_refused() {
    let mid = vec![Point2::new(-20.0, 0.0), Point2::new(20.0, 0.0)];
    let e = contour_open_run(&mid, &params(), &block).expect_err("both sides");
    assert!(e.to_string().contains("on both sides"), "{e}");
}

#[test]
fn a_run_whose_sides_change_between_levels_is_refused() {
    // Above Z 5 the block lies south of the edge, below it north.
    let flips = |z: f64| -> Result<Option<Region>, GenError> {
        let l = if z > 5.0 { rect(-20.0, -20.0, 20.0, 20.0) } else { rect(-20.0, 20.0, 20.0, 60.0) };
        Ok(Some(Region::new(vec![l])?))
    };
    let mut p = params();
    p.stepdown = 2.0;
    let e = contour_open_run(&north(), &p, &flips).expect_err("sides disagree");
    assert!(e.to_string().contains("air side is not clear"), "{e}");
}

#[test]
fn a_run_that_ends_at_a_wall_is_refused() {
    // A wall rises past the east end of the edge: the tool beside the
    // edge would cut into it.
    let walled = |_z: f64| -> Result<Option<Region>, GenError> {
        Ok(Some(Region::new(vec![rect(-20.0, -20.0, 20.0, 20.0)])?.union(&Region::new(vec![rect(20.0, 20.0, 30.0, 40.0)])?)?))
    };
    let e = contour_open_run(&north(), &params(), &walled).expect_err("wall at the end");
    assert!(e.to_string().contains("cuts the part"), "{e}");
}

#[test]
fn a_run_with_no_material_beside_it_rides_its_line() {
    let none = |_z: f64| -> Result<Option<Region>, GenError> { Ok(None) };
    let path = contour_open_run(&north(), &params(), &none).expect("air");
    assert!(feeds(&path).iter().all(|q| (q.y - 20.0).abs() < 1e-9));
    assert!(path.notes.iter().any(|n| n.contains("no part material")), "{:?}", path.notes);
}

#[test]
fn a_degenerate_run_is_refused_not_ridden() {
    let one = vec![Point2::new(1.0, 1.0), Point2::new(1.0, 1.0)];
    assert!(contour_open_run(&one, &params(), &block).is_err());
    let nan = vec![Point2::new(f64::NAN, 1.0), Point2::new(1.0, 1.0)];
    assert!(contour_open_run(&nan, &params(), &block).is_err());
}
