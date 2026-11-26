//! 3D Adaptive's Trochoidal method (issue #1591): a 34 x 16 pocket
//! floored at Z -6 with a boss over one end to Z -3, roughed with a D6
//! in two 3 mm levels - the upper one the whole rectangle, the lower one
//! the 22 x 16 the boss leaves. Under Trochoidal each level is cleared
//! by trochoidal loops inside its own level area; the path verifies
//! clean against the part solid; no rapid ends under the top the run
//! proved cleared; a level the method cannot cut is cut by the Spiral
//! plan and SAID with its Z; and a strategy that is not Adaptive reads
//! no method.
//!
//! Gate map (M4.4): covers camcraft_motion::machining3d (troch, ladder,
//! entry), camcraft_motion::machining2d (adaptive_troch_level,
//! adaptive_troch_lanes, adaptive_troch_clean, trochoid_level).

use camcraft_motion::cl::{MoveKind, Toolpath};
use camcraft_motion::geo2d::{Loop, Point2, Region};
use camcraft_motion::geometry::{Point3, Triangle};
use camcraft_motion::load::DEFAULT_OPTIMAL_LOAD;
use camcraft_motion::machining2d::PocketStrategy;
use camcraft_motion::machining3d::{Entry3D, Pocket3DParams, TROCH_SETTINGS, pocket3d, pocket3d_prev, pocket3d_trochoidal};
use camcraft_motion::verify::{Mode, ToolGeom, VerifyContext, Warning, verify};

const D: f64 = 6.0;
const WHO: &str = "3D Adaptive (Trochoidal)";

fn rect(x0: f64, y0: f64, x1: f64, y1: f64) -> Region {
    let Ok(l) = Loop::new(vec![Point2::new(x0, y0), Point2::new(x1, y0), Point2::new(x1, y1), Point2::new(x0, y1)]) else {
        panic!("fixture loop")
    };
    let Ok(r) = Region::new(vec![l]) else { panic!("fixture region") };
    r
}

/// A solid axis-aligned box as a facet soup, outward CCW (the
/// `machining3d` suite's fixture).
fn box_facets(min: [f64; 3], max: [f64; 3]) -> Vec<Triangle> {
    let v = |i: usize| Point3::new([min[0], max[0]][i & 1], [min[1], max[1]][(i >> 1) & 1], [min[2], max[2]][(i >> 2) & 1]);
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

/// The slab under the pocket (floor at -6) and a boss to -3 over
/// `boss` (x0, y0, x1, y1): an island at the lower level only.
fn part(boss: [f64; 4]) -> Vec<Triangle> {
    let mut f = box_facets([-5.0, -5.0, -12.0], [39.0, 21.0, -6.0]);
    f.extend(box_facets([boss[0], boss[1], -6.0], [boss[2], boss[3], -3.0]));
    f
}

/// The boss over the X end: the lower level is the 22 x 16 beside it.
const END_BOSS: [f64; 4] = [22.0, -5.0, 39.0, 21.0];
/// The boss over all but a 6.2 mm strip: the lower level's centre area
/// is 0.2 mm wide - no room to loop in.
const STRIP_BOSS: [f64; 4] = [-5.0, 6.2, 39.0, 21.0];

fn footprint() -> Region {
    rect(0.0, 0.0, 34.0, 16.0)
}

fn params(strategy: PocketStrategy) -> Pocket3DParams {
    Pocket3DParams {
        pass_start: Default::default(),
        links: camcraft_motion::links::Links::ROUGH3D,
        tool_diameter: D,
        stepover: 2.0,
        stock_to_leave: 0.0,
        clearance_z: 10.0,
        top_z: 0.0,
        bottom_z: -6.0,
        stepdown: 3.0,
        feed: 800.0,
        plunge_feed: 200.0,
        climb: true,
        strategy,
        retract_z: Some(5.0),
        entry: Entry3D::Helix,
        helix_radius: 1.0,
        optimal_load: DEFAULT_OPTIMAL_LOAD,
    }
}

fn verified(facets: &[Triangle], region: &Region, path: &Toolpath) -> Vec<Warning> {
    let tool = ToolGeom {
        radius: D * 0.5,
        nose_radius: 0.0,
        flute_len: 30.0,
        shank_radius: D * 0.5,
        shank_len: 40.0,
        holder_radius: 20.0,
        holder_len: 60.0,
    };
    let cx = VerifyContext {
        region,
        stock: None,
        model: Some(facets),
        tool: &tool,
        cutter: None,
        mode: Mode::Area,
        stock_top_z: 0.0,
        clearance_z: 10.0,
        stock_to_leave: 0.0,
        gouge_tol: 0.01,
        z_tol: 1e-6,
        finish_tol: None,
        fixtures: &[],
        spindle: None,
        ..Default::default()
    };
    verify(path, &cx)
}

/// Every point of every cutting move that ends on plane `z`, sampled.
fn cut_points(path: &Toolpath, z: f64) -> Vec<Point3> {
    let mut out = Vec::new();
    for m in path.moves.iter().filter(|m| m.kind != MoveKind::Rapid && (m.to.z - z).abs() < 1e-9) {
        for (a, b) in m.segments3(0.01) {
            out.push(a);
            out.push(b);
        }
    }
    out
}

fn troch(boss: [f64; 4]) -> Toolpath {
    match pocket3d_trochoidal(&part(boss), &footprint(), &params(PocketStrategy::Adaptive), false, None, None) {
        Ok(p) => p,
        Err(e) => panic!("trochoidal 3-D rough: {e}"),
    }
}

#[test]
fn each_level_is_cleared_by_loops_inside_its_own_level_area() {
    let f = part(END_BOSS);
    let path = troch(END_BOSS);
    println!("notes: {:?}", path.notes);
    assert!(path.is_chained() && path.all_finite());
    let warns = verified(&f, &footprint(), &path);
    assert!(warns.is_empty(), "the path verifies clean against the part: {warns:?}\nnotes {:?}", path.notes);
    // The level areas: the rectangle, then what the boss leaves - each
    // eroded by the tool radius (the cutter-centre area).
    let r = D * 0.5;
    for (z, area) in [(-3.0, rect(0.0, 0.0, 34.0, 16.0)), (-6.0, rect(0.0, 0.0, 22.0, 16.0))] {
        let center = area.offset(-r + 2e-3).expect("centre area");
        let pts = cut_points(&path, z);
        assert!(!pts.is_empty(), "Z {z}: the level cuts");
        for q in &pts {
            assert!(center.contains(Point2::new(q.x, q.y)), "Z {z}: a cut at ({:.3}, {:.3}) leaves the level area", q.x, q.y);
        }
        let arcs = path.moves.iter().filter(|m| m.kind == MoveKind::Feed && m.arc.is_some() && m.from.z == z && m.to.z == z).count();
        assert!(arcs >= 8, "Z {z}: trochoidal loops are arcs on the plane ({arcs})");
    }
    // Both levels are the method's, said once each way; no fallback.
    assert!(path.notes.iter().any(|n| n == TROCH_SETTINGS), "{:?}", path.notes);
    assert!(
        path.notes.iter().any(|n| n.starts_with(&format!("{WHO}: 2 level(s) cut by ")) && n.ends_with("; 0 level(s) cut by the Spiral plan")),
        "{:?}",
        path.notes
    );
    assert!(!path.notes.iter().any(|n| n.contains("instead")), "{:?}", path.notes);
    // And it is not the Spiral plan under another name.
    let spiral = pocket3d_prev(&f, &footprint(), &params(PocketStrategy::Adaptive), None).expect("spiral");
    assert_ne!(spiral.moves, path.moves);
    assert!(!spiral.notes.iter().any(|n| n.contains(WHO)), "{:?}", spiral.notes);
}

/// The material's top is the stock top until a level proved a disc
/// cleared: no rapid of the first level ends under Z 0, and the lower
/// level's bore - inside the upper level's cleared area - starts on the
/// plane Z -3, never lower.
#[test]
fn no_rapid_ends_under_the_top_the_run_proved_cleared() {
    let path = troch(END_BOSS);
    let first_low = path.moves.iter().position(|m| m.to.z < -3.0 - 1e-9).expect("the lower level cuts");
    // The upper level ends with its last cut on the plane Z -3.
    let upper_done = path.moves[..first_low].iter().rposition(|m| m.kind != MoveKind::Rapid).expect("the upper level cuts");
    for (i, m) in path.moves.iter().enumerate().filter(|(_, m)| m.kind == MoveKind::Rapid) {
        let floor = if i < upper_done { 0.0 } else { -3.0 };
        assert!(m.to.z >= floor - 1e-9, "rapid {i} ends at Z {:.3}, under the proved top {floor}", m.to.z);
    }
    // The proof is used, not only obeyed: the lower bore starts on the
    // plane above (the ladder's own law for a level under a cut one).
    assert!(
        path.moves.iter().any(|m| m.kind == MoveKind::Rapid && (m.to.z + 3.0).abs() < 1e-9),
        "the lower level's bore starts on the plane of the level above"
    );
}

#[test]
fn a_level_the_method_cannot_cut_is_cut_by_the_spiral_plan_and_said() {
    let f = part(STRIP_BOSS);
    let path = troch(STRIP_BOSS);
    println!("notes: {:?}", path.notes);
    assert!(path.is_chained() && path.all_finite());
    let warns = verified(&f, &footprint(), &path);
    assert!(warns.is_empty(), "the path verifies clean against the part: {warns:?}\nnotes {:?}", path.notes);
    assert!(
        path.notes.iter().any(|n| n.starts_with(&format!("{WHO}: ")) && n.ends_with(" - the Spiral plan cuts the level at Z -6.000 instead")),
        "the refused level is named with its Z: {:?}",
        path.notes
    );
    assert!(
        path.notes.iter().any(|n| n.starts_with(&format!("{WHO}: 1 level(s) cut by ")) && n.ends_with("; 1 level(s) cut by the Spiral plan")),
        "{:?}",
        path.notes
    );
    // The fallback level reads no plane above as cleared: its descent
    // starts over the stock top, as a first level's does.
    assert!(!cut_points(&path, -6.0).is_empty(), "the fallback level cuts");
    let first_low = path.moves.iter().position(|m| m.to.z < -3.0 - 1e-9).expect("the lower level cuts");
    let down = path.moves[..first_low].iter().rev().find(|m| m.kind == MoveKind::Rapid).expect("a rapid before the descent");
    assert!(down.to.z >= -1e-9, "the fallback level's descent starts over the stock top, not at Z {:.3}", down.to.z);
}

/// In a trochoidal run no level reads the plane above as cleared - not
/// a fallback level under a fallback level either: after a trochoidal
/// level the tool stands at that level's end, and a link that kept down
/// on "the plane above" would feed through stock the loops left. Three
/// levels: loops at -3, then the 0.2 mm strip twice (the Spiral plan).
/// The last level's descent starts over the stock top, like the others.
#[test]
fn a_fallback_level_under_a_fallback_level_still_enters_from_the_stock_top() {
    let mut f = box_facets([-5.0, -5.0, -15.0], [39.0, 21.0, -9.0]);
    f.extend(box_facets([STRIP_BOSS[0], STRIP_BOSS[1], -9.0], [STRIP_BOSS[2], STRIP_BOSS[3], -3.0]));
    let p = Pocket3DParams { bottom_z: -9.0, ..params(PocketStrategy::Adaptive) };
    let path = match pocket3d_trochoidal(&f, &footprint(), &p, false, None, None) {
        Ok(p) => p,
        Err(e) => panic!("trochoidal 3-D rough: {e}"),
    };
    println!("notes: {:?}", path.notes);
    assert!(verified(&f, &footprint(), &path).is_empty(), "notes {:?}", path.notes);
    for z in ["-6.000", "-9.000"] {
        let tail = format!(" - the Spiral plan cuts the level at Z {z} instead");
        assert!(path.notes.iter().any(|n| n.starts_with(&format!("{WHO}: ")) && n.ends_with(&tail)), "Z {z}: {:?}", path.notes);
    }
    assert!(!cut_points(&path, -9.0).is_empty(), "the last level cuts");
    let first_low = path.moves.iter().position(|m| m.to.z < -6.0 - 1e-9).expect("the last level cuts");
    let down = path.moves[..first_low].iter().rev().find(|m| m.kind == MoveKind::Rapid).expect("a rapid before the descent");
    assert!(down.to.z >= -1e-9, "the last level's descent starts over the stock top, not at Z {:.3}", down.to.z);
}

#[test]
fn a_strategy_that_is_not_adaptive_reads_no_method() {
    let f = part(END_BOSS);
    let p = params(PocketStrategy::OffsetRings);
    let a = pocket3d(&f, &footprint(), &p).expect("rings");
    let b = pocket3d_trochoidal(&f, &footprint(), &p, false, None, None).expect("rings under the method");
    assert_eq!(a.moves, b.moves);
    assert_eq!(a.notes, b.notes);
}
