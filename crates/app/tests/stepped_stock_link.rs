//! #1095: the rapid-in-stock law's lateral rapids over a stepped stock.
//! A drill's link from hole to hole runs at its retract plane, under the
//! clearance plane. A part-sourced drill under a "from model" casting
//! (kept in the tree as raw stock only) -
//! solid to Z 20 with a boss to Z 30 elsewhere: a link at Z 22 across
//! the low part was
//! convicted as if the boss stood under it (the column law caps every
//! column at the stock's highest point), and the post was blocked for a
//! legal program. These pin the law re-reading the link against the
//! stock really under it (`verify::acquit_by_stock_top`): the link over
//! the low part passes and posts, and the same link across the boss, or
//! with the boss under the tool's rim along the way, is still convicted
//! and still blocks the post.
//!
//! Provenance: our own fixture (#1091's stepped casting,
//! `stepped_stock_rapid.rs`), specced from the Prime Directive and the
//! shop rule "never rapid into uncut stock"; no vendor material.
//!
//! Gate map (#1095): covers app::operation (generate's verify wiring,
//! the Drill arm, stock_top acquittal), sim::verify (rapids, stock_top),
//! app::posting (the gate).
//! Lane shape: all tests here run in seconds.

use camcraft_app::cl::MoveKind;
use camcraft_app::commands::{AddTool, CreateOperation, GenerateOperation, History, ImportModel, SetStock, SetStockSourceOnly};
use camcraft_app::drilling::{CycleChoice, Hole};
use camcraft_app::geo2d::Point2;
use camcraft_app::geometry::{Mesh, Point3, Triangle, cube};
use camcraft_app::model;
use camcraft_app::operation::{Height, OpParams, SrcRef};
use camcraft_app::posting;
use camcraft_app::schema::Location;
use camcraft_app::stock::StockShape;
use camcraft_app::tools::catalog;
use camcraft_app::verify::Warning;
use camcraft_app::world::new_world;

/// The casting's low top, and the boss's.
const LOW: f64 = 20.0;
const BOSS: f64 = 30.0;
/// The drill's retract plane: 2 mm over the low top, under the boss.
const RETRACT: f64 = 22.0;
/// The two holes, on the line y = -5 inside the part's open cavity.
const Y: f64 = -5.0;
const XS: [f64; 2] = [-5.0, 5.0];

fn quad(a: Point3, b: Point3, c: Point3, d: Point3, f: &mut Vec<Triangle>) {
    f.push(Triangle::new(a, b, c));
    f.push(Triangle::new(a, c, d));
}

/// An outward-wound casting: a 60 x 60 slab Z -10..20 with a boss over
/// `[x0, x1] x [y0, y1]` up to Z 30 (inside the slab's outline). Closed.
fn casting([x0, x1, y0, y1]: [f64; 4]) -> Mesh {
    let slab = cube(Point3::new(-30.0, -30.0, -10.0), Point3::new(30.0, 30.0, LOW));
    let boss = cube(Point3::new(x0, y0, LOW), Point3::new(x1, y1, BOSS));
    let mut f: Vec<Triangle> = slab.facets[0..2].to_vec();
    f.extend(slab.facets[4..12].iter().cloned());
    let o = [(-30.0, -30.0), (30.0, -30.0), (30.0, 30.0), (-30.0, 30.0)];
    let i = [(x0, y0), (x1, y0), (x1, y1), (x0, y1)];
    let p = |(x, y): (f64, f64)| Point3::new(x, y, LOW);
    for k in 0..4 {
        let n = (k + 1) % 4;
        quad(p(o[k]), p(o[n]), p(i[n]), p(i[k]), &mut f);
    }
    f.extend(boss.facets[2..12].iter().cloned());
    Mesh::from_facets(f)
}

fn flip(t: &Triangle) -> Triangle {
    Triangle::new(t.a, t.c, t.b)
}

/// `stepped_stock_rapid.rs`'s part: a 40 x 40 x [0, 20] block with a
/// 20 x 20 cavity from its top to Z 10 - the holes are drilled in its air.
fn part() -> Mesh {
    let mut f = Vec::new();
    let outer = cube(Point3::new(-20.0, -20.0, 0.0), Point3::new(20.0, 20.0, LOW));
    f.extend(outer.facets[4..12].iter().cloned());
    let o = [(-20.0, -20.0), (20.0, -20.0), (20.0, 20.0), (-20.0, 20.0)];
    let i = [(-10.0, -10.0), (10.0, -10.0), (10.0, 10.0), (-10.0, 10.0)];
    let p = |(x, y): (f64, f64)| Point3::new(x, y, LOW);
    for k in 0..4 {
        let n = (k + 1) % 4;
        quad(p(o[k]), p(o[n]), p(i[n]), p(i[k]), &mut f);
    }
    let inner = cube(Point3::new(-10.0, -10.0, 10.0), Point3::new(10.0, 10.0, LOW));
    f.extend(inner.facets[4..12].iter().map(flip));
    f.extend(outer.facets[0..2].iter().cloned());
    f.extend(inner.facets[0..2].iter().map(flip));
    Mesh::from_facets(f)
}

fn drill() -> OpParams {
    let hole = |x: f64| Hole { xy: Point2::new(x, Y), radius: 2.0, entry_z: LOW, floor_z: 12.0, through: false, stepped: false };
    OpParams::Drill {
        clearance_z: Height::fixed(40.0),
        retract_z: Some(Height::fixed(RETRACT)),
        top_z: Height::fixed(LOW),
        bottom_z: None,
        cycle: CycleChoice::Auto,
        peck: 0.0,
        dwell: 0.0,
        feed: 130.0,
        plunge_feed: 100.0,
        holes: XS.iter().map(|&x| hole(x)).collect(),
        cross: Vec::new(),
    }
}

/// The drill from the part under the casting with its boss over `boss`:
/// the lateral rapid from hole to hole (index, its low Z), the warnings,
/// and the post.
fn run(boss: [f64; 4]) -> ((usize, f64), Vec<Warning>, Result<(), String>) {
    let world = new_world();
    let mut h = History::default();
    h.execute(&world, ImportModel::from_mesh("casting", casting(boss)));
    h.execute(&world, ImportModel::from_mesh("part", part()));
    let shape = StockShape::FromModel { source: "casting".into() };
    h.execute(&world, SetStock::new(&world, shape, Location::default()).expect("stock"));
    // The casting stays in the tree, marked raw stock only (#1094): a
    // from-model stock whose source is gone has an unknown chord and is
    // never re-read (#1056), so deleting it would prove nothing here.
    h.execute(&world, SetStockSourceOnly::new(&world, true).expect("raw stock only"));
    let tool = catalog().into_iter().find(|(n, _)| *n == "DR-0400").map(|(_, t)| t).expect("catalog");
    h.execute(&world, AddTool::new(&world, "T", tool).expect("tool"));
    h.execute(&world, CreateOperation::new(&world, "Op1", "T", SrcRef::Entity("part".into()), drill()).expect("create op"));
    h.execute(&world, GenerateOperation::new(&world, "Op1").expect("generate op"));
    let e = model::find_by_name(&world, "Op1").expect("op");
    let cache = model::toolpath_of(&world, &e).expect("toolpath");
    let moves = &cache.path.moves;
    let link = moves
        .iter()
        .position(|m| m.kind == MoveKind::Rapid && (m.to.x - m.from.x).abs() > 1.0)
        .and_then(|i| moves.get(i).map(|m| (i, m.from.z.min(m.to.z))))
        .unwrap_or_else(|| panic!("no lateral link: {moves:?}"));
    static RUN: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let n = RUN.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let out = std::env::temp_dir().join(format!("camcraft-1095-{n}-{}.nc", std::process::id()));
    let _ = std::fs::remove_file(&out);
    let posted = posting::post_setup(&world, &out).map(|_| ());
    let _ = std::fs::remove_file(&out);
    (link, cache.warnings.clone(), posted)
}

#[test]
fn a_link_over_the_low_part_of_a_stepped_stock_passes_and_posts() {
    // The boss at y 3..13: the link at y -5 passes 8 mm from it.
    let ((at, z), warns, posted) = run([-2.0, 2.0, 3.0, 13.0]);
    assert!(z > LOW && z < BOSS, "the fixture's link must run between the two tops, not at Z {z}");
    assert!(!warns.contains(&Warning::RapidInStock { at }), "{warns:?}");
    assert!(!warns.iter().any(|w| matches!(w, Warning::RapidInStock { .. })), "{warns:?}");
    assert!(posted.is_ok(), "{posted:?}");
}

fn convicted_and_blocked(boss: [f64; 4], what: &str) {
    let ((at, z), warns, posted) = run(boss);
    assert!(z < BOSS, "{what}: the link must run under the boss's top, not at Z {z}");
    assert!(warns.contains(&Warning::RapidInStock { at }), "{what}: the link went unnamed: {warns:?}");
    match posted {
        Ok(()) => panic!("{what}: a program linking through the boss posted"),
        Err(e) => assert!(e.contains("post blocked"), "{what}: {e}"),
    }
}

#[test]
fn the_same_link_across_the_boss_is_convicted() {
    // The boss stands between the two holes, across the link's line.
    convicted_and_blocked([-2.0, 2.0, -10.0, 0.0], "boss across the link");
}

#[test]
fn a_boss_under_the_tool_rim_along_the_link_is_convicted() {
    // The boss's edge 1 mm beside the link's centre line (the drill's
    // radius is 2), between the holes: under the rim, never the centre.
    convicted_and_blocked([-2.0, 2.0, Y + 1.0, Y + 11.0], "boss under the rim");
}
