//! Tests of the #2121 traverse re-read against solids (`traverse_solids`); our own fixtures.

use super::*;
use crate::cl::Move;
use crate::geometry::cube;

fn tool() -> ToolGeom {
    ToolGeom { radius: 3.0, nose_radius: 0.0, flute_len: 20.0, shank_radius: 3.0, shank_len: 20.0, holder_radius: 10.0, holder_len: 30.0 }
}

fn traverse(from: (f64, f64), to: (f64, f64), z: f64) -> Toolpath {
    Toolpath::new(vec![Move {
        rotary: None,
        kind: MoveKind::Rapid,
        from: Point3::new(from.0, from.1, z),
        to: Point3::new(to.0, to.1, z),
        feed: None,
        arc: None,
        axis: None,
    }])
}

fn quad(a: Point3, b: Point3, c: Point3, d: Point3, f: &mut Vec<Triangle>) {
    f.push(Triangle::new(a, b, c));
    f.push(Triangle::new(a, c, d));
}

/// A closed wedge over x, y in -20..20: its bottom at Z 0, its top a
/// slope from Z 4 at x = -20 up to Z 12 at x = 20 - a tilted block as a
/// 3+2 plane's frame sees it.
fn wedge() -> Vec<Triangle> {
    let p = |x: f64, y: f64, z: f64| Point3::new(x, y, z);
    let (lo, hi) = (4.0, 12.0);
    let mut f = Vec::new();
    quad(p(-20.0, -20.0, 0.0), p(-20.0, 20.0, 0.0), p(20.0, 20.0, 0.0), p(20.0, -20.0, 0.0), &mut f); // bottom
    quad(p(-20.0, -20.0, lo), p(20.0, -20.0, hi), p(20.0, 20.0, hi), p(-20.0, 20.0, lo), &mut f); // top
    quad(p(-20.0, -20.0, 0.0), p(20.0, -20.0, 0.0), p(20.0, -20.0, hi), p(-20.0, -20.0, lo), &mut f); // y = -20
    quad(p(20.0, 20.0, 0.0), p(-20.0, 20.0, 0.0), p(-20.0, 20.0, lo), p(20.0, 20.0, hi), &mut f); // y = 20
    quad(p(-20.0, 20.0, 0.0), p(-20.0, -20.0, 0.0), p(-20.0, -20.0, lo), p(-20.0, 20.0, lo), &mut f); // x = -20
    quad(p(20.0, -20.0, 0.0), p(20.0, 20.0, 0.0), p(20.0, 20.0, hi), p(20.0, -20.0, hi), &mut f); // x = 20
    f
}

fn judged(path: &Toolpath, solids: &[&[Triangle]], clearance: f64) -> Vec<Warning> {
    let mut w = vec![Warning::RapidInStock { at: 0 }];
    acquit_traverses_by_solids(path, Mode::Area, solids, &tool(), (clearance, 1e-3, 0.0), &mut w);
    w
}

#[test]
fn the_wedge_is_a_closed_shell() {
    assert!(closed_shell(&wedge()));
}

#[test]
fn a_traverse_over_the_low_side_of_a_slope_is_air_though_its_far_corner_stands_higher() {
    // Along y at x = -10: the slope stands at Z 6 there, Z 6.6 under the
    // rim at x = -7. The plane Z 8 clears it; the top's far corner Z 12
    // is the vertex reading that convicted it.
    let w = judged(&traverse((-10.0, -30.0), (-10.0, 30.0), 8.0), &[&wedge()], 8.0);
    assert!(w.is_empty(), "{w:?}");
}

#[test]
fn a_traverse_that_meets_the_slope_stands() {
    // Along y at x = 5: the slope is at Z 9 under the centre, over Z 8.
    let w = judged(&traverse((5.0, -30.0), (5.0, 30.0), 8.0), &[&wedge()], 8.0);
    assert_eq!(w, vec![Warning::RapidInStock { at: 0 }]);
    // Up the slope along x: its end is under the stock.
    let w = judged(&traverse((-30.0, 0.0), (10.0, 0.0), 8.0), &[&wedge()], 8.0);
    assert_eq!(w, vec![Warning::RapidInStock { at: 0 }]);
    // The rim alone: centre at x = -2.5 (slope Z 7.5), rim at x = 0.5
    // (slope Z 8.1).
    let w = judged(&traverse((-2.5, -30.0), (-2.5, 30.0), 8.0), &[&wedge()], 8.0);
    assert_eq!(w, vec![Warning::RapidInStock { at: 0 }]);
}

#[test]
fn a_flat_block_under_the_plane_is_air_and_one_over_it_stands() {
    let low = cube(Point3::new(-20.0, -20.0, 0.0), Point3::new(20.0, 20.0, 5.0)).facets;
    let w = judged(&traverse((-30.0, 0.0), (30.0, 0.0), 8.0), &[&low], 8.0);
    assert!(w.is_empty(), "{w:?}");
    let high = cube(Point3::new(-20.0, -20.0, 0.0), Point3::new(20.0, 20.0, 10.0)).facets;
    let w = judged(&traverse((-30.0, 0.0), (30.0, 0.0), 8.0), &[&high], 8.0);
    assert_eq!(w, vec![Warning::RapidInStock { at: 0 }]);
    // Beside the block by less than the radius: its wall is in reach.
    let w = judged(&traverse((-30.0, 22.0), (30.0, 22.0), 8.0), &[&high], 8.0);
    assert_eq!(w, vec![Warning::RapidInStock { at: 0 }]);
}

#[test]
fn the_holder_band_is_read_at_its_own_radius() {
    // A boss to Z 60 beside the trace: outside the 3 mm flute and shank,
    // inside the 10 mm holder that starts 40 mm over the tip (Z 48).
    let low = cube(Point3::new(-20.0, -20.0, 0.0), Point3::new(20.0, 20.0, 5.0)).facets;
    let boss = cube(Point3::new(-2.0, 6.0, 0.0), Point3::new(2.0, 9.0, 60.0)).facets;
    let w = judged(&traverse((-30.0, 0.0), (30.0, 0.0), 8.0), &[&low, &boss], 8.0);
    assert_eq!(w, vec![Warning::RapidInStock { at: 0 }]);
    // The same boss under the holder's bottom is passed over.
    let short = cube(Point3::new(-2.0, 6.0, 0.0), Point3::new(2.0, 9.0, 40.0)).facets;
    let w = judged(&traverse((-30.0, 0.0), (30.0, 0.0), 8.0), &[&low, &short], 8.0);
    assert!(w.is_empty(), "{w:?}");
}

#[test]
fn only_the_new_convictions_are_re_read_and_unknowns_stand() {
    let low = cube(Point3::new(-20.0, -20.0, 0.0), Point3::new(20.0, 20.0, 5.0)).facets;
    let path = traverse((-30.0, 0.0), (30.0, 0.0), 8.0);
    // Under the clearance plane: the pre-#2121 conviction, kept whole.
    assert_eq!(judged(&path, &[&low], 9.0).len(), 1);
    // An open shell, no solid, a flute over nothing, a posed rapid.
    let mut open = low.to_vec();
    open.pop();
    assert_eq!(judged(&path, &[&open], 8.0).len(), 1);
    assert_eq!(judged(&path, &[], 8.0).len(), 1);
    assert_eq!(judged(&traverse((-30.0, 60.0), (30.0, 60.0), 8.0), &[&low], 8.0).len(), 1);
    let mut posed = path.clone();
    if let Some(m) = posed.moves.get_mut(0) {
        m.axis = Some([0.5, 0.0, 0.866]);
    }
    assert_eq!(judged(&posed, &[&low], 8.0).len(), 1);
    for mode in [Mode::Radial, Mode::Revolve, Mode::Rotary] {
        let mut w = vec![Warning::RapidInStock { at: 0 }];
        acquit_traverses_by_solids(&path, mode, &[&low], &tool(), (8.0, 1e-3, 0.0), &mut w);
        assert_eq!(w.len(), 1, "{mode:?}");
    }
    // A coarse model may stand its excess outside its facets.
    let mut w = vec![Warning::RapidInStock { at: 0 }];
    acquit_traverses_by_solids(&path, Mode::Area, &[&low], &tool(), (8.0, 1e-3, 3.5), &mut w);
    assert_eq!(w.len(), 1);
    let mut w = vec![Warning::RapidInStock { at: 0 }];
    acquit_traverses_by_solids(&path, Mode::Area, &[&low], &tool(), (8.0, 1e-3, f64::NAN), &mut w);
    assert_eq!(w.len(), 1);
}
