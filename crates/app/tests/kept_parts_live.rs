//! #1628: an operation judged against parts BESIDE its source (#1607's
//! `keep_law::parts_in_stock`, #1621's `keep_law::neighbour_parts`) is
//! not made Stale when one of those parts moves, is added, or stops
//! being workholding after generation - those parts are not stamp
//! inputs (the stamp contracts stand). Before #1628, on a turned stock
//! (the program sim refuses to judge) the post read the op's cached,
//! clean verdict and the program went out into the moved part. Now the
//! post gate judges those parts again, live (`operation::kept_rejudge`),
//! and refuses by name: "<op> now cuts part '<part>'".
//!
//! Provenance: our own fixtures (`pocket_support::block_with_pocket`,
//! the #1585 block, beside plain boxes) over the solid law; no vendor
//! material consulted.
//!
//! Gate map: covers app::operation::kept_rejudge, app::operation::keep_law
//! (kept_parts_named), app::posting_sim (kept_moved), app::posting.

mod area_support;
mod pocket_support;

use area_support::*;
use camcraft_app::commands::{GenerateOperation, MoveModel, TagFixture, UntagFixture};
use camcraft_app::geo2d::Point2;
use camcraft_app::geometry::{Mesh, Triangle};
use camcraft_app::operation::OpStatus;
use camcraft_app::posting;

/// A 6 mm end mill.
const CUTTER: &str = "EM-0600-4FL";
const RP: f64 = 15.0;
const FLOOR: f64 = 4.0;
const TOP: f64 = 8.0;

/// The 40 x 40 x 8 block with a radius-[`RP`] round pocket sunk to
/// [`FLOOR`], its centre at (`cx`, 0).
fn pocket_block(cx: f64) -> Mesh {
    let ring: Vec<(f64, f64)> = (0..128)
        .map(|i| {
            let a = std::f64::consts::TAU * f64::from(i) / 128.0;
            (RP * a.cos(), RP * a.sin())
        })
        .collect();
    let m = pocket_support::block_with_pocket(20.0, 20.0, &ring, FLOOR, TOP);
    let at = |p: Point3| Point3::new(p.x + cx, p.y, p.z);
    Mesh::from_facets(m.facets.iter().map(|t| Triangle::new(at(t.a), at(t.b), at(t.c))).collect())
}

fn boxed(lo: Point3, hi: Point3) -> Mesh {
    Mesh::from_facets(cube(lo, hi).facets.to_vec())
}

/// The tool, then `models`, in an 80 x 80 x 8 box stock on the origin,
/// turned [`OBLIQUE`] about Z when `turned` (the same footprint within
/// 1e-4 mm; a turn the program sim refuses to model). #2177: a quarter
/// turn is no such device - the sim judges a square-turned box.
fn job(models: Vec<(&str, Mesh)>, turned: bool) -> (World, History) {
    let world = new_world();
    let mut h = History::default();
    let tool = catalog().into_iter().find(|(n, _)| *n == CUTTER).map(|(_, t)| t).expect("tool");
    h.execute(&world, AddTool::new(&world, CUTTER, tool).expect("tool"));
    for (name, mesh) in models {
        h.execute(&world, ImportModel::from_mesh(name, mesh));
    }
    let at = Location { pos: Point3::new(0.0, 0.0, 0.0), rot: stock_turn(turned) };
    h.execute(&world, SetStock::new(&world, StockShape::Box { dx: 80.0, dy: 80.0, dz: TOP }, at).expect("stock"));
    (world, h)
}

/// A Trace round its source's outline, 0.3 mm deep.
fn trace() -> OpParams {
    let (clearance_z, top_z, bottom_z) = (Height::fixed(25.0), Height::fixed(TOP), Height::fixed(7.7));
    OpParams::Trace { clearance_z, retract_z: None, top_z, bottom_z, stepdown: 10.0, feed: 800.0, plunge_feed: 200.0, climb: true, tilt: None, picks: Default::default() }
}

/// A 10 mm Slot along X from -6 to 6 at y 0, from the top to the pocket floor.
fn slot() -> OpParams {
    OpParams::Slot {
        links: camcraft_app::links::Links::PLANAR,
        a: Point2::new(-6.0, 0.0),
        b: Point2::new(6.0, 0.0),
        width: 10.0,
        stock_to_leave: 0.0,
        clearance_z: Height::fixed(25.0),
        retract_z: None,
        top_z: Height::fixed(TOP),
        bottom_z: Height::fixed(FLOOR),
        stepdown: 4.0,
        stepover: 3.0,
        feed: 800.0,
        plunge_feed: 200.0,
        climb: true,
        entry: Entry3D::Plunge,
        helix_radius: 1.0, blend_turns: false, blend_radius: None,
        tilt: None,
        extend_start: 0.0,
        extend_end: 0.0,
        picks: Default::default(),
    }
}

/// Create and generate `name`: clean and Ok, or the test is void.
fn generated_clean(world: &World, h: &mut History, name: &str, src: SrcRef, params: OpParams) {
    h.execute(world, CreateOperation::new(world, name, CUTTER, src, params).expect("create"));
    h.execute(world, GenerateOperation::new(world, name).expect("generate"));
    let e = model::find_by_name(world, name).expect("op");
    let tp = model::toolpath_of(world, &e).expect("cached path");
    assert!(!tp.path.moves.is_empty(), "{name}: the cut was generated");
    let blocking: Vec<_> = tp.warnings.iter().filter(|w| w.blocks_post()).collect();
    assert!(blocking.is_empty(), "{name}: a clean generation: {blocking:?}");
    assert_ok(world, name);
}

/// The op is still fresh: the edit was not a stamp input (the contract).
fn assert_ok(world: &World, name: &str) {
    let e = model::find_by_name(world, name).expect("op");
    assert_eq!(operation::status_of(world, &e), OpStatus::Ok, "{name}: still Ok - the kept parts are not stamp inputs");
}

fn nc(tag: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!("camcraft-kept-parts-live-{tag}-{}.nc", std::process::id()))
}

/// The post is refused, naming `op` and `part`, and nothing is written.
fn assert_refused_by_name(world: &World, tag: &str, op: &str, part: &str) {
    let out = nc(tag);
    let _ = std::fs::remove_file(&out);
    let err = posting::post_setup(world, &out).expect_err("a cut into a kept part never posts");
    assert!(err.contains("post blocked"), "{tag}: {err}");
    assert!(err.contains(&format!("{op} now cuts part '{part}'")), "{tag}: refused by name: {err}");
    assert!(err.contains("regenerate"), "{tag}: {err}");
    assert!(oblique_refused(world), "{tag}: the device holds: the program sim does not judge this stock");
    assert!(!out.exists(), "{tag}: blocked output is never written");
}

fn assert_posts(world: &World, tag: &str) {
    let out = nc(tag);
    let _ = std::fs::remove_file(&out);
    posting::post_setup(world, &out).unwrap_or_else(|e| panic!("{tag}: posts: {e}"));
    assert!(out.exists(), "{tag}: the program was written");
    let _ = std::fs::remove_file(&out);
}

/// Part B ("part") on x [0, 40] and part A ("other") 5 mm off its side.
fn two_parts(turned: bool) -> (World, History) {
    let a = boxed(Point3::new(-40.0, -20.0, 0.0), Point3::new(-5.0, 20.0, TOP));
    job(vec![("part", pocket_block(20.0)), ("other", a)], turned)
}

#[test]
fn a_neighbour_moved_into_a_part_source_cut_on_a_turned_stock_is_refused_by_name() {
    // A Trace round B, clean with A 5 mm off; A then moved 4.3 mm toward
    // B (0.7 mm off: the flank 2.3 mm into A). The op stays Ok; before
    // #1628 the turned stock's post went out into A.
    let (world, mut h) = two_parts(true);
    generated_clean(&world, &mut h, "N1", SrcRef::Entity("part".into()), trace());
    assert_posts(&world, "neighbour-before");
    let near = Location { pos: Point3::new(4.3, 0.0, 0.0), ..Location::default() };
    h.execute(&world, MoveModel::new(&world, "other", near).expect("move A"));
    assert_ok(&world, "N1");
    assert_refused_by_name(&world, "neighbour-moved", "N1", "other");
    // Moved away again, it posts: the re-judge reads the parts as they stand.
    h.execute(&world, MoveModel::new(&world, "other", Location { pos: Point3::new(-10.0, 0.0, 0.0), ..Location::default() }).expect("move A away"));
    assert_ok(&world, "N1");
    assert_posts(&world, "neighbour-away");
}

#[test]
fn a_part_added_into_a_stock_source_cut_on_a_turned_stock_is_refused_by_name() {
    // A Slot off the stock in the block's own pocket, to its floor:
    // clean. A boss then added in the pocket under the slot - a part the
    // stock-source law keeps - leaves the op Ok; the post refuses by name.
    let (world, mut h) = job(vec![("part", pocket_block(0.0))], true);
    generated_clean(&world, &mut h, "S1", SrcRef::Stock, slot());
    assert_posts(&world, "added-before");
    h.execute(&world, ImportModel::from_mesh("boss", boxed(Point3::new(-3.0, -3.0, 0.0), Point3::new(3.0, 3.0, FLOOR + 2.0))));
    assert_ok(&world, "S1");
    assert_refused_by_name(&world, "added", "S1", "boss");
}

#[test]
fn a_fixture_untagged_then_moved_into_a_part_source_cut_on_a_turned_stock_is_refused_by_name() {
    // A clamp block 5 mm off B, tagged as a fixture: not a part, so the
    // Trace round B is generated against B alone. Untagged, it is a part
    // the op keeps (workholding never stales a path, the M2.3 law: the op
    // stays Ok); moved 0.7 mm off B, the trace's flank enters it and the
    // live re-judge refuses by name.
    let clamp = boxed(Point3::new(-40.0, -20.0, 0.0), Point3::new(-5.0, 20.0, TOP));
    let (world, mut h) = job(vec![("part", pocket_block(20.0)), ("clamp", clamp)], true);
    h.execute(&world, TagFixture::new(&world, "clamp", None).expect("tag"));
    generated_clean(&world, &mut h, "N1", SrcRef::Entity("part".into()), trace());
    h.execute(&world, UntagFixture::new(&world, "clamp").expect("untag"));
    assert_ok(&world, "N1");
    assert_posts(&world, "untagged-clear");
    let near = Location { pos: Point3::new(4.3, 0.0, 0.0), ..Location::default() };
    h.execute(&world, MoveModel::new(&world, "clamp", near).expect("move the clamp"));
    assert_ok(&world, "N1");
    assert_refused_by_name(&world, "untagged-moved", "N1", "clamp");
}

#[test]
fn a_part_moved_into_a_stock_source_cut_on_a_turned_stock_never_posts() {
    // The same clean Slot in the block's pocket; the block then moved 8 mm
    // along X, so the slot's end runs into the pocket wall. Never posted:
    // moving the part moves another of the op's stamp inputs, so the op
    // reads Stale and the post says regenerate (#1628 found it so; the
    // live re-judge stands behind it should that input ever go).
    let (world, mut h) = job(vec![("part", pocket_block(0.0))], true);
    generated_clean(&world, &mut h, "S1", SrcRef::Stock, slot());
    let moved = Location { pos: Point3::new(8.0, 0.0, 0.0), ..Location::default() };
    h.execute(&world, MoveModel::new(&world, "part", moved).expect("move the part"));
    let out = nc("moved");
    let _ = std::fs::remove_file(&out);
    let err = posting::post_setup(&world, &out).expect_err("a cut into the moved part never posts");
    assert!(err.contains("S1") && err.contains("regenerate"), "{err}");
    assert!(!out.exists(), "blocked output is never written");
}
