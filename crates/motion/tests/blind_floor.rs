//! The 3-D level ladder under a blind floor: a block with a blind square
//! hole, pocketed to the hole's floor, to the block's underside and to a
//! stock bottom below it, at every stepdown and every entry arm. The
//! islands law takes each level's target as the footprint minus ALL the
//! part material standing above its plane (cross-section plus the shadow
//! of what hangs over it), so no level exists under the closed floor and
//! no descent reaches one through solid - checked here against the
//! block's own analytic solid, not against the law that built the path.
//!
//! Provenance: our own design (the blind-floor fix of the M11.2 C+D
//! review), specced from the Prime Directive; no vendor material.
//!
//! Gate map (M11.2 fix): covers camcraft_motion::machining3d (ladder,
//! rest, descent), camcraft_motion::verify.

use camcraft_motion::geo2d::Region;
use camcraft_motion::geometry::{Point3, Triangle};
use camcraft_motion::links::{Link, Links};
use camcraft_motion::machining2d::PocketStrategy;
use camcraft_motion::machining3d::{Entry3D, Pocket3DParams, material_above, pocket3d, pocket3d_air_split, pocket3d_offset};
use camcraft_motion::verify::{Mode, ToolGeom, VerifyContext, Warning, verify};

/// Block half-width, hole half-width, block underside.
const W: f64 = 25.0;
const A: f64 = 8.0;
const ZB: f64 = -15.0;
const R: f64 = 3.0;

fn box_facets(min: [f64; 3], max: [f64; 3]) -> Vec<Triangle> {
    let v = |i: usize| {
        Point3::new(
            [min[0], max[0]][i & 1],
            [min[1], max[1]][(i >> 1) & 1],
            [min[2], max[2]][(i >> 2) & 1],
        )
    };
    let quad = |a: usize, b: usize, c: usize, d: usize| -> [Triangle; 2] {
        [Triangle::new(v(a), v(b), v(c)), Triangle::new(v(a), v(c), v(d))]
    };
    let mut t = Vec::with_capacity(12);
    t.extend(quad(4, 5, 7, 6)); // +z top
    t.extend(quad(0, 2, 3, 1)); // -z bottom
    t.extend(quad(0, 1, 5, 4)); // -y
    t.extend(quad(2, 6, 7, 3)); // +y
    t.extend(quad(1, 3, 7, 5)); // +x
    t.extend(quad(2, 0, 4, 6)); // -x
    t
}

/// The block (top Z 0, underside `ZB`) with a blind hole `[-A, A]²` down
/// to the floor at `-d`: the base under the floor plus a four-box frame
/// around the hole, overlapping off the hole's edges so no slice welds
/// two boxes' corners together.
fn block(d: f64) -> Vec<Triangle> {
    let mut f = box_facets([-W, -W, ZB], [W, W, -d]);
    f.extend(box_facets([-W, -W, -d], [-A, W, 0.0]));
    f.extend(box_facets([A, -W, -d], [W, W, 0.0]));
    f.extend(box_facets([-A - 1.0, -W + 0.5, -d], [A + 1.0, -A, 0.0]));
    f.extend(box_facets([-A - 1.0, A, -d], [A + 1.0, W - 0.5, 0.0]));
    f
}

fn params(entry: Entry3D, strategy: PocketStrategy, stepdown: f64, bottom_z: f64) -> Pocket3DParams {
    Pocket3DParams {
        pass_start: Default::default(),
        links: Links::ROUGH3D,
        tool_diameter: 2.0 * R,
        stepover: 2.0,
        stock_to_leave: 0.0,
        clearance_z: 5.0,
        top_z: 0.0,
        bottom_z,
        stepdown,
        feed: 500.0,
        plunge_feed: 100.0,
        climb: true,
        strategy,
        retract_z: None,
        entry,
        helix_radius: 1.5,
        optimal_load: camcraft_motion::load::DEFAULT_OPTIMAL_LOAD,
    }
}

/// The highest part surface a flat cutter of radius `R` standing at XY
/// meets, penetration tolerance `tol` granted: 0 where its disc reaches
/// the frame, `-d` where it sits wholly in the hole, None off the block.
fn roof(x: f64, y: f64, d: f64, tol: f64) -> Option<f64> {
    let rr = R - tol;
    let (dx, dy) = ((x.abs() - W).max(0.0), (y.abs() - W).max(0.0));
    if dx.hypot(dy) >= rr {
        return None;
    }
    if x.abs() + rr > A || y.abs() + rr > A {
        return Some(0.0);
    }
    Some(-d)
}

/// The first tool position inside the block's solid, sampled along every
/// move (arcs flattened), or None.
fn first_gouge(t: &camcraft_motion::cl::Toolpath, d: f64) -> Option<(usize, Point3)> {
    const TOL: f64 = 1e-3;
    for (i, m) in t.moves.iter().enumerate() {
        for (a, b) in m.segments3(0.005) {
            let n = ((b.x - a.x).hypot(b.y - a.y).max((b.z - a.z).abs()) / 0.1).ceil().clamp(1.0, 10_000.0) as usize;
            for k in 0..=n {
                let s = k as f64 / n as f64;
                let q = Point3::new(a.x + (b.x - a.x) * s, a.y + (b.y - a.y) * s, a.z + (b.z - a.z) * s);
                if roof(q.x, q.y, d, TOL).is_some_and(|h| q.z < h - TOL) {
                    return Some((i, q));
                }
            }
        }
    }
    None
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

fn silent(facets: &[Triangle], region: &Region, t: &camcraft_motion::cl::Toolpath) -> Vec<Warning> {
    let tool = tool();
    let cx = VerifyContext {
        region,
        stock: None,
        model: Some(facets),
        tool: &tool,
        cutter: None,
        mode: Mode::Area,
        stock_top_z: 0.0,
        clearance_z: 5.0,
        stock_to_leave: 0.0,
        gouge_tol: 0.01,
        z_tol: 1e-6,
        finish_tol: None,
        fixtures: &[],
        spindle: None,
        ..Default::default()
    };
    verify(t, &cx)
}

#[test]
fn the_islands_law_shadows_a_blind_floor_all_the_way_down() {
    // Under the underside the plane sections nothing - yet the whole
    // block stands above it, hole floor included.
    let f = block(5.0);
    for z in [-2.5, -5.0, -10.0, ZB, ZB - 3.0, -100.0] {
        let m = material_above(&f, z).expect("islands");
        for q in [(0.0, 0.0), (20.0, 20.0), (-A + 0.5, 0.0)] {
            let q = camcraft_motion::geo2d::Point2::new(q.0, q.1);
            let inside_hole = q.x.abs() < A && q.y.abs() < A;
            let expect = !(inside_hole && z >= -5.0);
            assert_eq!(m.contains(q), expect, "z {z}: {q:?}");
        }
    }
    assert!(material_above(&f, 0.5).expect("islands").is_empty(), "nothing above the top");
}

/// The motions between levels: the entry repeated at every level (the
/// 3-D default), or the same arm as a keep-down transition.
fn link_sets(entry: Entry3D) -> [Links; 2] {
    [
        Links::ROUGH3D,
        Links {
            level: Link::KeepDown,
            transition: Some(entry),
            ramp_angle: 5.0,
            helix_pitch: 1.0,
            ..Links::ROUGH3D
        },
    ]
}

#[test]
fn a_blind_hole_shallower_than_one_stepdown_is_cut_at_its_floor() {
    let f = block(3.0);
    let region = camcraft_motion::geometry::slice::project(&f, 64).expect("silhouette");
    let p = params(Entry3D::Plunge, PocketStrategy::Waterline, 4.0, ZB);
    for ladder in [pocket3d, pocket3d_offset] {
        let t = ladder(&f, &region, &p).expect("the shallow hole has one floor level");
        let mut levels: Vec<_> = t.moves.iter()
            .filter(|m| m.from.z == m.to.z && m.from.z < 0.0)
            .map(|m| m.to.z)
            .collect();
        levels.sort_by(f64::total_cmp);
        levels.dedup();
        assert_eq!(levels, vec![-3.0]);
        assert_eq!(first_gouge(&t, 3.0), None);
        assert_eq!(silent(&f, &region, &t), Vec::<Warning>::new());
    }
}

#[test]
fn a_shallow_floor_keeps_stock_allowance_and_the_top_stock_level() {
    let f = block(3.0);
    let region = camcraft_motion::geometry::slice::project(&f, 64).expect("silhouette");
    for stock_top in [None, Some(1.0)] {
        let mut p = params(Entry3D::Plunge, PocketStrategy::Waterline, 4.0, ZB);
        p.stock_to_leave = 0.5;
        p.links.approach.stock_top = stock_top;
        for ladder in [pocket3d, pocket3d_offset] {
            let t = ladder(&f, &region, &p).expect("the shallow floor leaves the specified stock");
            assert!(t.moves.iter().any(|m| m.from.z == -2.5 && m.to.z == -2.5));
            assert!(t.moves.iter().all(|m| m.from.z.min(m.to.z) >= -2.5));
            if stock_top.is_some() {
                assert!(t.moves.iter().any(|m| m.from.z == 0.0 && m.to.z == 0.0));
            }
            assert_eq!(first_gouge(&t, 3.0), None);
            assert_eq!(silent(&f, &region, &t), Vec::<Warning>::new());
        }
    }
}

#[test]
fn the_rotary_air_split_ladder_keeps_the_plain_level_law() {
    // #2088 floor discovery is the 3D Pocket's only. The air-split ladder
    // (the rotary wrap) keeps the plain law: the shallow hole that
    // pocket3d now cuts at its floor gets no level here, and it refuses.
    let f = block(3.0);
    let region = camcraft_motion::geometry::slice::project(&f, 64).expect("silhouette");
    let p = params(Entry3D::Plunge, PocketStrategy::Waterline, 4.0, ZB);
    assert!(pocket3d(&f, &region, &p).is_ok(), "pocket3d cuts the shallow floor");
    match pocket3d_air_split(&f, &region, &region, 100.0, None, &p) {
        Err(camcraft_motion::machining2d::GenError::Empty(_)) => {}
        other => panic!("the air-split ladder found a shallow floor level: {other:?}"),
    }
}

#[test]
fn excessive_shallow_floor_levels_are_refused_by_name() {
    let mut f = block(3.0);
    let region = camcraft_motion::geometry::slice::project(&f, 64).expect("silhouette");
    let p = params(Entry3D::Plunge, PocketStrategy::Waterline, 4.0, ZB);
    // Each upward flat triangle adds a distinct floor in the first band.
    for k in 1..=camcraft_motion::machining2d::MAX_PASSES as usize {
        let z = -3.0 * k as f64 / (camcraft_motion::machining2d::MAX_PASSES + 1.0);
        f.push(Triangle::new(
            Point3::new(-1.0, -1.0, z),
            Point3::new(1.0, -1.0, z),
            Point3::new(0.0, 1.0, z),
        ));
    }
    let err = pocket3d(&f, &region, &p).expect_err("floor discovery must keep the pass budget");
    assert!(err.to_string().contains("shallow floor levels need more than"), "{err}");
}

#[test]
fn a_blind_pocket_never_enters_solid_under_any_ladder() {
    let entries = [Entry3D::Plunge, Entry3D::Ramp, Entry3D::Helix, Entry3D::Arc];
    let strategies = [PocketStrategy::Waterline, PocketStrategy::Zigzag];
    let mut runs = 0;
    for d in [3.0, 5.5, 8.0, 12.0] {
        let f = block(d);
        let region = camcraft_motion::geometry::slice::project(&f, 64).expect("silhouette");
        for bottom_z in [-d, ZB, ZB - 3.0] {
            for stepdown in [1.5, 2.5, 4.0] {
                for entry in entries {
                    for strategy in strategies {
                        for (k, links) in link_sets(entry).into_iter().enumerate() {
                            for shell in [false, true] {
                                let tag = format!(
                                    "d {d} bottom {bottom_z} step {stepdown} {entry:?} {strategy:?} links#{k} shell {shell}"
                                );
                                let p = Pocket3DParams { links, ..params(entry, strategy, stepdown, bottom_z) };
                                let ladder = if shell { pocket3d_offset } else { pocket3d };
                                // #2088: a shallow floor supplies the missing
                                // plane without moving any level below the floor.
                                let t = ladder(&f, &region, &p)
                                    .unwrap_or_else(|e| panic!("{tag}: the hole is always there to cut: {e}"));
                                assert!(t.all_finite() && t.is_chained(), "{tag}");
                                if let Some((i, q)) = first_gouge(&t, d) {
                                    panic!("{tag}: move {i} enters solid at {q:?} ({:?})", t.moves[i]);
                                }
                                let below = t.moves.iter().any(|m| m.from.z.min(m.to.z) < -d - 1e-6);
                                assert!(!below, "{tag}: a move below the floor at {}", -d);
                                // Verify silent, every arm: the shell Ramp's planar
                                // tail once crossed the zig-zag rows beside the pass
                                // start (a Crossing at the level's own depth); it now
                                // answers the dive link's plan law.
                                let w = silent(&f, &region, &t);
                                assert_eq!(w, Vec::<Warning>::new(), "{tag}: {:?}", t.notes);
                                runs += 1;
                            }
                        }
                    }
                }
            }
        }
    }
    assert_eq!(runs, 4 * 3 * 3 * 4 * 2 * 2 * 2);
}
