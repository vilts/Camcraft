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
//! Gate map (#43): covers sim::verify (section law b, Boundary walls).

use camcraft_app::cl::{Move, MoveKind, Toolpath};
use camcraft_app::geo2d::{Loop, Point2, Region};
use camcraft_app::geometry::{Point3, Triangle, cube};
use camcraft_app::verify::{Mode, ToolGeom, VerifyContext, Warning, verify};

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
    let (region, geom) = (outline(), tool());
    verify(
        t,
        &VerifyContext {
            region: &region,
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
