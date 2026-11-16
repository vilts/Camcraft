//! #1733 arc turns with a radius: the pocket family's corner arcs at a
//! user radius. Every arc the option adds is ONE arc tangent at both
//! ends with a radius no larger than the one asked; no point of the
//! blended path comes closer to the wall than the sharp path does (no
//! arc cuts past the wall), sampled on the true arcs; the corner rest is
//! stated, and a smaller radius states less; no radius is exactly the
//! tool radius every earlier file blends at; the door refuses a radius
//! no control can cut or one past the tool radius, by name; with the
//! option off the radius changes nothing.
//!
//! Gate map (#1733): covers camcraft_motion::machining2d (pocket, fillet_tally).

use camcraft_motion::cl::{Move, MoveKind, Toolpath};
use camcraft_motion::entry::Entry3D;
use camcraft_motion::geo2d::{Loop, Point2, Region};
use camcraft_motion::machining2d::{corner_arc_radius, pocket, GenError, PocketParams, PocketStrategy};

const TOOL_R: f64 = 4.0;

fn rect(x0: f64, y0: f64, x1: f64, y1: f64) -> Loop {
    let pts = [(x0, y0), (x1, y0), (x1, y1), (x0, y1)];
    Loop::new(pts.iter().map(|&(x, y)| Point2::new(x, y)).collect()).unwrap()
}

/// 40x30 pocket with a 10x8 island: convex ring corners and island
/// corners both.
fn pocket_with_island() -> Region {
    Region::new(vec![rect(0.0, 0.0, 40.0, 30.0), rect(15.0, 11.0, 25.0, 19.0)]).unwrap()
}

/// The fixture for a strategy. The HSM planner behind Adaptive takes
/// ~15 s a run in debug, so it gets a small plain pocket and one test
/// of its own (two runs); the fast strategies take the island pocket.
fn fixture(strategy: PocketStrategy) -> Region {
    match strategy {
        PocketStrategy::Adaptive => Region::new(vec![rect(0.0, 0.0, 16.0, 12.0)]).unwrap(),
        _ => pocket_with_island(),
    }
}

fn pp(strategy: PocketStrategy, blend: bool, radius: Option<f64>) -> PocketParams {
    PocketParams {
        links: camcraft_motion::links::Links::PLANAR,
        tool_diameter: 2.0 * TOOL_R,
        stepover: 3.0,
        stock_to_leave: 0.0,
        clearance_z: 5.0,
        retract_z: None,
        top_z: 0.0,
        bottom_z: -2.0,
        stepdown: 2.0,
        feed: 400.0,
        plunge_feed: 80.0,
        climb: true,
        strategy,
        entry: Entry3D::Plunge,
        helix_radius: 1.0,
        optimal_load: camcraft_motion::load::DEFAULT_OPTIMAL_LOAD,
        blend_turns: blend,
        blend_radius: radius,
    }
}

/// The fast strategies behind the kinds that carry the option: the
/// Pocket's Zigzag and offset rings, 2D Offset's Waterline. 2D
/// Adaptive's spiral has its own test (`fixture`).
const STRATEGIES: [PocketStrategy; 3] =
    [PocketStrategy::Zigzag, PocketStrategy::Waterline, PocketStrategy::OffsetRings];

// ---- measuring helpers ---------------------------------------------------

fn seg_dist(q: Point2, a: Point2, b: Point2) -> f64 {
    let (dx, dy) = (b.x - a.x, b.y - a.y);
    let l2 = dx * dx + dy * dy;
    if l2 == 0.0 {
        return (q.x - a.x).hypot(q.y - a.y);
    }
    let t = (((q.x - a.x) * dx + (q.y - a.y) * dy) / l2).clamp(0.0, 1.0);
    (q.x - a.x - t * dx).hypot(q.y - a.y - t * dy)
}

/// Distance from `q` to the wall (every loop edge, islands included).
fn to_wall(region: &Region, q: Point2) -> f64 {
    let mut d = f64::INFINITY;
    for l in region.loops() {
        let p = l.points();
        for i in 0..p.len() {
            d = d.min(seg_dist(q, p[i], p[(i + 1) % p.len()]));
        }
    }
    d
}

/// Points ON the true curve of a planar move: lines every ~0.05 mm,
/// arcs by angle in the travel sense.
fn samples(m: &Move) -> Vec<Point2> {
    let a = Point2::new(m.from.x, m.from.y);
    let b = Point2::new(m.to.x, m.to.y);
    match m.arc {
        None => {
            let n = (((b.x - a.x).hypot(b.y - a.y) / 0.05).ceil() as usize).clamp(1, 4000);
            (0..=n)
                .map(|k| {
                    let t = k as f64 / n as f64;
                    Point2::new(a.x + (b.x - a.x) * t, a.y + (b.y - a.y) * t)
                })
                .collect()
        }
        Some(arc) => {
            let c = arc.center;
            let rho = (a.x - c.x).hypot(a.y - c.y);
            let s = (a.y - c.y).atan2(a.x - c.x);
            let e = (b.y - c.y).atan2(b.x - c.x);
            let tau = std::f64::consts::TAU;
            let sweep = if arc.ccw { (e - s).rem_euclid(tau) } else { (s - e).rem_euclid(tau) };
            (0..=256)
                .map(|k| {
                    let t = s + if arc.ccw { 1.0 } else { -1.0 } * sweep * k as f64 / 256.0;
                    Point2::new(c.x + rho * t.cos(), c.y + rho * t.sin())
                })
                .collect()
        }
    }
}

/// The closest any cutting move in the plane of `z` comes to the wall.
fn min_to_wall(t: &Toolpath, z: f64, region: &Region) -> f64 {
    t.moves
        .iter()
        .filter(|m| m.kind == MoveKind::Feed && m.from.z == z && m.to.z == z)
        .flat_map(samples)
        .map(|q| to_wall(region, q))
        .fold(f64::INFINITY, f64::min)
}

fn tangent(m: &Move, start: bool) -> (f64, f64) {
    let v = match m.arc {
        None => (m.to.x - m.from.x, m.to.y - m.from.y),
        Some(arc) => {
            let p = if start { m.from } else { m.to };
            let (dx, dy) = (p.x - arc.center.x, p.y - arc.center.y);
            if arc.ccw { (-dy, dx) } else { (dy, -dx) }
        }
    };
    let l = v.0.hypot(v.1);
    (v.0 / l, v.1 / l)
}

/// An arc the option added: a fillet, not a sharp-path arc (an island's
/// offset corner) that a fillet only trimmed - a trimmed arc keeps its
/// center, and its untouched end keeps the sharp path's own joint.
fn is_new_arc(m: &Move, sharp: &Toolpath) -> bool {
    m.arc.is_some_and(|a| !sharp.moves.iter().any(|s| s.arc.is_some_and(|b| b.center == a.center)))
}

/// The radii of the arcs the option added, each checked tangent to
/// both neighbours (the center rides the 1 um lattice: micro-radians
/// per mm of radius).
fn added_tangent_radii(blended: &Toolpath, sharp: &Toolpath, what: &str) -> Vec<f64> {
    let mut radii = Vec::new();
    for (i, m) in blended.moves.iter().enumerate() {
        let Some(arc) = m.arc.filter(|_| is_new_arc(m, sharp)) else { continue };
        let rho = (m.from.x - arc.center.x).hypot(m.from.y - arc.center.y);
        let budget = 2e-6 / rho + 1e-9;
        let (before, after) = (&blended.moves[i - 1], &blended.moves[i + 1]);
        for (u, w) in [(tangent(before, false), tangent(m, true)), (tangent(m, false), tangent(after, true))] {
            let cross = u.0 * w.1 - u.1 * w.0;
            let dot = u.0 * w.0 + u.1 * w.1;
            assert!(dot > 0.0 && cross.abs() <= budget, "{what}: kink {cross} at an arc turn end (move {i}: {before:?} / {m:?} / {after:?})");
        }
        radii.push(rho);
    }
    radii
}

/// The `up to X mm` corner rest the rounding note states.
fn stated_rest(t: &Toolpath) -> f64 {
    let n: Vec<&String> = t.notes.iter().filter(|n| n.contains("rounded")).collect();
    let s = n.first().expect("a rounding note");
    let after = s.split("up to ").nth(1).expect("rest figure");
    after.split(' ').next().and_then(|v| v.parse().ok()).expect("number")
}

// ---- the radius ------------------------------------------------------------

#[test]
fn corners_turn_on_tangent_arcs_of_the_given_radius() {
    for strategy in STRATEGIES {
        let region = fixture(strategy);
        let sharp = pocket(&region, &pp(strategy, false, None)).unwrap();
        let blended = pocket(&region, &pp(strategy, true, Some(1.0))).unwrap();
        assert!(blended.is_chained() && blended.all_finite(), "{strategy:?}");
        let radii = added_tangent_radii(&blended, &sharp, &format!("{strategy:?}"));
        assert!(!radii.is_empty(), "{strategy:?}: no corner turned on an arc");
        // The law may shrink a corner without room, never grow one.
        assert!(radii.iter().all(|r| *r <= 1.0 + 1e-6), "{strategy:?}: {radii:?}");
        assert!(radii.iter().any(|r| (*r - 1.0).abs() < 1e-6), "{strategy:?}: no arc at the radius: {radii:?}");
    }
}

#[test]
fn no_arc_turn_cuts_past_the_wall() {
    for strategy in STRATEGIES {
        let region = fixture(strategy);
        let sharp = pocket(&region, &pp(strategy, false, None)).unwrap();
        let d_sharp = min_to_wall(&sharp, -2.0, &region);
        for radius in [Some(0.5), Some(2.0), None] {
            let blended = pocket(&region, &pp(strategy, true, radius)).unwrap();
            {
                let (z, d) = (-2.0, min_to_wall(&blended, -2.0, &region));
                // The sharp path stands within 0.01 mm of the offset (its
                // standing budget); the arcs may not come one micron closer.
                assert!(
                    d_sharp >= TOOL_R - 0.01 && d >= d_sharp - 1e-6,
                    "{strategy:?} r={radius:?} z={z}: center {d} mm from the wall (sharp {d_sharp})"
                );
            }
        }
    }
}

#[test]
fn the_corner_rest_is_stated_and_shrinks_with_the_radius() {
    for strategy in STRATEGIES {
        let region = fixture(strategy);
        let small = pocket(&region, &pp(strategy, true, Some(0.5))).unwrap();
        let full = pocket(&region, &pp(strategy, true, None)).unwrap();
        let (rs, rf) = (stated_rest(&small), stated_rest(&full));
        assert!(rs > 0.0 && rs <= rf + 1e-9, "{strategy:?}: rest {rs} at 0.5 mm, {rf} at the tool radius");
    }
}

#[test]
fn no_radius_is_the_tool_radius_every_earlier_file_blends_at() {
    for strategy in STRATEGIES {
        let region = fixture(strategy);
        let none = pocket(&region, &pp(strategy, true, None)).unwrap();
        let tool = pocket(&region, &pp(strategy, true, Some(TOOL_R))).unwrap();
        assert_eq!(none.moves, tool.moves, "{strategy:?}");
        assert_eq!(none.notes, tool.notes, "{strategy:?}");
    }
}

#[test]
fn with_the_option_off_the_radius_changes_nothing() {
    for strategy in STRATEGIES {
        let region = fixture(strategy);
        let plain = pocket(&region, &pp(strategy, false, None)).unwrap();
        // Off, even a radius the door would refuse is never read.
        for radius in [Some(1.0), Some(f64::NAN)] {
            let t = pocket(&region, &pp(strategy, false, radius)).unwrap();
            assert_eq!(plain.moves, t.moves, "{strategy:?} r={radius:?}");
            assert_eq!(plain.notes, t.notes, "{strategy:?} r={radius:?}");
        }
    }
}

#[test]
fn the_door_refuses_a_radius_no_control_can_cut_by_name() {
    let region = pocket_with_island();
    for radius in [0.0, -1.0, 0.001, f64::NAN, f64::INFINITY, TOOL_R + 0.01] {
        match pocket(&region, &pp(PocketStrategy::Waterline, true, Some(radius))) {
            Err(GenError::Invalid(why)) => assert!(why.contains("corner arc radius"), "{radius}: {why}"),
            other => panic!("radius {radius} must be refused, got {:?}", other.map(|t| t.moves.len())),
        }
    }
    assert_eq!(corner_arc_radius(None, 8.0), Ok(4.0));
    assert_eq!(corner_arc_radius(Some(1.5), 8.0), Ok(1.5));
    assert!(corner_arc_radius(Some(4.5), 8.0).is_err());
}




#[test]
fn the_adaptive_spiral_turns_on_arcs_of_the_radius_inside_the_wall() {
    let strategy = PocketStrategy::Adaptive;
    let region = fixture(strategy);
    let sharp = pocket(&region, &pp(strategy, false, None)).unwrap();
    let blended = pocket(&region, &pp(strategy, true, Some(1.0))).unwrap();
    assert!(blended.is_chained() && blended.all_finite());
    let radii = added_tangent_radii(&blended, &sharp, "Adaptive");
    assert!(!radii.is_empty(), "no corner turned on an arc");
    assert!(radii.iter().all(|r| *r <= 1.0 + 1e-6), "{radii:?}");
    assert!(radii.iter().any(|r| (*r - 1.0).abs() < 1e-6), "no arc at the radius: {radii:?}");
    let (d, d_sharp) = (min_to_wall(&blended, -2.0, &region), min_to_wall(&sharp, -2.0, &region));
    assert!(d_sharp >= TOOL_R - 0.01 && d >= d_sharp - 1e-6, "center {d} mm from the wall (sharp {d_sharp})");
    assert!(stated_rest(&blended) > 0.0);
}
