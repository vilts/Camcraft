//! Zigzag and Auto pockets finish their walls and islands (#3021).
//! Gate map: covers camcraft_motion::machining2d (pocket, pocket_zigzag, pocket_emit).
//! Provenance: wall-reach law from general machining practice - a pocket
//! cut to size has a cutter center within one radius of every wall point.

use camcraft_motion::cl::{MoveKind, Toolpath};
use camcraft_motion::entry::Entry3D;
use camcraft_motion::geo2d::{Loop, Point2, Region};
use camcraft_motion::links::Links;
use camcraft_motion::machining2d::{ARC_FIT_TOL, PocketParams, PocketStrategy, pocket};

const TOOL: f64 = 6.0;
const BOTTOM: f64 = -6.0;
/// A wall scallop deeper than this is a missing finish ring. A 2 mm
/// row spacing with a 3 mm radius leaves about 0.17 mm cusps.
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

/// The issue's part: 60 x 40 pocket, R4 corners, a 16 mm island.
fn walls() -> (Vec<Point2>, Vec<Point2>) {
    (rounded_rect(-30.0, -20.0, 30.0, 20.0, 4.0), circle(0.0, 0.0, 8.0, 72))
}

fn region(island: bool) -> Region {
    let (outer, isl) = walls();
    let mut loops = vec![lp(outer)];
    if island {
        loops.push(lp(isl));
    }
    let Ok(r) = Region::new(loops) else { panic!("fixture region") };
    r
}

fn params(strategy: PocketStrategy, climb: bool, blend: bool) -> PocketParams {
    PocketParams {
        tool_diameter: TOOL,
        stepover: 2.0,
        stock_to_leave: 0.0,
        clearance_z: 10.0,
        retract_z: None,
        top_z: 0.0,
        bottom_z: BOTTOM,
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

/// The floor feed chords of `path`: where the cutter cuts the wall at full depth.
fn floor_chords(path: &Toolpath) -> Vec<(Point2, Point2)> {
    path.moves
        .iter()
        .filter(|m| m.kind == MoveKind::Feed && (m.from.z - BOTTOM).abs() < 1e-6 && (m.to.z - BOTTOM).abs() < 1e-6)
        .flat_map(|m| m.chords(0.001))
        .collect()
}

fn seg_dist(p: Point2, a: Point2, b: Point2) -> f64 {
    let (dx, dy) = (b.x - a.x, b.y - a.y);
    let len2 = dx * dx + dy * dy;
    let t = if len2 > 0.0 { (((p.x - a.x) * dx + (p.y - a.y) * dy) / len2).clamp(0.0, 1.0) } else { 0.0 };
    (p.x - a.x - t * dx).hypot(p.y - a.y - t * dy)
}

/// The worst wall point: the largest distance from a wall sample to the
/// nearest floor cutter center, less the tool radius (the scallop depth).
fn worst_scallop(path: &Toolpath, wall: &[Point2]) -> (f64, Point2) {
    let chords = floor_chords(path);
    assert!(!chords.is_empty(), "no floor feed moves");
    let mut worst = (f64::NEG_INFINITY, Point2::new(0.0, 0.0));
    for (i, &a) in wall.iter().enumerate() {
        let b = wall[(i + 1) % wall.len()];
        for s in 0..10 {
            let t = s as f64 / 10.0;
            let p = Point2::new(a.x + t * (b.x - a.x), a.y + t * (b.y - a.y));
            let d = chords.iter().map(|&(c, e)| seg_dist(p, c, e)).fold(f64::INFINITY, f64::min) - 0.5 * TOOL;
            if d > worst.0 {
                worst = (d, p);
            }
        }
    }
    worst
}

fn generate(island: bool, p: &PocketParams) -> Toolpath {
    match pocket(&region(island), p) {
        Ok(t) => t,
        Err(e) => panic!("pocket refused: {e:?}"),
    }
}

fn assert_walls_finished(island: bool, p: &PocketParams, what: &str) {
    let path = generate(island, p);
    let (outer, isl) = walls();
    let (d, at) = worst_scallop(&path, &outer);
    assert!(d <= REACH_TOL, "{what}: outer wall scallop {d:.3} mm at {at:?}");
    if island {
        let (d, at) = worst_scallop(&path, &isl);
        assert!(d <= REACH_TOL, "{what}: island wall scallop {d:.3} mm at {at:?}");
    }
    let crossed = camcraft_motion::verify::crossed_pairs(&path, ARC_FIT_TOL);
    assert!(crossed.is_empty(), "{what}: crossing feed pairs {crossed:?}");
}

/// #3021: the issue's part with strategy Auto (it resolves to Zigzag).
#[test]
fn auto_pocket_with_island_finishes_walls_and_island() {
    assert_walls_finished(true, &params(PocketStrategy::Auto, true, false), "auto climb");
}

/// #3021: explicit Zigzag, conventional direction.
#[test]
fn zigzag_pocket_with_island_finishes_walls_conventional() {
    assert_walls_finished(true, &params(PocketStrategy::Zigzag, false, false), "zigzag conventional");
}

/// #3021: no island; the rounded corners and the side walls are cut to size.
#[test]
fn zigzag_pocket_without_island_finishes_walls() {
    assert_walls_finished(false, &params(PocketStrategy::Zigzag, true, false), "zigzag no island");
}

/// #3021: blended turns keep the crossing law with the closing rings.
#[test]
fn blended_zigzag_pocket_finishes_walls_without_crossings() {
    assert_walls_finished(true, &params(PocketStrategy::Zigzag, true, true), "zigzag blended");
}

/// #3021: a square island close to the walls. A blended row turn
/// reached across the closing ring here until the blends obeyed the
/// crossing law.
#[test]
fn blended_zigzag_turns_do_not_cross_the_closing_rings() {
    let sq = |a: f64, b: f64| lp(vec![Point2::new(a, a), Point2::new(b, a), Point2::new(b, b), Point2::new(a, b)]);
    let Ok(region) = Region::new(vec![sq(0.0, 30.0), sq(10.0, 20.0)]) else { panic!("fixture region") };
    // Plunge entries: two helices in this 2 mm band overlap on their own,
    // before and after #3021 (a separate defect).
    let p = PocketParams {
        tool_diameter: 8.0,
        stepover: 3.0,
        bottom_z: -4.0,
        entry: Entry3D::Plunge,
        ..params(PocketStrategy::Zigzag, true, true)
    };
    let path = match pocket(&region, &p) {
        Ok(t) => t,
        Err(e) => panic!("pocket refused: {e:?}"),
    };
    let crossed = camcraft_motion::verify::crossed_pairs(&path, ARC_FIT_TOL);
    assert!(crossed.is_empty(), "crossing feed pairs {crossed:?}");
}
