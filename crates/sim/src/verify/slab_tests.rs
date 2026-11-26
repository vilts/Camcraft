//! #3831: a ramped path through a prismatic slab sections the model once, and the gouge law asks the wall probe only for the edges a move stands near - our own analytic fixtures.
//!
//! Gate map (#3831): covers camcraft_sim::verify (slab, rest, section, reach).

use super::reach::band_reach_counted;
use super::rest::Rest;
use super::section;
use super::types::{Mode, ToolGeom, VerifyContext, Warning};
use crate::cl::{Move, MoveKind, Toolpath};
use crate::geo2d::{Loop, Point2, Region};
use crate::geometry::{Point3, Triangle, cube};

fn square(x0: f64, y0: f64, x1: f64, y1: f64) -> Region {
    let pts = vec![Point2::new(x0, y0), Point2::new(x1, y0), Point2::new(x1, y1), Point2::new(x0, y1)];
    match Loop::new(pts).and_then(|l| Region::new(vec![l])) {
        Ok(r) => r,
        Err(e) => panic!("region: {e:?}"),
    }
}

/// A 6 mm cutter, a 10 mm shank 5 mm over the tip, a 30 mm holder.
fn tool() -> ToolGeom {
    ToolGeom { radius: 3.0, nose_radius: 0.0, flute_len: 5.0, shank_radius: 5.0, shank_len: 20.0, holder_radius: 15.0, holder_len: 30.0 }
}

/// A 20 mm boss (Z 0..10) on a 2 mm floor plate, in a 100 mm pocket.
fn boss() -> Vec<Triangle> {
    let mut f = cube(Point3::new(0.0, 0.0, -2.0), Point3::new(100.0, 100.0, 0.0)).facets.to_vec();
    f.extend(cube(Point3::new(40.0, 40.0, 0.0), Point3::new(60.0, 60.0, 10.0)).facets.iter().cloned());
    f
}

fn feed(a: (f64, f64, f64), b: (f64, f64, f64)) -> Move {
    Move::new(MoveKind::Feed, Point3::new(a.0, a.1, a.2), Point3::new(b.0, b.1, b.2), Some(500.0))
}

/// A ramp of `n` moves along Y `y`, X 10 to 90, Z 9 down to Z 1: every
/// move ends at a Z of its own, all inside the boss's one slab.
fn ramp(n: usize, y: f64) -> Vec<Move> {
    let (dz, dx) = (8.0 / n as f64, 80.0 / n as f64);
    (0..n)
        .map(|k| {
            let (a, b) = (k as f64, (k + 1) as f64);
            feed((10.0 + a * dx, y, 9.0 - a * dz), (10.0 + b * dx, y, 9.0 - b * dz))
        })
        .collect()
}

/// Laws (b) and (c) on `moves`: the findings and the sections sliced,
/// with the slab reading and with every plane on its own. The section
/// count measures law (b); law (c) separately keeps exact plane samples.
fn both(model: &[Triangle], moves: Vec<Move>, mode: Mode) -> ((Vec<Warning>, usize), (Vec<Warning>, usize)) {
    let (region, tool) = (square(0.0, 0.0, 100.0, 100.0), tool());
    let cx = VerifyContext {
        region: &region,
        stock: Some(&region),
        model: Some(model),
        tool: &tool,
        mode,
        stock_top_z: 10.0,
        clearance_z: 30.0,
        gouge_tol: 0.01,
        z_tol: 1e-6,
        ..Default::default()
    };
    let path = Toolpath { moves, ..Toolpath::default() };
    let run = |rest: Rest| {
        let mut w = Vec::new();
        section::tangency(&path, &cx, &rest, 0.0, cx.gouge_tol, &mut w);
        let geometric_sections = rest.sliced();
        band_reach_counted(&path, &cx, &rest, cx.gouge_tol, &mut w);
        (w, geometric_sections)
    };
    let slab = run(Rest::new(&path, &cx, cx.gouge_tol, &[]));
    let plane = run(Rest::new(&path, &cx, cx.gouge_tol, &[]).plane_exact());
    (slab, plane)
}

/// A stepped boss on the floor plate: a wide block (Z 0..5) under a
/// narrow one (Z 5..10) - two slabs with two different sections.
fn stepped() -> Vec<Triangle> {
    let mut f = cube(Point3::new(0.0, 0.0, -2.0), Point3::new(100.0, 100.0, 0.0)).facets.to_vec();
    f.extend(cube(Point3::new(40.0, 40.0, 0.0), Point3::new(60.0, 60.0, 5.0)).facets.iter().cloned());
    f.extend(cube(Point3::new(45.0, 45.0, 5.0), Point3::new(55.0, 55.0, 10.0)).facets.iter().cloned());
    f
}

/// The floor plate under a four-sided pyramid (Z 0 to an apex at Z 10):
/// its section shrinks with every plane.
fn pyramid() -> Vec<Triangle> {
    let mut f = cube(Point3::new(0.0, 0.0, -2.0), Point3::new(100.0, 100.0, 0.0)).facets.to_vec();
    let apex = Point3::new(50.0, 50.0, 10.0);
    let c = [(40.0, 40.0), (60.0, 40.0), (60.0, 60.0), (40.0, 60.0)];
    for k in 0..4 {
        let (a, b) = (c[k], c[(k + 1) % 4]);
        f.push(Triangle::new(Point3::new(a.0, a.1, 0.0), Point3::new(b.0, b.1, 0.0), apex));
    }
    f
}

fn shank_hits(w: &[Warning]) -> usize {
    w.iter().filter(|w| matches!(w, Warning::ShankHit { .. })).count()
}

#[test]
fn a_ramp_in_a_prismatic_slab_sections_the_model_once() {
    // Clear of the boss (Y 34), the shank 1 mm into its wall (Y 36), and
    // the cutter's rim 1.5 mm into it (Y 38.5).
    for (n, y, hits) in [(500usize, 34.0, false), (500, 36.0, true), (2000, 36.0, true), (500, 38.5, true)] {
        for mode in [Mode::Area, Mode::Boundary] {
            let ((slab, sliced), (plane, by_plane)) = both(&boss(), ramp(n, y), mode);
            assert_eq!(by_plane, n, "n {n} y {y} {mode:?}: one section for each plane, as before");
            assert_eq!(sliced, 1, "n {n} y {y} {mode:?}: one section for the slab");
            assert_eq!(slab, plane, "n {n} y {y} {mode:?}: the findings and their order must not move");
            assert_eq!(shank_hits(&slab) > 0, hits, "n {n} y {y} {mode:?}: {slab:?}");
        }
    }
}

#[test]
fn a_level_ladder_keeps_the_section_of_each_flat_plane() {
    // Four rings round the boss, 1 mm off its walls, four moves a side
    // (the law reads a straight move at its end): the shank is in it.
    let (lo, hi) = (36.0, 64.0);
    let corners = [(lo, lo), (hi, lo), (hi, hi), (lo, hi), (lo, lo)];
    let mut moves = Vec::new();
    for z in [8.0, 6.0, 4.0, 2.0] {
        for w in corners.windows(2) {
            let (a, b) = (w[0], w[1]);
            for k in 0..4 {
                let at = |t: f64| (a.0 + (b.0 - a.0) * t, a.1 + (b.1 - a.1) * t, z);
                moves.push(feed(at(f64::from(k) / 4.0), at(f64::from(k + 1) / 4.0)));
            }
        }
    }
    let ((slab, sliced), (plane, by_plane)) = both(&boss(), moves, Mode::Area);
    assert_eq!((sliced, by_plane), (4, 4), "a flat plane is never read through its slab");
    assert_eq!(slab, plane);
    assert!(shank_hits(&slab) > 0, "{slab:?}");
}

#[test]
fn each_slab_of_a_stepped_part_keeps_its_own_section() {
    // The ramp at Y 49 puts the shank on the narrow upper block's wall
    // and clear of the wide lower block's: only the upper slab convicts.
    let ((slab, sliced), (plane, by_plane)) = both(&stepped(), ramp(500, 49.0), Mode::Area);
    assert_eq!(by_plane, 500);
    assert_eq!(sliced, 2, "the slab over Z 5 and the slab under it");
    assert_eq!(slab, plane, "the findings and their order must not move");
    assert!(slab.iter().any(|w| matches!(w, Warning::ShankHit { part: true, .. })), "{slab:?}");
}

#[test]
fn a_sloped_part_is_still_sectioned_at_every_plane() {
    let ((slab, sliced), (plane, by_plane)) = both(&pyramid(), ramp(200, 36.0), Mode::Area);
    assert_eq!((sliced, by_plane), (200, 200), "no slab of a pyramid is a prism");
    assert_eq!(slab, plane);
}

#[test]
fn the_gouge_law_probes_a_wall_only_where_a_move_stands_near_it() {
    use super::section::wall::PROBES;
    let (region, tool) = (square(0.0, 0.0, 100.0, 100.0), tool());
    let model = boss();
    let cx = VerifyContext {
        region: &region,
        model: Some(&model),
        tool: &tool,
        mode: Mode::Boundary,
        stock_top_z: 10.0,
        clearance_z: 30.0,
        gouge_tol: 0.01,
        z_tol: 1e-6,
        ..Default::default()
    };
    let run = |moves: Vec<Move>| {
        let path = Toolpath { moves, ..Toolpath::default() };
        let mut w = Vec::new();
        PROBES.with(|c| c.set(0));
        section::tangency(&path, &cx, &Rest::new(&path, &cx, cx.gouge_tol, &[]), 0.0, cx.gouge_tol, &mut w);
        (w, PROBES.with(std::cell::Cell::get))
    };
    // 500 moves, each 34 mm or more from the region's four edges: the
    // old order asked the probe for every edge of every move, twice.
    let (clear, probes) = run(ramp(500, 34.0));
    assert_eq!(clear, Vec::new());
    assert_eq!(probes, 0, "no edge is near, so no probe is asked");
    // A move that ends 1 mm from the region's X 0 edge still asks it,
    // and the edge is no wall there (the plate is under the plane).
    let (near, probes) = run(vec![feed((10.0, 20.0, 5.0), (1.0, 20.0, 5.0))]);
    assert!(probes > 0, "the near edge is probed");
    assert_eq!(near, Vec::new(), "an open rim over air is no wall");
}

/// The shank stays above the finished boss. Only the stock law can see
/// these contacts, including points between fixed wall samples.
fn stock_collision_hits(width: f64, x: f64, y: f64, radius: f64, shank_radius: f64, z_offset: f64) -> usize {
    let region = square(0.0, 0.0, width + 100.0, 100.0);
    let mut model = cube(Point3::new(0.0, 0.0, -2.0), Point3::new(width + 100.0, 100.0, 0.0)).facets.to_vec();
    model.extend(cube(Point3::new(40.0, 40.0, 0.0), Point3::new(40.0 + width, 60.0, 10.0)).facets.iter().cloned());
    let tool = ToolGeom { radius, nose_radius: 0.0, flute_len: 20.0, shank_radius, shank_len: 20.0, holder_radius: 0.0, holder_len: 0.0 };
    let cx = VerifyContext {
        region: &region,
        stock: Some(&region),
        model: Some(&model),
        tool: &tool,
        mode: Mode::Area,
        stock_top_z: 30.0,
        clearance_z: 60.0,
        gouge_tol: 0.01,
        z_tol: 1e-6,
        ..Default::default()
    };
    let moves = (0..500).map(|k| {
        let z = 9.0 + z_offset - f64::from(k) * 8.0 / 500.0;
        feed((x, y, z), (x, y, z - 8.0 / 500.0))
    }).collect();
    let path = Toolpath { moves, ..Toolpath::default() };
    shank_hits(&super::verify(&path, &cx))
}

#[test]
fn a_shared_slab_keeps_near_tangent_stock_collisions() {
    // Review #3977: fixed 0.25 mm samples miss every move here.
    assert_eq!(stock_collision_hits(20.0, 50.125, 35.0501, 3.0, 5.0, 0.0), 500);
}

#[test]
fn a_shared_slab_keeps_small_band_stock_collisions() {
    assert_eq!(stock_collision_hits(20.0, 50.125, 39.97, 0.02, 0.1, 0.0), 500);
}

#[test]
fn a_shared_slab_keeps_stock_collisions_when_wall_spacing_would_widen() {
    // The 40,040 mm perimeter exceeded the old 50,000 sample budget.
    // Shift the Z planes so their diagonal vertices meet X 10,040.125.
    assert_eq!(stock_collision_hits(20_000.0, 10_040.125, 35.0501, 3.0, 5.0, 0.0000625), 500);
}
