//! #1603: the pocket family (2D Pocket, 2D Adaptive, 2D Offset) keeps
//! the part, and says so on the operation (`DepthLaw::Keep`,
//! `operation::keep_law`). A whole-model source is cleared AROUND (#12),
//! and since #1554 a stock source is too (the stock minus the setup's
//! parts); an area offset or extension that carries the boundary into
//! the part, from either source, is a blocking
//! `Warning::Gouge` on the operation and the post is refused by name on
//! its moves - with the stock aligned and with it turned (where the
//! program sim refuses to judge). Before #1603 these carried
//! `DepthLaw::None`: 0 findings on the op, and with the stock turned the
//! post went out 4-7 mm into the part. A legal pocket or adaptive with
//! the default area controls stays clean and posts on both stocks.
//!
//! Provenance: our own fixtures (the #12 block with a boss) over the
//! sim crate's solid law; no vendor material consulted.
//!
//! Gate map: covers app::operation::keep_law, app::operation::generate
//! (depth_law_of, the Keep arm), app::operation::job (keep_parts),
//! app::posting, app::posting_sim (uncovered), app::mcp (add_operation,
//! set_operation_area, verify).

mod area_support;
mod pocket_support;

use area_support::*;
use camcraft_app::commands::GenerateOperation;
use camcraft_app::geometry::{Mesh, Triangle};
use camcraft_app::mcp::Server;
use camcraft_app::posting;
use camcraft_app::verify::Warning;
use serde_json::{json, Value};

/// A 30 x 30 x [0, 10] block with a 12 x 12 boss to Z 20 (the #12
/// fixture). Outward-wound.
fn block_with_boss() -> Mesh {
    let quad = |a: Point3, b: Point3, c: Point3, d: Point3, f: &mut Vec<Triangle>| {
        f.push(Triangle::new(a, b, c));
        f.push(Triangle::new(a, c, d));
    };
    let mut f = Vec::new();
    let block = cube(Point3::new(-15.0, -15.0, 0.0), Point3::new(15.0, 15.0, 10.0));
    f.extend(block.facets[0..2].iter().cloned()); // bottom
    f.extend(block.facets[4..12].iter().cloned()); // walls
    let (o, i) = (15.0, 6.0);
    let oo = [(-o, -o), (o, -o), (o, o), (-o, o)];
    let ii = [(-i, -i), (i, -i), (i, i), (-i, i)];
    let p = |(x, y): (f64, f64)| Point3::new(x, y, 10.0);
    for k in 0..4 {
        let n = (k + 1) % 4;
        quad(p(oo[k]), p(oo[n]), p(ii[n]), p(ii[k]), &mut f); // shoulder
    }
    let boss = cube(Point3::new(-i, -i, 10.0), Point3::new(i, i, 20.0));
    f.extend(boss.facets[2..12].iter().cloned()); // boss top + walls
    Mesh::from_facets(f)
}

/// The stock's side; it stands on [-25, 25]^2 x [0, 20] either way.
const SIDE: f64 = 50.0;

/// `part` (when given) in the 50 x 50 x 20 stock: axis-aligned, or the
/// same box turned [`OBLIQUE`] about Z (the same footprint within 1e-4
/// mm, but a turn the program sim refuses to model). #2177: a quarter
/// turn is no such device - the sim judges a square-turned box.
fn job(part: Option<Mesh>, rotated: bool) -> (World, History) {
    let world = new_world();
    let mut h = History::default();
    if let Some(m) = part {
        h.execute(&world, ImportModel::from_mesh("part", m));
    }
    // A box stock stands centred on its place, so a turn about Z leaves
    // the footprint where it was.
    let at = Location { pos: Point3::new(0.0, 0.0, 0.0), rot: stock_turn(rotated) };
    h.execute(&world, SetStock::new(&world, StockShape::Box { dx: SIDE, dy: SIDE, dz: 20.0 }, at).expect("stock"));
    let ((lo, hi), half) = (stock_box(&world), SIDE / 2.0);
    // The oblique turn moves the corners by 25 * sqrt(2) * 1e-6 mm.
    let on = |v: f64, want: f64| (v - want).abs() < 1e-4;
    assert!(on(lo.x, -half) && on(lo.y, -half) && on(hi.x, half) && on(hi.y, half), "stock at {lo:?}..{hi:?}");
    let tool = catalog().into_iter().find(|(n, _)| *n == TOOL).map(|(_, t)| t).expect("catalog");
    h.execute(&world, AddTool::new(&world, TOOL, tool).expect("tool"));
    (world, h)
}

fn retyped(to: OpType, top: f64, bottom: f64) -> OpParams {
    let mut p = operation::retype_params(&pocket(top, bottom), to as usize);
    match &mut p {
        OpParams::Adaptive2D { top_z, bottom_z, stepdown, stepover, .. }
        | OpParams::Offset2D { top_z, bottom_z, stepdown, stepover, .. } => {
            *top_z = Height::fixed(top);
            *bottom_z = Height::fixed(bottom);
            *stepdown = 5.0;
            *stepover = 4.0;
        }
        _ => {}
    }
    assert_eq!(p.kind(), to);
    p
}

/// A kind's label and its params.
type Kind = (&'static str, OpParams);

fn pocket_k(top: f64, bottom: f64) -> Kind {
    ("2D Pocket", pocket(top, bottom))
}

fn adaptive_k(top: f64, bottom: f64) -> Kind {
    ("2D Adaptive", retyped(OpType::Adaptive2D, top, bottom))
}

fn offset_k(top: f64, bottom: f64) -> Kind {
    ("2D Offset", retyped(OpType::Offset2D, top, bottom))
}

/// The two kinds #1603 names, cut from `top` to `bottom`.
fn kinds(top: f64, bottom: f64) -> [Kind; 2] {
    [pocket_k(top, bottom), adaptive_k(top, bottom)]
}

/// Create "P1" with `area`, generate it; its cached warnings.
fn generated(world: &World, h: &mut History, src: SrcRef, params: OpParams, area: AreaControls) -> Vec<Warning> {
    h.execute(world, CreateOperation::new(world, "P1", TOOL, src, params.clone()).expect("create"));
    if !area.is_default() {
        h.execute(world, EditOperationParams::with_area(world, "P1", params, area).expect("area"));
    }
    h.execute(world, GenerateOperation::new(world, "P1").expect("generate"));
    let e = model::find_by_name(world, "P1").expect("op");
    let tp = model::toolpath_of(world, &e).expect("cached path");
    assert!(!tp.path.moves.is_empty(), "the area was generated");
    tp.warnings.clone()
}

fn gouges(w: &[Warning]) -> usize {
    w.iter().filter(|w| matches!(w, Warning::Gouge { .. })).count()
}

fn nc(tag: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!("camcraft-keep-law-{tag}-{}.nc", std::process::id()))
}

/// The post is refused and nothing is written. `by_op`: the refusal
/// names the operation's own gouge (where the program sim judges too,
/// its clause may be the one the message leads with).
fn assert_post_refused(world: &World, tag: &str, by_op: bool) {
    let out = nc(tag);
    let _ = std::fs::remove_file(&out);
    let err = posting::post_setup(world, &out).expect_err("a gouge blocks the post");
    assert!(err.contains("post blocked"), "{tag}: {err}");
    if by_op {
        assert!(err.contains("P1: move") && err.contains("cuts past the finish surface"), "{tag}: {err}");
        assert!(oblique_refused(world), "{tag}: the device holds: the program sim does not judge this stock");
    }
    assert!(!out.exists(), "{tag}: blocked output is never written");
}

fn assert_posts(world: &World, tag: &str) {
    let out = nc(tag);
    let _ = std::fs::remove_file(&out);
    posting::post_setup(world, &out).unwrap_or_else(|e| panic!("{tag}: a legal program posts: {e}"));
    assert!(out.exists(), "{tag}: the program was written");
    let _ = std::fs::remove_file(&out);
}

fn whole() -> SrcRef {
    SrcRef::Entity("part".into())
}

// #1715: each test below was one loop over the kinds and the two stocks
// (5 of the 8 tests ran over 60 s in debug). Each (kind, stock) case is
// its own test now, in a module with the old test's name, so libtest
// runs the cases in parallel. Each case builds its own world.

/// A whole-model `area` case: convicted on the op and never posted.
fn refused((label, params): Kind, rotated: bool, area: &AreaControls, tag: &str) {
    let (world, mut h) = job(Some(block_with_boss()), rotated);
    let warns = generated(&world, &mut h, whole(), params, area.clone());
    assert!(gouges(&warns) > 0, "{label} {tag} rotated {rotated}: convicted on the op: {warns:?}");
    assert_post_refused(&world, &format!("{tag}-{label}-{rotated}"), rotated);
}

/// Offset +3: the boundary grows 3 mm into the part's own section.
fn offset_3() -> AreaControls {
    AreaControls { offset: 3.0, ..Default::default() }
}

// Offset +3 into the part. Before #1603: 0 findings, and it POSTED with
// the stock turned.
mod a_whole_model_area_offset_into_the_part_gouges_and_never_posts {
    use super::*;

    #[test]
    fn pocket_aligned() {
        refused(pocket_k(20.0, 5.0), false, &offset_3(), "offset");
    }

    #[test]
    fn pocket_turned() {
        refused(pocket_k(20.0, 5.0), true, &offset_3(), "offset");
    }

    #[test]
    fn adaptive_aligned() {
        refused(adaptive_k(20.0, 5.0), false, &offset_3(), "offset");
    }

    #[test]
    fn adaptive_turned() {
        refused(adaptive_k(20.0, 5.0), true, &offset_3(), "offset");
    }
}

// Extension only grows past the exterior stock outline (#2438): the part's
// standing material stays in `blocked`, so extension clears past the stock
// edges without gouging the block or boss and posts on both stocks.
mod a_whole_model_extension_keeps_the_part_and_posts {
    use super::*;

    fn extension_5() -> AreaControls {
        AreaControls { extension: 5.0, ..Default::default() }
    }

    fn stays_clear((label, params): Kind, rotated: bool) {
        let (world, mut h) = job(Some(block_with_boss()), rotated);
        let warns = generated(&world, &mut h, whole(), params, extension_5());
        let blocking: Vec<_> = warns.iter().filter(|w| w.blocks_post()).collect();
        assert!(blocking.is_empty(), "{label} extension rotated {rotated}: {blocking:?}");
        assert_posts(&world, &format!("ext-legal-{label}-{rotated}"));
    }

    #[test]
    fn pocket_aligned() {
        stays_clear(pocket_k(20.0, 5.0), false);
    }

    #[test]
    fn pocket_turned() {
        stays_clear(pocket_k(20.0, 5.0), true);
    }

    #[test]
    fn adaptive_aligned() {
        stays_clear(adaptive_k(20.0, 5.0), false);
    }

    #[test]
    fn adaptive_turned() {
        stays_clear(adaptive_k(20.0, 5.0), true);
    }
}

/// Default area controls: every ring stands outside the part. A turned
/// stock must not refuse it - that would be a false refusal.
fn legal_whole_model_posts((label, params): Kind, rotated: bool) {
    let (world, mut h) = job(Some(block_with_boss()), rotated);
    let warns = generated(&world, &mut h, whole(), params, AreaControls::default());
    let blocking: Vec<_> = warns.iter().filter(|w| w.blocks_post()).collect();
    assert!(blocking.is_empty(), "{label} rotated {rotated}: {blocking:?}");
    assert_posts(&world, &format!("legal-{label}-{rotated}"));
}

// 2D Adaptive cuts two levels, the last on the block's top: around the
// boss and tangent to the shoulder (the Z5 ring round the block costs a
// debug build a minute per run and adds no other contact).
mod a_legal_whole_model_clearing_stays_clean_and_posts_on_both_stocks {
    use super::*;

    #[test]
    fn pocket_aligned() {
        legal_whole_model_posts(pocket_k(20.0, 5.0), false);
    }

    #[test]
    fn pocket_turned() {
        legal_whole_model_posts(pocket_k(20.0, 5.0), true);
    }

    #[test]
    fn adaptive_aligned() {
        legal_whole_model_posts(adaptive_k(20.0, 10.0), false);
    }

    #[test]
    fn adaptive_turned() {
        legal_whole_model_posts(adaptive_k(20.0, 10.0), true);
    }

    #[test]
    fn offset_aligned() {
        legal_whole_model_posts(offset_k(20.0, 5.0), false);
    }

    #[test]
    fn offset_turned() {
        legal_whole_model_posts(offset_k(20.0, 5.0), true);
    }
}

#[path = "keep_law/stock_source.rs"]
mod stock_source;

// ---- MCP parity -----------------------------------------------------

#[path = "keep_law/mcp.rs"]
mod mcp;
