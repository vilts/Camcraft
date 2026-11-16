//! #1659: a stock-source Face whose last row is tangent to the stock's
//! edge, above the stock top and with no part in the document, read a
//! false `Warning::Gouge` and the post refused the program. The stock's
//! outline carries vertices on its straight sides, on the rows' own
//! lines; the Boundary wall reading (`sim::verify::section`, no model)
//! excuses a row that crosses the rim, but counted only a crossing
//! through an edge's interior - a row run exactly through a vertex was
//! read as a move inside the region, tangent to the far side. Now a row
//! through a vertex from one side to the other crosses the rim too. A
//! real cut into a part near that edge is still refused.
//!
//! Provenance: the issue's own repro (catalog mill, box stock, Face on
//! the stock), over the section and solid laws; no vendor material.
//!
//! Gate map (#1659): covers app::operation::generate (the Face on a
//! stock source), app::operation::keep_law, app::posting.

mod area_support;

use area_support::*;
use camcraft_app::commands::GenerateOperation;
use camcraft_app::posting;
use camcraft_app::verify::Warning;

/// The issue's 4 mm two-flute mill.
const MILL: &str = "EM-0400-2FL";
/// The stock: 64 x 64 x 32 at (30, 30, 0), so x and y -2..62, z 0..32.
const CUT_Z: f64 = 34.0;

/// The issue's job: the mill, the box stock and (when given) a part.
fn job(part: Option<camcraft_app::geometry::Mesh>) -> (World, History) {
    let world = new_world();
    let mut h = History::default();
    let tool = catalog().into_iter().find(|(n, _)| *n == MILL).map(|(_, t)| t).expect("tool");
    h.execute(&world, AddTool::new(&world, MILL, tool).expect("tool"));
    if let Some(m) = part {
        h.execute(&world, ImportModel::from_mesh("part", m));
    }
    let at = Location { pos: Point3::new(30.0, 30.0, 0.0), ..Default::default() };
    h.execute(&world, SetStock::new(&world, StockShape::Box { dx: 64.0, dy: 64.0, dz: 32.0 }, at).expect("stock"));
    (world, h)
}

/// The issue's Face on the stock: stepover 3, cut Z 34, clearance Z 45;
/// its cached path's warnings and its last cutting row's move number.
fn face_on_stock(world: &World, h: &mut History) -> (Vec<Warning>, usize) {
    let mut p = face(CUT_Z);
    if let OpParams::Face { stepover, clearance_z, .. } = &mut p {
        *stepover = 3.0;
        *clearance_z = Height::fixed(45.0);
    }
    h.execute(world, CreateOperation::new(world, "F1", MILL, SrcRef::Stock, p).expect("create"));
    h.execute(world, GenerateOperation::new(world, "F1").expect("generate"));
    let e = model::find_by_name(world, "F1").expect("op");
    let tp = model::toolpath_of(world, &e).expect("cached path");
    let last = tp.path.moves.iter().rposition(|m| m.kind == MoveKind::Feed).expect("a cutting row");
    let row = &tp.path.moves[last];
    assert!((row.from.y - 60.0).abs() < 1e-9 && (row.to.y - 60.0).abs() < 1e-9, "the last row is y 60: {row:?}");
    (tp.warnings.clone(), last)
}

fn gouges(w: &[Warning]) -> Vec<usize> {
    w.iter().filter_map(|w| if let Warning::Gouge { at } = w { Some(*at) } else { None }).collect()
}

fn nc(tag: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!("camcraft-face-stock-edge-{tag}-{}.nc", std::process::id()))
}

#[test]
fn a_face_row_tangent_to_the_stock_edge_in_air_with_no_part_is_clean_and_posts() {
    let (world, mut h) = job(None);
    let (warns, last) = face_on_stock(&world, &mut h);
    assert_eq!(last, 42, "the issue's move 42");
    assert!(warns.iter().all(|w| !w.blocks_post()), "no part, no material in the way: {warns:?}");
    let out = nc("clean");
    let _ = std::fs::remove_file(&out);
    posting::post_setup(&world, &out).unwrap_or_else(|e| panic!("the clean Face posts: {e}"));
    assert!(out.exists(), "the program was written");
    let _ = std::fs::remove_file(&out);
}

/// The post is refused and nothing is written.
fn assert_refused(world: &World, tag: &str) {
    let out = nc(tag);
    let _ = std::fs::remove_file(&out);
    let err = posting::post_setup(world, &out).expect_err("a gouge blocks the post");
    assert!(err.contains("post blocked"), "{tag}: {err}");
    assert!(!out.exists(), "{tag}: blocked output is never written");
}

#[test]
fn the_same_face_over_a_part_whose_top_stands_above_cut_z_is_refused() {
    // A 40 x 40 part in the stock's middle, its top at 36: 2 mm over the
    // Face's cut Z.
    let part = cube(Point3::new(10.0, 10.0, 0.0), Point3::new(50.0, 50.0, 36.0));
    let (world, mut h) = job(Some(part));
    let (warns, _) = face_on_stock(&world, &mut h);
    assert!(!gouges(&warns).is_empty(), "a row through the part is a gouge: {warns:?}");
    assert_refused(&world, "part-top");
}

#[test]
fn a_tangent_last_row_0_02_mm_into_a_part_is_refused() {
    // A part whose -Y wall stands at y 61.98 along the stock's +Y edge,
    // its top over cut Z: the last row's edge (y 62) is 0.02 mm into it,
    // twice the gouge tolerance; every other row clears it by 3 mm.
    let part = cube(Point3::new(10.0, 61.98, 0.0), Point3::new(50.0, 66.0, 36.0));
    let (world, mut h) = job(Some(part));
    let (warns, last) = face_on_stock(&world, &mut h);
    assert_eq!(gouges(&warns), vec![last], "only the tangent row cuts into the part: {warns:?}");
    assert_refused(&world, "tangent-0.02");
}
