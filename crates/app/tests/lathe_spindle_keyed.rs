//! The lathe spindle law on keyed shafts (issue #71): a keyway is an
//! off-centre feature whose walls' normal lines miss the axis, and a
//! plain least-squares fit lets them drag the fitted origin a tenth of
//! a millimetre off a shaft sitting exactly on G54 - refused with the
//! advice to move the work offset, which cannot help. The robust fit
//! drops the facets whose normal lines miss the candidate axis and
//! refits on the revolved surface, so a correctly placed keyed shaft
//! passes and generates; the same shaft off G54 is still refused, by
//! the misplaced-part message, naming the share the fit set aside.
//! A keyway too large for the plain fit's gate (issue #113: 20 mm long,
//! a deep floor) is read by the consensus pass and passes the same way.
//!
//! Gate map (M4.4): covers app::operation (spindle_law's reading and
//! refusal text, collect_job's door), motion::multiaxis::axis and
//! axis_robust and axis_consensus (the robust fit, via the reading).
//! Lane shape (M4.4): all tests here run in seconds.

use flecs_ecs::prelude::World;

use camcraft_app::commands::{
    AddTool, CreateOperation, GenerateOperation, History, ImportModel, LoadMachine, SetStock,
};
use camcraft_app::geometry::{Mesh, Point3, Triangle};
use camcraft_app::machine::Machine;
use camcraft_app::operation::{spindle_reading, Height, OpParams, SrcRef, SPINDLE_TOL};
use camcraft_app::schema::Location;
use camcraft_app::tools::catalog;
use camcraft_app::world::new_world;

const R: f64 = 10.0;
const SEGS: usize = 72;
const Z: (f64, f64) = (-30.0, 30.0);
/// The keyway runs from Z0 to Z10, its floor at x = 8.
const KEY_Z: (f64, f64) = (0.0, 10.0);
const FLOOR_X: f64 = 8.0;

fn ring(j: i64, z: f64, dx: f64) -> Point3 {
    let th = std::f64::consts::TAU * j as f64 / SEGS as f64;
    Point3::new(R * th.cos() + dx, R * th.sin(), z)
}

/// A D20 shaft about the line x = dx, y = 0, with a flat-floored
/// keyway on +x whose walls meet the ring at +-`half` segments (5
/// degrees each: half 3 is a 5.18 mm key, half 6 a 10 mm one).
fn keyed_shaft(half: i64, dx: f64) -> Vec<Triangle> {
    keyed_shaft_in(half, dx, KEY_Z, FLOOR_X)
}

/// [`keyed_shaft`] with the keyway over `key_z` and its floor at
/// x = `floor_x` (about the shaft's own axis).
fn keyed_shaft_in(half: i64, dx: f64, key_z: (f64, f64), floor_x: f64) -> Vec<Triangle> {
    let mut f = Vec::new();
    let zs = [Z.0, key_z.0, key_z.1, Z.1];
    let n = SEGS as i64;
    for j in -n / 2..n / 2 {
        let in_key = j >= -half && j < half;
        for (k, w) in zs.windows(2).enumerate() {
            if in_key && k == 1 {
                continue; // the keyway takes this stretch of wall
            }
            let (a1, a2, b1, b2) = (ring(j, w[0], dx), ring(j + 1, w[0], dx), ring(j, w[1], dx), ring(j + 1, w[1], dx));
            f.push(Triangle::new(a1, a2, b2));
            f.push(Triangle::new(a1, b2, b1));
        }
        let (c0, c1) = (Point3::new(dx, 0.0, Z.0), Point3::new(dx, 0.0, Z.1));
        f.push(Triangle::new(c0, ring(j + 1, Z.0, dx), ring(j, Z.0, dx)));
        f.push(Triangle::new(c1, ring(j, Z.1, dx), ring(j + 1, Z.1, dx)));
    }
    if half == 0 {
        return f; // the plain revolve: no keyway
    }
    let top = ring(half, 0.0, 0.0);
    let (xt, ys) = (top.x, top.y);
    let q = |x: f64, y: f64, z: f64| Point3::new(x + dx, y, z);
    let (z0, z1) = key_z;
    // The floor (+x), the walls (-y at +ys, +y at -ys).
    f.push(Triangle::new(q(floor_x, -ys, z0), q(floor_x, ys, z0), q(floor_x, ys, z1)));
    f.push(Triangle::new(q(floor_x, -ys, z0), q(floor_x, ys, z1), q(floor_x, -ys, z1)));
    f.push(Triangle::new(q(floor_x, ys, z0), q(xt, ys, z0), q(xt, ys, z1)));
    f.push(Triangle::new(q(floor_x, ys, z0), q(xt, ys, z1), q(floor_x, ys, z1)));
    f.push(Triangle::new(q(xt, -ys, z0), q(floor_x, -ys, z0), q(floor_x, -ys, z1)));
    f.push(Triangle::new(q(xt, -ys, z0), q(floor_x, -ys, z1), q(xt, -ys, z1)));
    // The pocket's end faces: +z at the keyway's foot, -z at its head.
    let mut poly = vec![q(floor_x, -ys, 0.0)];
    poly.extend((-half..=half).map(|j| ring(j, 0.0, dx)));
    poly.push(q(floor_x, ys, 0.0));
    let at = |p: Point3, z: f64| Point3::new(p.x, p.y, z);
    for w in poly[1..].windows(2) {
        f.push(Triangle::new(at(poly[0], z0), at(w[0], z0), at(w[1], z0)));
        f.push(Triangle::new(at(poly[0], z1), at(w[1], z1), at(w[0], z1)));
    }
    f
}

fn lathe_world(part: Vec<Triangle>) -> (World, History) {
    let world = new_world();
    let mut history = History::default();
    let stock = SetStock::new(
        &world,
        camcraft_app::stock::StockShape::Box { dx: 40.0, dy: 40.0, dz: 80.0 },
        Location::default(),
    );
    history.execute(&world, stock.expect("box stock"));
    let lathe = Machine { model: "TestBench lathe".into(), kind: Some("lathe".into()), ..Default::default() };
    history.execute(&world, LoadMachine::new(&world, "lathe.mch".into(), Some(lathe)).expect("lathe"));
    let tool = catalog().into_iter().find(|(n, _)| *n == "TN-1600-0R8").map(|(_, t)| t).expect("insert");
    history.execute(&world, AddTool::new(&world, "TN-1600-0R8", tool).expect("tool"));
    history.execute(&world, ImportModel::from_mesh("part", Mesh::from_facets(part)));
    (world, history)
}

/// Generate a Facing on the shaft's top; Err is the refusal text, Ok
/// the generated path's notes.
fn face(part: Vec<Triangle>) -> Result<Vec<String>, String> {
    let (world, mut history) = lathe_world(part);
    let p = OpParams::Facing {
        tolerance: 0.05,
        clearance_z: Height::fixed(40.0),
        top_z: Height::fixed(30.0),
        bottom_z: Height::fixed(29.0),
        stepdown: 0.5,
        feed: 150.0,
        plunge_feed: 80.0,
        picks: Default::default(),
    };
    let cmd = CreateOperation::new(&world, "F1", "TN-1600-0R8", SrcRef::Entity("part".into()), p).expect("create");
    history.execute(&world, cmd);
    let cmd = GenerateOperation::new(&world, "F1").map_err(|e| e.to_string())?;
    history.execute(&world, cmd);
    let e = camcraft_app::model::find_by_name(&world, "F1").ok_or("no F1")?;
    let t = camcraft_app::model::toolpath_of(&world, &e).ok_or("F1 has no path")?;
    Ok(t.path.notes)
}

/// The set-aside note a trusted fit with features aside carries.
const ASIDE: &str = "set aside and is not turned";

#[test]
fn a_keyed_shaft_on_g54_passes_the_spindle_law_and_generates() {
    for half in [3, 6] {
        let part = keyed_shaft(half, 0.0);
        let r = spindle_reading(&part, [0.0; 3]).expect("a keyed shaft reads as revolved");
        println!("key half {half}: {r:?}");
        assert!(r.worst() <= SPINDLE_TOL, "key half {half}: the keyway dragged the fit: {r:?}");
        let notes = match face(part) {
            Ok(n) => n,
            Err(e) => panic!("key half {half}: a correctly placed keyed shaft must generate: {e}"),
        };
        // Named, never silent (issue #71 review): the keyway is not
        // turned, and the note says so with the share it set aside.
        let pct = format!("{:.1}% of the turned (non-end-face) surface", (100.0 * r.outlier_share).max(0.1));
        let note = notes.iter().find(|n| n.contains(ASIDE));
        assert!(note.is_some_and(|n| n.contains(&pct) && n.contains("later setup")), "key half {half}: {notes:?}");
        assert!(!notes.iter().any(|n| n.contains(" 0.0%")), "key half {half}: {notes:?}");
    }
}

#[test]
fn a_plain_revolve_on_g54_generates_with_no_set_aside_note() {
    let part = keyed_shaft(0, 0.0);
    let r = spindle_reading(&part, [0.0; 3]).expect("a plain shaft reads as revolved");
    assert!(r.trusted && r.outlier_share == 0.0, "{r:?}");
    let notes = face(part).unwrap_or_else(|e| panic!("a plain revolve on G54 generates: {e}"));
    assert!(!notes.iter().any(|n| n.contains(ASIDE)), "a plain revolve says nothing: {notes:?}");
}

#[test]
fn a_keyed_shaft_off_g54_is_refused_as_misplaced_naming_the_set_aside_share() {
    for half in [3, 6] {
        let part = keyed_shaft(half, 1.0);
        let r = spindle_reading(&part, [0.0; 3]).expect("reads");
        assert!((r.offset_mid - 1.0).abs() < 1e-6, "the robust fit reads the true 1 mm offset: {r:?}");
        let e = face(part).expect_err("a shaft 1 mm off the spindle never generates");
        for s in ["lathe spindle", "1.000 mm off", "move the work offset", "% of the part's turned (non-end-face) surface"] {
            assert!(e.contains(s), "key half {half}: the refusal names '{s}': {e}");
        }
        assert!(!e.contains("biased by off-centre features"), "a misplaced part is not a biased fit: {e}");
    }
}

/// Issue #113: a 10 mm wide, 20 mm long keyway with its floor 6 mm deep
/// (x = 4) - its walls drag the plain fit past the sagitta band, which
/// refused the part as not revolved before the robust pass saw it.
fn deep_keyed(dx: f64) -> Vec<Triangle> {
    keyed_shaft_in(6, dx, (0.0, 20.0), 4.0)
}

#[test]
fn a_deep_twenty_mm_keyway_on_g54_passes_the_spindle_law_and_generates() {
    let part = deep_keyed(0.0);
    let r = spindle_reading(&part, [0.0; 3]).expect("a deep-keyed shaft reads as revolved");
    assert!(r.trusted && r.worst() <= SPINDLE_TOL, "the keyway dragged the fit: {r:?}");
    assert!(r.outlier_share > 0.0, "the keyway's walls are set aside: {r:?}");
    let notes = face(part).unwrap_or_else(|e| panic!("a correctly placed deep-keyed shaft must generate: {e}"));
    let pct = format!("{:.1}% of the turned (non-end-face) surface", (100.0 * r.outlier_share).max(0.1));
    assert!(notes.iter().any(|n| n.contains(ASIDE) && n.contains(&pct)), "named, never silent: {notes:?}");
}

#[test]
fn a_deep_twenty_mm_keyway_off_g54_is_refused_as_misplaced() {
    let part = deep_keyed(1.0);
    let r = spindle_reading(&part, [0.0; 3]).expect("reads");
    assert!((r.offset_mid - 1.0).abs() < 1e-6, "the consensus fit reads the true 1 mm offset: {r:?}");
    let e = face(part).expect_err("a shaft 1 mm off the spindle never generates");
    for s in ["lathe spindle", "1.000 mm off", "move the work offset", "% of the part's turned (non-end-face) surface"] {
        assert!(e.contains(s), "the refusal names '{s}': {e}");
    }
    assert!(!e.contains("biased by off-centre features"), "a misplaced part is not a biased fit: {e}");
}

#[test]
fn a_shaft_with_a_large_off_axis_lump_is_still_refused_by_name() {
    // An 8 x 8 x 50 block beside the shaft, off every meridian through
    // its walls: 30 % of the answering surface revolves about nothing.
    let mut part = keyed_shaft(0, 0.0);
    let q = Point3::new;
    let v = [q(12.0, 6.0, -25.0), q(20.0, 6.0, -25.0), q(20.0, 14.0, -25.0), q(12.0, 14.0, -25.0)];
    let up = |p: Point3| q(p.x, p.y, 25.0);
    let w = [v[0], v[1], v[2], v[3], up(v[0]), up(v[1]), up(v[2]), up(v[3])];
    for [i, j, k, l] in [[0, 3, 2, 1], [4, 5, 6, 7], [0, 1, 5, 4], [1, 2, 6, 5], [2, 3, 7, 6], [3, 0, 4, 7]] {
        part.push(Triangle::new(w[i], w[j], w[k]));
        part.push(Triangle::new(w[i], w[k], w[l]));
    }
    assert!(spindle_reading(&part, [0.0; 3]).is_none(), "no axis is read off a part mostly not revolved");
    let e = face(part).expect_err("a part mostly not revolved never generates");
    assert!(e.contains("share no rotation axis"), "refused by name: {e}");
}
