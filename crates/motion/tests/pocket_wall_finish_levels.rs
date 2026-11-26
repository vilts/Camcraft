//! Zigzag levels of the 2D pocket with "Do not touch" and of the 3D pocket finish their walls (#3129).
//! Gate map: covers camcraft_motion::machining2d (pocket_avoid, pocket_zigzag) and camcraft_motion::machining3d (hsm).
//! Provenance: wall-reach law from general machining practice - a pocket
//! cut to size has a cutter center within one radius of every wall point.

use camcraft_motion::cl::{MoveKind, Toolpath};
use camcraft_motion::entry::Entry3D;
use camcraft_motion::geo2d::{Loop, Point2, Region};
use camcraft_motion::geometry::{Point3, Triangle};
use camcraft_motion::links::Links;
use camcraft_motion::machining2d::{ARC_FIT_TOL, PocketParams, PocketStrategy, pocket_avoid};
use camcraft_motion::machining3d::{Pocket3DParams, pocket3d};
use camcraft_motion::tools::ToolGeom;
use camcraft_motion::verify::{AvoidItem, AvoidSpec, Cutter};

/// A wall scallop deeper than this is a missing finish ring. Rows 2-3 mm
/// apart with a 3-4 mm radius leave cusps of 0.17-0.3 mm.
const REACH_TOL: f64 = 0.02;

fn lp(pts: Vec<Point2>) -> Loop {
    let Ok(l) = Loop::new(pts) else { panic!("fixture loop") };
    l
}

/// A rounded rectangle, CCW, corners of radius `rc` sampled every 5 degrees.
fn rounded_rect(x0: f64, y0: f64, x1: f64, y1: f64, rc: f64) -> Vec<Point2> {
    let corners = [(x1 - rc, y0 + rc, -90.0), (x1 - rc, y1 - rc, 0.0), (x0 + rc, y1 - rc, 90.0), (x0 + rc, y0 + rc, 180.0)];
    let mut pts = Vec::new();
    for (cx, cy, a0) in corners {
        for k in 0..=18 {
            let a = (a0 + 5.0 * k as f64).to_radians();
            pts.push(Point2::new(cx + rc * a.cos(), cy + rc * a.sin()));
        }
    }
    pts
}

fn circle(cx: f64, cy: f64, r: f64, n: usize) -> Vec<Point2> {
    (0..n)
        .map(|k| {
            let a = k as f64 * std::f64::consts::TAU / n as f64;
            Point2::new(cx + r * a.cos(), cy + r * a.sin())
        })
        .collect()
}

fn box_facets(min: [f64; 3], max: [f64; 3]) -> Vec<Triangle> {
    let p = |x, y, z| Point3::new(x, y, z);
    let v = |i: usize| p([min[0], max[0]][i & 1], [min[1], max[1]][(i >> 1) & 1], [min[2], max[2]][(i >> 2) & 1]);
    let quad = |a: usize, b: usize, c: usize, d: usize| -> [Triangle; 2] {
        [Triangle::new(v(a), v(b), v(c)), Triangle::new(v(a), v(c), v(d))]
    };
    let mut t = Vec::with_capacity(12);
    t.extend(quad(4, 5, 7, 6));
    t.extend(quad(0, 2, 3, 1));
    t.extend(quad(0, 1, 5, 4));
    t.extend(quad(2, 6, 7, 3));
    t.extend(quad(1, 3, 7, 5));
    t.extend(quad(2, 0, 4, 6));
    t
}

/// The feed chords at plane `z`: where the cutter cuts the walls of that level.
fn level_chords(path: &Toolpath, z: f64) -> Vec<(Point2, Point2)> {
    path.moves
        .iter()
        .filter(|m| m.kind == MoveKind::Feed && (m.from.z - z).abs() < 1e-6 && (m.to.z - z).abs() < 1e-6)
        .flat_map(|m| m.chords(0.001))
        .collect()
}

fn seg_dist(p: Point2, a: Point2, b: Point2) -> f64 {
    let (dx, dy) = (b.x - a.x, b.y - a.y);
    let len2 = dx * dx + dy * dy;
    let t = if len2 > 0.0 { (((p.x - a.x) * dx + (p.y - a.y) * dy) / len2).clamp(0.0, 1.0) } else { 0.0 };
    (p.x - a.x - t * dx).hypot(p.y - a.y - t * dy)
}

/// The worst wall point at plane `z`: the largest distance from a wall
/// sample to the nearest cutter center, less the tool radius.
fn worst_scallop(path: &Toolpath, z: f64, wall: &[Point2], radius: f64) -> (f64, Point2) {
    let chords = level_chords(path, z);
    assert!(!chords.is_empty(), "no feed moves at Z {z}");
    let mut worst = (f64::NEG_INFINITY, Point2::new(0.0, 0.0));
    for (i, &a) in wall.iter().enumerate() {
        let b = wall[(i + 1) % wall.len()];
        for s in 0..10 {
            let t = s as f64 / 10.0;
            let p = Point2::new(a.x + t * (b.x - a.x), a.y + t * (b.y - a.y));
            let d = chords.iter().map(|&(c, e)| seg_dist(p, c, e)).fold(f64::INFINITY, f64::min) - radius;
            if d > worst.0 {
                worst = (d, p);
            }
        }
    }
    worst
}

fn assert_no_crossings(path: &Toolpath, what: &str) {
    let crossed = camcraft_motion::verify::crossed_pairs(path, ARC_FIT_TOL);
    assert!(crossed.is_empty(), "{what}: crossing feed pairs {crossed:?}");
}

// ---- 2D pocket with "Do not touch" ----

const TOOL_2D: f64 = 6.0;
const BOTTOM_2D: f64 = -6.0;

/// #3021's part: 60 x 40 pocket, R4 corners, a 16 mm island.
fn walls() -> (Vec<Point2>, Vec<Point2>) {
    (rounded_rect(-30.0, -20.0, 30.0, 20.0, 4.0), circle(0.0, 0.0, 8.0, 72))
}

fn region_2d() -> Region {
    let (outer, isl) = walls();
    let Ok(r) = Region::new(vec![lp(outer), lp(isl)]) else { panic!("fixture region") };
    r
}

/// A short protected pin in the pocket, clear of the measured walls.
fn pin() -> AvoidSpec {
    AvoidSpec {
        items: vec![AvoidItem { label: "pin".into(), tris: box_facets([19.0, -1.0, -10.0], [21.0, 1.0, -3.0]), edges: vec![] }],
        cutter: Cutter::flat(&ToolGeom {
            radius: 0.5 * TOOL_2D,
            nose_radius: 0.0,
            flute_len: 8.0,
            shank_radius: 4.5,
            shank_len: 10.0,
            holder_radius: 12.0,
            holder_len: 20.0,
        }),
        clearance: 0.5,
    }
}

fn params_2d(strategy: PocketStrategy, climb: bool, blend: bool) -> PocketParams {
    PocketParams {
        tool_diameter: TOOL_2D,
        stepover: 2.0,
        stock_to_leave: 0.0,
        clearance_z: 10.0,
        retract_z: None,
        top_z: 0.0,
        bottom_z: BOTTOM_2D,
        stepdown: 2.0,
        feed: 1000.0,
        plunge_feed: 300.0,
        climb,
        strategy,
        entry: Entry3D::Helix,
        helix_radius: 1.0,
        optimal_load: 0.25,
        blend_turns: blend,
        blend_radius: None,
        links: Links::PLANAR,
    }
}

fn assert_avoid_walls_finished(p: &PocketParams, what: &str) {
    let path = match pocket_avoid(&region_2d(), p, &pin()) {
        Ok(t) => t,
        Err(e) => panic!("{what}: pocket_avoid refused: {e:?}"),
    };
    let (outer, isl) = walls();
    let r = 0.5 * TOOL_2D;
    let (d, at) = worst_scallop(&path, BOTTOM_2D, &outer, r);
    assert!(d <= REACH_TOL, "{what}: outer wall scallop {d:.3} mm at {at:?}");
    let (d, at) = worst_scallop(&path, BOTTOM_2D, &isl, r);
    assert!(d <= REACH_TOL, "{what}: island wall scallop {d:.3} mm at {at:?}");
    assert_no_crossings(&path, what);
}

/// #3129: Zigzag with "Do not touch", climb.
#[test]
fn avoid_zigzag_pocket_finishes_walls_and_island() {
    assert_avoid_walls_finished(&params_2d(PocketStrategy::Zigzag, true, false), "avoid zigzag climb");
}

/// #3129: Auto with "Do not touch" resolves to Zigzag on this part.
#[test]
fn avoid_auto_pocket_finishes_walls_conventional() {
    assert_avoid_walls_finished(&params_2d(PocketStrategy::Auto, false, false), "avoid auto conventional");
}

/// #3129: blended row turns keep clear of the closing rings.
#[test]
fn avoid_blended_zigzag_pocket_finishes_walls_without_crossings() {
    assert_avoid_walls_finished(&params_2d(PocketStrategy::Zigzag, true, true), "avoid zigzag blended");
}

// ---- 3D pocket levels ----

/// A 40 x 40 slab, floor at -10, with a 10 x 10 boss to Z 0 at the center.
fn slab_with_boss() -> Vec<Triangle> {
    let mut f = box_facets([-20.0, -20.0, -20.0], [20.0, 20.0, -10.0]);
    f.extend(box_facets([-5.0, -5.0, -10.0], [5.0, 5.0, 0.0]));
    f
}

fn params_3d(strategy: PocketStrategy) -> Pocket3DParams {
    Pocket3DParams {
        pass_start: Default::default(),
        links: Links::ROUGH3D,
        tool_diameter: 8.0,
        stepover: 3.0,
        stock_to_leave: 0.0,
        clearance_z: 5.0,
        top_z: 0.0,
        bottom_z: -10.0,
        stepdown: 5.0,
        feed: 500.0,
        plunge_feed: 100.0,
        climb: true,
        strategy,
        retract_z: None,
        entry: Entry3D::Plunge,
        helix_radius: 1.0,
        optimal_load: camcraft_motion::load::DEFAULT_OPTIMAL_LOAD,
    }
}

fn assert_boss_finished(strategy: PocketStrategy, what: &str) {
    let f = slab_with_boss();
    let Ok(footprint) = camcraft_motion::geometry::slice::project(&f, 64) else { panic!("fixture footprint") };
    let path = match pocket3d(&f, &footprint, &params_3d(strategy)) {
        Ok(t) => t,
        Err(e) => panic!("{what}: pocket3d refused: {e:?}"),
    };
    let boss = [Point2::new(-5.0, -5.0), Point2::new(5.0, -5.0), Point2::new(5.0, 5.0), Point2::new(-5.0, 5.0)];
    for z in [-5.0, -10.0] {
        let (d, at) = worst_scallop(&path, z, &boss, 4.0);
        assert!(d <= REACH_TOL, "{what}: boss wall scallop {d:.3} mm at {at:?}, level Z {z}");
    }
    assert_no_crossings(&path, what);
}

/// #3129: the 3D pocket's Zigzag levels close with a ring on the boss.
#[test]
fn pocket3d_zigzag_levels_finish_the_boss_walls() {
    assert_boss_finished(PocketStrategy::Zigzag, "3d zigzag");
}

/// #3129: Auto takes the Zigzag rows in the 3D level plan.
#[test]
fn pocket3d_auto_levels_finish_the_boss_walls() {
    assert_boss_finished(PocketStrategy::Auto, "3d auto");
}
