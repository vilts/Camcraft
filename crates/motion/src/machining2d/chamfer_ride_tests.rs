//! Tests of the no-pick 2D chamfer's ride (#2435, #2596, #2887, #2936,
//! #2944, #2929): which wall each top loop rides, and which loops refuse
//! by name. Our own regressions over hand-built sections; no vendor
//! material consulted.

use super::*;

const UPRIGHT: Option<Point2> = Some(Point2 { x: 0.0, y: 0.0 });

/// A 1 mm ride, 1 mm deep, over the corner plane Z9. The section half
/// way down is the wall itself: each modelled bevel here ends above it.
fn ride(top: &Region, corner: &Region, lean: Option<Point2>) -> Result<Region, GenError> {
    ride_loops(top, corner, corner, 1.0, 1.0, 9.0, lean)
}

fn rect(x0: f64, y0: f64, x1: f64, y1: f64) -> Loop {
    Loop::new(vec![Point2::new(x0, y0), Point2::new(x1, y0), Point2::new(x1, y1), Point2::new(x0, y1)]).expect("rect")
}

#[test]
fn a_vertical_wall_rides_its_own_top_loop() {
    let top = Region::new(vec![rect(-20.0, -10.0, 20.0, 10.0)]).expect("top");
    let got = ride(&top, &top.clone(), UPRIGHT).expect("ride");
    assert_eq!(got, top);
}

#[test]
fn a_modelled_bevel_rides_the_wall_under_it() {
    // The bevel's top edge 1 mm inside the wall.
    let top = Region::new(vec![rect(-19.0, -9.0, 19.0, 9.0)]).expect("top");
    let wall = Region::new(vec![rect(-20.0, -10.0, 20.0, 10.0)]).expect("wall");
    let got = ride(&top, &wall, UPRIGHT).expect("ride");
    assert_eq!(got, wall);
}

#[test]
fn a_wall_farther_than_the_width_refuses_by_name() {
    let top = Region::new(vec![rect(-19.0, -9.0, 19.0, 9.0)]).expect("top");
    let plate = Region::new(vec![rect(-50.0, -30.0, 50.0, 30.0)]).expect("plate");
    let err = ride(&top, &plate, UPRIGHT).expect_err("no wall within the width");
    assert!(err.to_string().contains("no wall under it"), "{err}");
    // #2521: the source may be the stock, so the text names no part.
    assert!(err.to_string().contains("would ride into the material under it;"), "{err}");
    assert!(!err.to_string().contains("part"), "a stock-source chamfer has no part to name: {err}");
}

#[test]
fn a_lower_edge_matched_to_no_top_loop_is_left_alone() {
    let top = Region::new(vec![rect(-20.0, -10.0, 20.0, 10.0)]).expect("top");
    // A second, lower boss beside the island: its wall is in the
    // corner section, but no edge of the top plane stands over it.
    let corner = Region::new(vec![rect(-20.0, -10.0, 20.0, 10.0), rect(30.0, -10.0, 40.0, 10.0)]).expect("corner");
    let got = ride(&top, &corner, UPRIGHT).expect("ride");
    assert_eq!(got, top);
}

fn hole(x0: f64, y0: f64, x1: f64, y1: f64) -> Loop {
    Loop::new(vec![Point2::new(x0, y0), Point2::new(x0, y1), Point2::new(x1, y1), Point2::new(x1, y0)]).expect("hole")
}

#[test]
fn a_sharp_pocket_beside_a_modelled_bevel_is_left_alone() {
    // #2596: the outer edge carries a 1 mm bevel, the pocket rim is
    // sharp. Only the outer wall is ridden.
    let top = Region::new(vec![rect(-29.0, -19.0, 29.0, 19.0), hole(-10.0, -5.0, 10.0, 5.0)]).expect("top");
    let corner = Region::new(vec![rect(-30.0, -20.0, 30.0, 20.0), hole(-10.0, -5.0, 10.0, 5.0)]).expect("corner");
    let got = ride(&top, &corner, UPRIGHT).expect("ride");
    assert_eq!(got, Region::new(vec![rect(-30.0, -20.0, 30.0, 20.0)]).expect("wall"));
}

#[test]
fn a_bevelled_pocket_rim_inside_a_sharp_plate_refuses_by_name() {
    // Alone, the rim would read as a boss and the ride would travel
    // against its material (#1555): refused, the user picks it.
    let top = Region::new(vec![rect(-30.0, -20.0, 30.0, 20.0), hole(-9.0, -4.0, 9.0, 4.0)]).expect("top");
    let corner = Region::new(vec![rect(-30.0, -20.0, 30.0, 20.0), hole(-10.0, -5.0, 10.0, 5.0)]).expect("corner");
    let err = ride(&top, &corner, UPRIGHT).expect_err("an orphan bevelled rim");
    assert!(err.to_string().contains("pick the edges to break"), "{err}");
}

/// #2887's plate: a 1 mm bevel on the X0 side only, the other three
/// sides sharp, and a sharp pocket. `extra` adds a collinear vertex
/// on the bevel's top edge.
fn one_side_bevelled(extra: bool) -> (Region, Region) {
    let mut outer = vec![Point2::new(1.0, 0.0), Point2::new(60.0, 0.0), Point2::new(60.0, 40.0), Point2::new(1.0, 40.0)];
    if extra {
        outer.push(Point2::new(1.0, 20.0));
    }
    let top = Region::new(vec![Loop::new(outer).expect("outer"), hole(20.0, 15.0, 40.0, 25.0)]).expect("top");
    let corner = Region::new(vec![rect(0.0, 0.0, 60.0, 40.0), hole(20.0, 15.0, 40.0, 25.0)]).expect("corner");
    (top, corner)
}

#[test]
fn a_loop_bevelled_on_one_side_refuses_by_name() {
    // Vertex gap 0 (both bevel corners lie on the wall's sides): the
    // old reading was "sharp", the tip riding x = 1 inside the part.
    let (top, corner) = one_side_bevelled(false);
    let err = ride(&top, &corner, UPRIGHT).expect_err("a partly bevelled loop").to_string();
    assert!(err.contains("only part of its length"), "{err}");
    assert!(err.contains("pick the edges to break"), "{err}");
    assert!(err.contains("near X1.000 Y20.000"), "names the bevelled part: {err}");
}

#[test]
fn a_loop_bevelled_on_one_side_refuses_with_an_extra_vertex_too() {
    // Vertex gap 1: the old reading was "bevelled", the wall ridden
    // along the three sharp sides as well.
    let (top, corner) = one_side_bevelled(true);
    let err = ride(&top, &corner, UPRIGHT).expect_err("a partly bevelled loop").to_string();
    assert!(err.contains("only part of its length"), "{err}");
    assert!(err.contains("pick the edges to break"), "{err}");
}

#[test]
fn a_loop_bevelled_all_round_is_not_partly_bevelled() {
    let (top, wall) = (rect(-19.0, -9.0, 19.0, 9.0), rect(-20.0, -10.0, 20.0, 10.0));
    let mid = Region::new(vec![wall.clone()]).expect("mid");
    assert_eq!(read(&top, &wall, &mid, 1.0, UPRIGHT).partly_bevelled(1.0), None);
    assert_eq!(read(&wall, &wall, &mid, 1.0, UPRIGHT).partly_bevelled(1.0), None);
}

/// The lean of an upright wall over 1 mm in the frame of `leaning_box`.
const TILT_10: Option<Point2> = Some(Point2 { x: 0.0, y: 0.176327 });

/// #2936: the sections of a 70 mm box stock in a frame tilted 10
/// degrees, 1 mm apart - the corner section and the same outline
/// moved by `tan(10 deg)` in Y. The mesh diagonals leave a collinear
/// vertex at a different place on each side of each section.
fn leaning_box() -> (Region, Region) {
    let ring = |y0: f64, y1: f64, mids: [f64; 4]| {
        let p = [(35.0, mids[0]), (35.0, y1), (mids[1], y1), (-35.0, y1), (-35.0, mids[2]), (-35.0, y0), (mids[3], y0), (35.0, y0)];
        Region::new(vec![Loop::new(p.iter().map(|&(x, y)| Point2::new(x, y)).collect()).expect("ring")]).expect("region")
    };
    let top = ring(-33.776662, 37.303201, [3.1953, -22.139987, 1.42424, -21.060124]);
    let corner = ring(-33.952989, 37.126874, [-6.407086, -18.585993, 3.479512, -24.614117]);
    (top, corner)
}

#[test]
fn a_wall_leaning_in_the_frame_is_not_a_modelled_chamfer() {
    // No edge of the box carries a chamfer: the whole outline moved.
    // The ride is the wall at the corner depth, as before #2887.
    let (top, corner) = leaning_box();
    let got = ride(&top, &corner, TILT_10).expect("a leaning box rides its wall");
    assert_eq!(got, corner);
}

#[test]
fn a_moved_outline_with_one_side_bevelled_still_refuses() {
    // The leaning box with a 1 mm bevel on its X35 side as well:
    // that side is no part of the lean, and the #2887 refusal holds.
    let (top, corner) = leaning_box();
    let bevelled: Vec<Point2> = top.loops()[0].points().iter().map(|p| Point2::new(p.x.min(34.0), p.y)).collect();
    let top = Region::new(vec![Loop::new(bevelled).expect("ring")]).expect("top");
    let err = ride(&top, &corner, TILT_10).expect_err("a partly bevelled loop").to_string();
    assert!(err.contains("only part of its length"), "{err}");
}

#[test]
fn an_upright_bevel_beside_an_equal_undercut_lip_still_refuses() {
    // #2944: a 1 mm bevel on the -X side and a 1 mm undercut lip on
    // the +X side. The top section is the wall moved 1 mm in X, but
    // an upright frame moves no wall: this is no lean, and the
    // #2887 refusal holds.
    let top = Region::new(vec![rect(-29.0, -20.0, 31.0, 20.0)]).expect("top");
    let corner = Region::new(vec![rect(-30.0, -20.0, 30.0, 20.0)]).expect("corner");
    let err = ride(&top, &corner, UPRIGHT).expect_err("a partly bevelled loop").to_string();
    assert!(err.contains("only part of its length"), "{err}");
}

#[test]
fn the_wall_lean_is_the_frames_own() {
    // #2944: no frame and an upright frame lean no wall; a 10 degree
    // tilt leans one tan(10 deg) per mm; an axis in XY leans none.
    let o = Point3::new(0.0, 0.0, 0.0);
    assert_eq!(wall_lean(None, 1.0), UPRIGHT);
    assert_eq!(wall_lean(TiltedFrame::new(o, [0.0, 0.0, 1.0]).as_ref(), 1.0), UPRIGHT);
    let tilted = TiltedFrame::from_angles(o, 10.0, 0.0).expect("frame");
    let l = wall_lean(Some(&tilted), 1.0).expect("a lean");
    assert!((l.x.hypot(l.y) - 10f64.to_radians().tan()).abs() < 1e-9, "{l:?}");
    assert_eq!(wall_lean(TiltedFrame::new(o, [1.0, 0.0, 0.0]).as_ref(), 1.0), None);
}

/// #2929: the sections of #2887's plate (60 x 40, a sharp pocket) whose
/// X0 side stands `top` mm off the wall at the top plane and `mid` mm
/// off it half way down.
fn x0_side_off(top: f64, mid: f64) -> (Region, Region, Region) {
    let plate = |x: f64| {
        let outer = vec![Point2::new(x, 0.0), Point2::new(60.0, 0.0), Point2::new(60.0, 40.0), Point2::new(x, 40.0)];
        Region::new(vec![Loop::new(outer).expect("outer"), hole(20.0, 15.0, 40.0, 25.0)]).expect("plate")
    };
    (plate(top), plate(mid), plate(0.0))
}

#[test]
fn a_plate_with_one_drafted_side_rides_its_wall() {
    // #2929: the X0 side drafted 5 degrees (0.087 mm over the 1 mm
    // ride) - no modelled chamfer anywhere, so every edge is broken,
    // the sharp pocket too, the drafted side along its wall.
    for deg in [3.5, 5.0, 10.0] {
        let off = f64::to_radians(deg).tan();
        let (top, mid, corner) = x0_side_off(off, off * 0.5);
        let got = ride_loops(&top, &mid, &corner, 1.0, 1.0, 9.0, UPRIGHT).expect("a drafted side is no modelled chamfer");
        assert_eq!(got, corner, "{deg} deg");
    }
}

#[test]
fn a_small_bevel_on_one_side_is_no_draft() {
    // #2929: a 0.2 mm modelled bevel on the X0 side stands 0.2 mm off
    // the wall at the top plane - a draft's offset - but the section
    // half way down is on the wall: no straight face, a chamfer.
    let (top, mid, corner) = x0_side_off(0.2, 0.0);
    let err = ride_loops(&top, &mid, &corner, 1.0, 1.0, 9.0, UPRIGHT).expect_err("a partly bevelled loop").to_string();
    assert!(err.contains("only part of its length"), "{err}");
}

#[test]
fn a_bevel_ending_below_half_depth_is_no_draft() {
    // The 0.25 mm bevel ends 0.7 mm below the top. The vertical wall
    // below it makes the half-depth section depart from a straight draft.
    let (top, mid, corner) = x0_side_off(0.25, 0.25 * (0.7 - 0.5) / 0.7);
    let err = ride_loops(&top, &mid, &corner, 1.0, 1.0, 9.0, UPRIGHT).expect_err("a partly bevelled loop").to_string();
    assert!(err.contains("only part of its length"), "{err}");
}

#[test]
fn a_side_whose_slope_turns_back_is_no_draft() {
    // The side stands at X0.2, X0.3 and X0 at Z10, Z9.5 and Z9: it
    // bends back half way down, so it is no one straight drafted wall.
    let (top, mid, corner) = x0_side_off(0.2, 0.3);
    let err = ride_loops(&top, &mid, &corner, 1.0, 1.0, 9.0, UPRIGHT).expect_err("a partly bevelled loop").to_string();
    assert!(err.contains("only part of its length"), "{err}");
}

#[test]
fn a_straight_face_steeper_than_a_draft_is_a_chamfer() {
    // #2929: a 1 mm 45 degree bevel deeper than the ride is one
    // straight face too; its slope is no draft's, and #2887 holds.
    let (top, mid, corner) = x0_side_off(1.0, 0.5);
    let err = ride_loops(&top, &mid, &corner, 1.0, 1.0, 9.0, UPRIGHT).expect_err("a partly bevelled loop").to_string();
    assert!(err.contains("only part of its length"), "{err}");
}

#[test]
fn half_depth_must_match_along_the_whole_piece() {
    let (top, _, corner) = x0_side_off(0.2, 0.1);
    let outer = Loop::new(vec![Point2::new(0.0, 0.0), Point2::new(60.0, 0.0), Point2::new(60.0, 40.0), Point2::new(0.2, 40.0)]).expect("mid outer");
    let mid = Region::new(vec![outer, hole(20.0, 15.0, 40.0, 25.0)]).expect("mid");
    assert!(ride_loops(&top, &mid, &corner, 1.0, 1.0, 9.0, UPRIGHT).is_err());
}

#[test]
fn an_unrelated_mid_depth_hole_must_not_excuse_a_bevel() {
    let (top, mid, corner) = x0_side_off(0.2, 0.0);
    let mut loops = mid.loops().to_vec();
    loops.push(hole(0.1, 19.0, 1.0, 21.0));
    let mid = Region::new(loops).expect("mid with a buried hole");
    assert!(ride_loops(&top, &mid, &corner, 1.0, 1.0, 9.0, UPRIGHT).is_err());
}

#[test]
fn a_drafted_side_allows_collinear_half_depth_vertices() {
    let (top, _, corner) = x0_side_off(0.2, 0.1);
    let outer = Loop::new(vec![Point2::new(0.1, 0.0), Point2::new(60.0, 0.0), Point2::new(60.0, 40.0), Point2::new(0.1, 40.0), Point2::new(0.1, 27.0), Point2::new(0.1, 13.0)])
        .expect("mid outer");
    let mid = Region::new(vec![outer, hole(20.0, 15.0, 40.0, 25.0)]).expect("mid");
    assert_eq!(ride_loops(&top, &mid, &corner, 1.0, 1.0, 9.0, UPRIGHT).expect("a split draft"), corner);
}
