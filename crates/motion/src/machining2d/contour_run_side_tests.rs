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
        blend_turns: false, blend_radius: None,
        links: crate::links::Links::PLANAR,
        picks: crate::path_picks::PathPicks::default(),
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

#[test]
fn an_open_run_with_a_bottleneck_notch_bridges_the_mouth() {
    // A keyhole notch whose mouth is 6 mm wide (x in 17..23) opening into
    // a 16 mm wide room inside (x in 12..28): with a 10 mm tool (d = 5),
    // the two-sided buffer bridges the 6 mm mouth and leaves an inner CW
    // hole. The air-side offset must bridge the mouth instead of failing.
    let part_poly = vec![
        Point2::new(0.0, 0.0),
        Point2::new(40.0, 0.0),
        Point2::new(40.0, 30.0),
        Point2::new(23.0, 30.0),
        Point2::new(23.0, 22.0),
        Point2::new(28.0, 22.0),
        Point2::new(28.0, 8.0),
        Point2::new(12.0, 8.0),
        Point2::new(12.0, 22.0),
        Point2::new(17.0, 22.0),
        Point2::new(17.0, 30.0),
        Point2::new(0.0, 30.0),
    ];
    let keyhole = move |_z: f64| -> Result<Option<Region>, GenError> {
        Ok(Some(Region::new(vec![Loop::new(part_poly.clone())?])?))
    };
    let run = vec![
        Point2::new(0.0, 30.0),
        Point2::new(17.0, 30.0),
        Point2::new(17.0, 22.0),
        Point2::new(12.0, 22.0),
        Point2::new(12.0, 8.0),
        Point2::new(28.0, 8.0),
        Point2::new(28.0, 22.0),
        Point2::new(23.0, 22.0),
        Point2::new(23.0, 30.0),
        Point2::new(40.0, 30.0),
    ];
    let path = contour_open_run(&run, &params(), &keyhole).expect("bridges bottleneck mouth");
    let f = feeds(&path);
    assert!(!f.is_empty());
    assert!(f.iter().all(|q| q.y >= 34.0 - 0.01), "bridges above mouth: {f:?}");
}

#[test]
fn an_open_run_with_a_short_leg_at_an_inside_corner_stops_clear() {
    // An L-shaped part (material in x <= 0 or y <= 0) whose bottom leg is
    // 3 mm long while the tool radius is 5 mm: the return leg's own end
    // normal (3, 5) sits only 3 mm from x = 0, so stopping at the corner
    // offset cuts without gouging x = 0.
    let l_part = |_z: f64| -> Result<Option<Region>, GenError> {
        let poly = Loop::new(vec![
            Point2::new(-10.0, -10.0),
            Point2::new(3.0, -10.0),
            Point2::new(3.0, 0.0),
            Point2::new(0.0, 0.0),
            Point2::new(0.0, 20.0),
            Point2::new(-10.0, 20.0),
        ])?;
        Ok(Some(Region::new(vec![poly])?))
    };
    let run = vec![Point2::new(0.0, 20.0), Point2::new(0.0, 0.0), Point2::new(3.0, 0.0)];
    let path = contour_open_run(&run, &params(), &l_part).expect("short inside-corner leg");
    let f = feeds(&path);
    assert!(!f.is_empty());
    assert!(
        f.iter().all(|q| q.x >= 5.0 - 1e-3 && (q.x - 3.0).hypot(q.y) >= 5.0 - 0.01),
        "{f:?}"
    );

    // 45-degree acute inside corner with a 7.07 mm end leg (needs
    // r * tan(67.5 deg) = 12.07 mm from the apex).
    let v_part = |_z: f64| -> Result<Option<Region>, GenError> {
        let poly = Loop::new(vec![
            Point2::new(-10.0, -10.0),
            Point2::new(15.0, -10.0),
            Point2::new(5.0, 5.0),
            Point2::new(0.0, 0.0),
            Point2::new(0.0, 25.0),
            Point2::new(-10.0, 25.0),
        ])?;
        Ok(Some(Region::new(vec![poly])?))
    };
    let v_run = vec![Point2::new(0.0, 25.0), Point2::new(0.0, 0.0), Point2::new(5.0, 5.0)];
    let v_path = contour_open_run(&v_run, &params(), &v_part).expect("short acute inside-corner leg");
    assert!(feeds(&v_path).iter().all(|q| q.x >= 5.0 - 1e-3), "{:?}", feeds(&v_path));
}

#[test]
fn an_open_run_with_sharp_or_filleted_inside_corner_and_acute_turn_leaves_tool_radius() {
    // 45-degree inside corner with long legs.
    let acute = |_z: f64| -> Result<Option<Region>, GenError> {
        let poly = Loop::new(vec![
            Point2::new(-10.0, -10.0),
            Point2::new(30.0, -10.0),
            Point2::new(20.0, 20.0),
            Point2::new(0.0, 0.0),
            Point2::new(0.0, 30.0),
            Point2::new(-10.0, 30.0),
        ])?;
        Ok(Some(Region::new(vec![poly])?))
    };
    let run = vec![Point2::new(0.0, 30.0), Point2::new(0.0, 0.0), Point2::new(20.0, 20.0)];
    let path = contour_open_run(&run, &params(), &acute).expect("45 deg inside corner");
    assert!(feeds(&path).iter().all(|q| q.x >= 5.0 - 1e-3), "{:?}", feeds(&path));

    // Acute (>90 deg) outside corner wrapping around a sharp tip.
    let wedge = |_z: f64| -> Result<Option<Region>, GenError> {
        let poly = Loop::new(vec![
            Point2::new(0.0, 0.0),
            Point2::new(0.0, -2.0),
            Point2::new(10.0, 0.0),
        ])?;
        Ok(Some(Region::new(vec![poly])?))
    };
    let tip = vec![Point2::new(0.0, 0.0), Point2::new(10.0, 0.0), Point2::new(0.0, -2.0)];
    let tip_path = contour_open_run(&tip, &params(), &wedge).expect("acute outside corner");
    let tf = feeds(&tip_path);
    assert!(
        tip_path.moves.iter().any(|m| m.kind == MoveKind::Feed
            && m.arc.as_ref().is_some_and(|a| (a.center.x - 10.0).abs() < 0.02 && a.center.y.abs() < 0.02)),
        "wraps around sharp tip with a corner arc: {:?}",
        tip_path.moves
    );
    assert!(tf.iter().any(|q| q.y < -4.0), "continues around past -90 deg: {tf:?}");

    // R2 inside fillet at (0,0) between (0,20)->(0,2) and (2,0)->(20,0)
    // cut with 10 mm tool (r = 5 > 2).
    let mut arc = Vec::new();
    for k in 0..=8 {
        let a = -std::f64::consts::PI + std::f64::consts::FRAC_PI_2 * f64::from(k) / 8.0;
        arc.push(Point2::new(2.0 + 2.0 * a.cos(), 2.0 + 2.0 * a.sin()));
    }
    let fillet_run: Vec<Point2> = std::iter::once(Point2::new(0.0, 20.0))
        .chain(arc.iter().copied())
        .chain(std::iter::once(Point2::new(20.0, 0.0)))
        .collect();
    let mut poly = vec![Point2::new(-10.0, -10.0), Point2::new(20.0, -10.0), Point2::new(20.0, 0.0)];
    poly.extend(arc.into_iter().rev());
    poly.push(Point2::new(0.0, 20.0));
    poly.push(Point2::new(-10.0, 20.0));
    let filleted = move |_z: f64| -> Result<Option<Region>, GenError> {
        Ok(Some(Region::new(vec![Loop::new(poly.clone())?])?))
    };
    let f_path = contour_open_run(&fillet_run, &params(), &filleted).expect("R2 inside fillet with D10");
    assert!(feeds(&f_path).iter().all(|q| q.x >= 5.0 - 1e-3 && q.y >= 5.0 - 1e-3), "{:?}", feeds(&f_path));

    // Collinear seam vertex and tiny near-end collinear split must stay
    // on the side offset without wrapping onto the end cap.
    let col = vec![Point2::new(-20.0, 20.0), Point2::new(0.0, 20.0), Point2::new(20.0, 20.0)];
    let c_path = contour_open_run(&col, &params(), &block).expect("collinear seam");
    assert!(feeds(&c_path).iter().all(|q| (q.y - 25.0).abs() < 1e-3), "{:?}", feeds(&c_path));

    let near_end = vec![Point2::new(-20.0, 20.0), Point2::new(19.99, 20.0), Point2::new(20.0, 20.0)];
    let n_path = contour_open_run(&near_end, &params(), &block).expect("near-end collinear split");
    let nf = feeds(&n_path);
    assert!(
        nf.iter().all(|q| (q.y - 25.0).abs() < 1e-3 && q.x >= -20.0 - 1e-3 && q.x <= 20.0 + 1e-3),
        "must not wrap onto end cap: {nf:?}"
    );
}
