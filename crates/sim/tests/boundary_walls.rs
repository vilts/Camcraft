//! The Boundary-mode gouge law reads a straight move along its whole
//! length against the region's WALLS (issue #43): a Contour G1 chorded
//! across a convex corner of the part's wall, both ends exactly R off
//! it, sinks into the corner and is convicted at that move; the same
//! contour turning the corner on a true R arc is clean. Open edges - a
//! Face's rim, where rows overrun by design - stay excused (the
//! area_controls Face tests pin that side).
//!
//! Provenance: our own fixtures, specced from the shop rule of holding
//! the cutter radius to the finish wall (machine-tool handbooks); no
//! vendor material.
//!
//! #1659: a loop may carry vertices on a straight side. A Face row run
//! exactly through two of them, from air to air, crosses the open rim
//! as truly as one through an edge's interior, and is excused the same
//! way; a move that stays inside, or only touches a vertex, still
//! answers to the walls.
//!
//! Gate map (#43, #1659): covers sim::verify (section law b, Boundary
//! walls, the rim crossing).

use camcraft_sim::cl::{Move, MoveKind, Toolpath};
use camcraft_sim::geo2d::{Loop, Point2, Region};
use camcraft_sim::geometry::{Point3, Triangle, cube};
use camcraft_sim::verify::{Mode, ToolGeom, VerifyContext, Warning, verify};

const R: f64 = 4.0;
const Z: f64 = -5.0;

fn p(x: f64, y: f64) -> Point3 {
    Point3::new(x, y, Z)
}

fn tool() -> ToolGeom {
    ToolGeom {
        radius: R,
        nose_radius: 0.0,
        flute_len: 30.0,
        shank_radius: R,
        shank_len: 40.0,
        holder_radius: 20.0,
        holder_len: 60.0,
    }
}

/// The part: a 20 x 20 block, top at 0, the contour's selection its
/// outline.
fn block() -> Vec<Triangle> {
    cube(Point3::new(0.0, 0.0, -10.0), Point3::new(20.0, 20.0, 0.0)).facets.to_vec()
}

fn outline() -> Region {
    let pts = vec![Point2::new(0.0, 0.0), Point2::new(20.0, 0.0), Point2::new(20.0, 20.0), Point2::new(0.0, 20.0)];
    Region::new(vec![Loop::new(pts).expect("square")]).expect("region")
}

fn judge(t: &Toolpath, model: Option<&[Triangle]>) -> Vec<Warning> {
    judge_in(t, model, &outline())
}

fn judge_in(t: &Toolpath, model: Option<&[Triangle]>, region: &Region) -> Vec<Warning> {
    let geom = tool();
    verify(
        t,
        &VerifyContext {
            region,
            stock: None,
            model,
            tool: &geom,
            cutter: None,
            mode: Mode::Boundary,
            stock_top_z: 0.0,
            clearance_z: 10.0,
            stock_to_leave: 0.0,
            gouge_tol: 0.01,
            z_tol: 1e-6,
            finish_tol: None,
            fixtures: &[],
            spindle: None,
            ..Default::default()
        },
    )
}

/// Outside contour along x = 24 up to the (20, 20) corner, round it,
/// then along y = 24: `corner` is the move that turns it.
fn contour(corner: Move) -> Toolpath {
    let moves = vec![
        Move::new(MoveKind::Plunge, Point3::new(24.0, 10.0, 5.0), p(24.0, 10.0), Some(100.0)),
        Move::new(MoveKind::Feed, p(24.0, 10.0), p(24.0, 20.0), Some(500.0)),
        corner,
        Move::new(MoveKind::Feed, p(20.0, 24.0), p(10.0, 24.0), Some(500.0)),
    ];
    Toolpath { moves, ..Toolpath::default() }
}

fn chorded() -> Toolpath {
    // Both ends exactly R off the corner; the chord's middle stands
    // 2.83 off it - 1.17 mm into the wall.
    contour(Move::new(MoveKind::Feed, p(24.0, 20.0), p(20.0, 24.0), Some(500.0)))
}

fn arced() -> Toolpath {
    contour(Move::new(MoveKind::Feed, p(24.0, 20.0), p(20.0, 24.0), Some(500.0)).with_arc(Point2::new(20.0, 20.0), true))
}

#[test]
fn a_contour_g1_chorded_across_a_convex_wall_corner_is_a_gouge_at_that_move() {
    let part = block();
    assert_eq!(judge(&chorded(), Some(&part)), vec![Warning::Gouge { at: 2 }]);
    // The 2.5-D reading (no mesh): the outline's edges are the walls.
    assert_eq!(judge(&chorded(), None), vec![Warning::Gouge { at: 2 }]);
}

#[test]
fn the_same_contour_rounding_the_corner_on_a_true_arc_is_clean() {
    let part = block();
    assert_eq!(judge(&arced(), Some(&part)), Vec::new());
    assert_eq!(judge(&arced(), None), Vec::new());
}

/// The outline as the stock's was read (#1659): the 20 x 20 square with
/// extra vertices on its straight sides, two of them on the line y 16.
fn outline_with_side_vertices() -> Region {
    let pts = [(20.0, 0.0), (20.0, 16.0), (20.0, 20.0), (8.0, 20.0), (0.0, 20.0), (0.0, 16.0), (0.0, 0.0), (8.0, 0.0)];
    let pts = pts.iter().map(|&(x, y)| Point2::new(x, y)).collect();
    Region::new(vec![Loop::new(pts).expect("square")]).expect("region")
}

fn one_feed(from: Point3, to: Point3) -> Toolpath {
    let moves = vec![
        Move::new(MoveKind::Plunge, Point3::new(from.x, from.y, 5.0), from, Some(100.0)),
        Move::new(MoveKind::Feed, from, to, Some(500.0)),
    ];
    Toolpath { moves, ..Toolpath::default() }
}

#[test]
fn a_row_through_two_side_vertices_tangent_to_the_far_edge_crosses_the_rim_and_is_clean() {
    // The issue's last Face row: from R outside the rim to R outside it,
    // along y 16 - exactly through the vertices (0, 16) and (20, 16) - its
    // edge tangent to the y 20 side. With no part it is an open rim.
    let region = outline_with_side_vertices();
    let row = one_feed(p(-R, 20.0 - R), p(20.0 + R, 20.0 - R));
    assert_eq!(judge_in(&row, None, &region), Vec::new());
    // The plain square read the same row clean already: it crosses the
    // side edges' interiors.
    assert_eq!(judge_in(&row, None, &outline()), Vec::new());
}

#[test]
fn a_move_inside_or_only_touching_a_vertex_still_answers_to_the_walls() {
    let region = outline_with_side_vertices();
    // Inside the region, 1 mm nearer the y 20 wall than R: a gouge.
    let inside = one_feed(p(4.0, 20.0 - R + 1.0), p(16.0, 20.0 - R + 1.0));
    // (Its plunge stands as near, so it is convicted as well.)
    let w = judge_in(&inside, None, &region);
    assert!(w.contains(&Warning::Gouge { at: 1 }), "{w:?}");
    // Across the corner (20, 20) from outside to outside, both its
    // neighbours on one side of the move: a touch, not a crossing - the
    // centre on the wall.
    let touch = one_feed(p(16.0, 24.0), p(24.0, 16.0));
    assert_eq!(judge_in(&touch, None, &region), vec![Warning::Gouge { at: 1 }]);
}
