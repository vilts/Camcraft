//! #1607: a 2D operation whose source is the STOCK (or a pick off it)
//! answers for the parts standing in that stock on the operation itself
//! (`operation::keep_law::parts_in_stock`). Before #1607 only the pocket
//! family did (#1603): `depth_law_of` returned `None` for a source that
//! is not the part, so an Engrave, Trace, Slot, Trochoidal slot,
//! Circular, Contour or Face cut off the stock and into the part had 0
//! findings - with the stock aligned the program sim held the post, with
//! it turned off the machine axes (the sim refuses to judge) the post
//! went out. (#2177: a quarter turn is square to the axes, and the sim
//! judges it.) Now Engrave
//! and Trace take the mark law against the setup's parts (and the sim
//! credits that mark), the other kinds the solid law: a cut into the
//! part is a blocking `Warning::Gouge` on the operation on both stocks;
//! a legal stock-source cut (a mark, a face above the part, a contour
//! round the stock, a slot in the part's own pocket) posts on both; a
//! setup with no part judges nothing, as before. A Face of a stock lower
//! than the part runs through the part and is refused the same (#1657).
//!
//! Provenance: our own fixtures (`pocket_support::block_with_pocket`,
//! the #1585 block) over the solid and mark laws and the column sim; no
//! vendor material consulted.
//!
//! Gate map: covers app::operation::keep_law (parts_in_stock,
//! is_stock_kept_kind), app::operation::mark_law (budget_of),
//! app::operation::generate (the stock-source keep and mark laws),
//! app::stock_sim (marks), app::posting, app::posting_sim. The MCP
//! parity is in `stock_source_law_mcp`.

mod area_support;
mod pocket_support;

use area_support::*;
use camcraft_app::commands::GenerateOperation;
use camcraft_app::geo2d::Point2;
use camcraft_app::geometry::{quat_from_yaw, Mesh};
use camcraft_app::posting;
use camcraft_app::schema::STOCK_NAME;
use camcraft_app::machining2d::TrochoidReturn;
use camcraft_app::verify::Warning;

/// A 6 mm end mill: the mark budget's cap is its 3 mm radius.
const CUTTER: &str = "EM-0600-4FL";
/// The round pocket's radius and its floor; the block is 40 x 40 x 8.
const RP: f64 = 15.0;
const FLOOR: f64 = 4.0;
const TOP: f64 = 8.0;

/// The 40 x 40 x 8 block with a radius-[`RP`] round pocket sunk to
/// [`FLOOR`] - the #1585 / #1593 / #1595 fixture.
fn pocket_block() -> Mesh {
    pocket_support::round_pocket_block(RP, FLOOR, TOP)
}

/// The block as "part" (when `part`) in a 40 x 40 x `dz` box stock on
/// [-20, 20]^2 x [0, dz]: axis-aligned, or the same box turned
/// [`OBLIQUE`] about Z (the same footprint within 1e-4 mm, but a turn the
/// program sim refuses to model).
fn job(part: bool, rotated: bool, dz: f64) -> (World, History) {
    job_turned(part, stock_turn(rotated), dz)
}

/// [`job`] with the stock turned by `rot` about its place.
fn job_turned(part: bool, rot: [f64; 4], dz: f64) -> (World, History) {
    let world = new_world();
    let mut h = History::default();
    let tool = catalog().into_iter().find(|(n, _)| *n == CUTTER).map(|(_, t)| t).expect("tool");
    h.execute(&world, AddTool::new(&world, CUTTER, tool).expect("tool"));
    if part {
        h.execute(&world, ImportModel::from_mesh("part", pocket_block()));
    }
    let at = Location { pos: Point3::new(0.0, 0.0, 0.0), rot };
    h.execute(&world, SetStock::new(&world, StockShape::Box { dx: 40.0, dy: 40.0, dz }, at).expect("stock"));
    let (lo, hi) = stock_box(&world);
    // The oblique turn moves the corners by 20 * sqrt(2) * 1e-6 mm.
    let on = |v: f64, want: f64| (v - want).abs() < 1e-4;
    let level = |v: f64, want: f64| (v - want).abs() < 1e-9;
    assert!(
        on(lo.x, -20.0) && on(lo.y, -20.0) && on(hi.x, 20.0) && on(hi.y, 20.0) && level(lo.z, 0.0) && level(hi.z, dz),
        "stock at {lo:?}..{hi:?}"
    );
    (world, h)
}

/// An Engrave (`trace` false) or a Trace, one level from `top` to `bottom`.
fn mark(trace: bool, top: f64, bottom: f64) -> OpParams {
    let (clearance_z, top_z, bottom_z) = (Height::fixed(25.0), Height::fixed(top), Height::fixed(bottom));
    if trace {
        OpParams::Trace { clearance_z, retract_z: None, top_z, bottom_z, stepdown: 10.0, feed: 800.0, plunge_feed: 200.0, climb: true, tilt: None }
    } else {
        OpParams::Engrave { clearance_z, retract_z: None, top_z, bottom_z, stepdown: 10.0, feed: 800.0, plunge_feed: 200.0, climb: true, tilt: None }
    }
}

/// A 10 mm Slot along X from `a` to `b`, from the top to `bottom`.
fn slot(a: f64, b: f64, bottom: f64) -> OpParams {
    OpParams::Slot {
        links: camcraft_app::links::Links::PLANAR,
        a: Point2::new(a, 0.0),
        b: Point2::new(b, 0.0),
        width: 10.0,
        stock_to_leave: 0.0,
        clearance_z: Height::fixed(25.0),
        retract_z: None,
        top_z: Height::fixed(TOP),
        bottom_z: Height::fixed(bottom),
        stepdown: 4.0,
        stepover: 3.0,
        feed: 800.0,
        plunge_feed: 200.0,
        climb: true,
        entry: Entry3D::Plunge,
        helix_radius: 1.0,
        tilt: None,
        extend_start: 0.0,
        extend_end: 0.0,
    }
}

/// A 10 mm Trochoidal slot along X from `a` to `b`, from the top to `bottom`.
fn trochoid(a: f64, b: f64, bottom: f64) -> OpParams {
    OpParams::TrochoidalSlot {
        a: Point2::new(a, 0.0),
        b: Point2::new(b, 0.0),
        width: 10.0,
        loop_width: None,
        step: 0.8,
        max_engagement: 90.0,
        ret: TrochoidReturn::Microlift { height: 0.3 },
        smoothing: 0.5,
        stock_to_leave: 0.0,
        clearance_z: Height::fixed(25.0),
        retract_z: Some(Height::fixed(15.0)),
        top_z: Height::fixed(TOP),
        bottom_z: Height::fixed(bottom),
        stepdown: 4.0,
        feed: 800.0,
        return_feed: 2000.0,
        plunge_feed: 200.0,
        climb: true,
    }
}

/// A Circular of `radius` about the pocket's centre, from the top to `bottom`.
fn circular(radius: f64, bottom: f64) -> OpParams {
    OpParams::Circular {
        links: camcraft_app::links::Links::PLANAR,
        center: Point2::new(0.0, 0.0),
        radius,
        vault: false,
        stepover: 2.0,
        stock_to_leave: 0.0,
        clearance_z: Height::fixed(25.0),
        retract_z: None,
        top_z: Height::fixed(TOP),
        bottom_z: Height::fixed(bottom),
        stepdown: 4.0,
        feed: 800.0,
        plunge_feed: 200.0,
        climb: true,
        entry: Entry3D::Plunge,
        helix_radius: 1.0,
        tilt: None,
    }
}

/// The stock's top face picked.
fn stock_top_pick(world: &World, z: f64) -> SrcRef {
    sel(STOCK_NAME, top_face(world, STOCK_NAME, z))
}

/// Create and generate "S1"; its cached warnings and notes.
fn generated(world: &World, h: &mut History, src: SrcRef, params: OpParams) -> (Vec<Warning>, Vec<String>) {
    h.execute(world, CreateOperation::new(world, "S1", CUTTER, src, params).expect("create"));
    h.execute(world, GenerateOperation::new(world, "S1").expect("generate"));
    let e = model::find_by_name(world, "S1").expect("op");
    let tp = model::toolpath_of(world, &e).expect("cached path");
    assert!(!tp.path.moves.is_empty(), "the cut was generated");
    (tp.warnings.clone(), tp.path.notes.clone())
}


fn gouges(w: &[Warning]) -> usize {
    w.iter().filter(|w| matches!(w, Warning::Gouge { .. })).count()
}

fn nc(tag: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!("camcraft-stock-source-law-{tag}-{}.nc", std::process::id()))
}

/// The post is refused and nothing is written. `by_op`: the refusal
/// names the operation's own gouge (on an aligned stock the program
/// sim's clause may lead).
fn assert_post_refused(world: &World, tag: &str, by_op: bool) {
    let out = nc(tag);
    let _ = std::fs::remove_file(&out);
    let err = posting::post_setup(world, &out).expect_err("a gouge blocks the post");
    assert!(err.contains("post blocked"), "{tag}: {err}");
    if by_op {
        assert!(err.contains("S1: move") && err.contains("cuts past the finish surface"), "{tag}: {err}");
        assert!(oblique_refused(world), "{tag}: the program sim judged the oblique stock");
    }
    assert!(!out.exists(), "{tag}: blocked output is never written");
}

fn assert_posts(world: &World, tag: &str) {
    let out = nc(tag);
    let _ = std::fs::remove_file(&out);
    posting::post_setup(world, &out).unwrap_or_else(|e| panic!("{tag}: a legal stock-source cut posts: {e}"));
    assert!(out.exists(), "{tag}: the program was written");
    let _ = std::fs::remove_file(&out);
}

/// A stock-source cut into the part: convicted on the op, never posted,
/// on both stocks.
fn refused_on_both_stocks(label: &str, params: &OpParams, picked: bool) {
    for rotated in [false, true] {
        let (world, mut h) = job(true, rotated, TOP);
        let src = if picked { stock_top_pick(&world, TOP) } else { SrcRef::Stock };
        let (warns, _) = generated(&world, &mut h, src, params.clone());
        assert!(gouges(&warns) > 0, "{label} picked {picked} rotated {rotated}: convicted on the op: {warns:?}");
        assert_post_refused(&world, &format!("{label}-{picked}-{rotated}"), rotated);
    }
}

#[test]
fn engrave_and_trace_off_the_stock_down_the_part_wall_gouge_and_never_post() {
    // The issue's probe: Top 8, Bottom 1 along the stock's outline - the
    // part's own outside wall - 7 mm under its top on a 3 mm budget.
    // Before #1607: 0 findings; aligned the sim held it, turned off the
    // axes it POSTED.
    for trace in [false, true] {
        for picked in [false, true] {
            refused_on_both_stocks(if trace { "trace" } else { "engrave" }, &mark(trace, TOP, 1.0), picked);
        }
        let (world, mut h) = job(true, true, TOP);
        let (_, notes) = generated(&world, &mut h, SrcRef::Stock, mark(trace, TOP, 1.0));
        assert!(notes.iter().any(|n| n.contains("deeper than a mark")), "the budget is named: {notes:?}");
    }
}

#[test]
fn the_typed_kinds_and_face_off_the_stock_into_the_part_gouge_and_never_post() {
    // Slot, Trochoidal slot and Circular inside the part's pocket, 3 mm
    // under its floor; a Face down to Z 5, 3 mm into the part's top.
    let cases = [
        ("slot", slot(-6.0, 6.0, 1.0)),
        ("trochoid", trochoid(-6.0, 6.0, 1.0)),
        ("circular", circular(8.0, 1.0)),
        ("face", face(5.0)),
    ];
    for (label, params) in &cases {
        refused_on_both_stocks(label, params, false);
    }
}

#[test]
fn a_face_of_a_stock_lower_than_the_part_cuts_the_part_and_never_posts() {
    // #1657: a 6 mm stock under the 8 mm part - the part stands 2 mm out
    // of the stock's top. A Face of the stock's top (Z 5.5) runs through
    // the part's body: a true gouge, convicted on the op, never posted.
    for rotated in [false, true] {
        let (world, mut h) = job(true, rotated, 6.0);
        let (warns, _) = generated(&world, &mut h, SrcRef::Stock, face(5.5));
        assert!(gouges(&warns) > 0, "rotated {rotated}: the face cuts the part: {warns:?}");
        assert_post_refused(&world, &format!("low-stock-face-{rotated}"), rotated);
    }
}

#[test]
fn a_quarter_turned_stock_is_judged_by_the_program_sim() {
    // #2177: the stock turned a quarter about Z is the same box square
    // to the axes. The program sim judges it: an Engrave off the stock
    // down the part wall, and a Face of a stock lower than the part, are
    // convicted by the sim; a legal stock-source mark is credited.
    let quarter = quat_from_yaw(std::f64::consts::FRAC_PI_2);
    for (label, dz, params) in [("engrave", TOP, mark(false, TOP, 1.0)), ("low-stock-face", 6.0, face(5.5))] {
        let (world, mut h) = job_turned(true, quarter, dz);
        let (warns, _) = generated(&world, &mut h, SrcRef::Stock, params);
        assert!(gouges(&warns) > 0, "{label}: convicted on the op: {warns:?}");
        let dev = camcraft_app::stock_sim::simulate_document(&world).expect("a quarter-turned box is judged").dev;
        assert!(dev.gouge_cells > 0 && dev.max_gouge > 0.5, "{label}: the sim convicts the cut: {} cell(s), worst {}", dev.gouge_cells, dev.max_gouge);
        assert_post_refused(&world, &format!("quarter-{label}"), false);
    }
    let (world, mut h) = job_turned(true, quarter, TOP);
    generated(&world, &mut h, SrcRef::Stock, mark(false, TOP, 7.7));
    let dev = camcraft_app::stock_sim::simulate_document(&world).expect("a quarter-turned box is judged").dev;
    assert_eq!(dev.gouge_cells, 0, "a legal mark is credited: worst {} at {:?}", dev.max_gouge, dev.max_gouge_at);
    assert_posts(&world, "quarter-legal-engrave");
}

#[test]
fn a_legal_stock_source_cut_stays_clean_and_posts_on_both_stocks() {
    // A mark 0.3 mm deep along the part's outline (credited by the sim),
    // a contour round the stock to Z 1 (tangent to the part's wall), a
    // slot in the part's own pocket to its floor, and a face
    // of the 2 mm the 10 mm stock stands over the part.
    let cases = [
        ("engrave", TOP, mark(false, TOP, 7.7)),
        ("trace", TOP, mark(true, TOP, 7.7)),
        ("contour", TOP, contour(TOP, 1.0, Side::Outside)),
        ("slot", TOP, slot(-6.0, 6.0, FLOOR)),
        ("face", 10.0, face(TOP)),
    ];
    for (label, dz, params) in &cases {
        for rotated in [false, true] {
            let (world, mut h) = job(true, rotated, *dz);
            let (warns, _) = generated(&world, &mut h, SrcRef::Stock, params.clone());
            let blocking: Vec<_> = warns.iter().filter(|w| w.blocks_post()).collect();
            assert!(blocking.is_empty(), "{label} rotated {rotated}: {blocking:?}");
            assert_posts(&world, &format!("legal-{label}-{rotated}"));
        }
    }
}

#[test]
fn a_stock_source_cut_with_no_part_in_the_setup_judges_nothing() {
    let cases = [
        ("engrave", mark(false, TOP, 1.0)),
        ("trace", mark(true, TOP, 1.0)),
        ("slot", slot(-6.0, 6.0, 1.0)),
        ("circular", circular(8.0, 1.0)),
        ("face", face(5.0)),
    ];
    for (label, params) in &cases {
        let (world, mut h) = job(false, false, TOP);
        let (warns, notes) = generated(&world, &mut h, SrcRef::Stock, params.clone());
        assert_eq!(gouges(&warns), 0, "{label}: no part, no part law: {warns:?}");
        assert!(!notes.iter().any(|n| n.contains("deeper than a mark")), "{label}: {notes:?}");
    }
}

#[test]
fn a_stock_source_cut_answers_for_every_part_in_the_stock() {
    // Two parts in one 80 x 40 x 8 stock: the pocket block on
    // [-20, 20]^2 and a plain 30 x 30 x 8 block "other" on x [25, 55].
    // A Slot and a Trochoidal slot off the stock (x 25..55 with the
    // kerf) stay clear of the first part and enter only the second: they
    // are convicted the same, because the law reads every part the setup
    // holds (`setup_align::parts_facets_in`), never one alone - and a
    // slot under the first part's pocket floor is convicted beside it. The
    // stock is also turned a half turn about Z (the same footprint).
    let cases = [("slot", slot(30.0, 50.0, 1.0)), ("trochoid", trochoid(30.0, 50.0, 1.0)), ("slot-in-first", slot(-6.0, 6.0, 1.0))];
    for (label, params) in &cases {
        for rotated in [false, true] {
            let world = new_world();
            let mut h = History::default();
            let tool = catalog().into_iter().find(|(n, _)| *n == CUTTER).map(|(_, t)| t).expect("tool");
            h.execute(&world, AddTool::new(&world, CUTTER, tool).expect("tool"));
            h.execute(&world, ImportModel::from_mesh("part", pocket_block()));
            let other = cube(Point3::new(25.0, -15.0, 0.0), Point3::new(55.0, 15.0, TOP));
            h.execute(&world, ImportModel::from_mesh("other", Mesh::from_facets(other.facets.to_vec())));
            let rot = if rotated { quat_from_yaw(std::f64::consts::PI) } else { Location::default().rot };
            let at = Location { pos: Point3::new(20.0, 0.0, 0.0), rot };
            h.execute(&world, SetStock::new(&world, StockShape::Box { dx: 80.0, dy: 40.0, dz: TOP }, at).expect("stock"));
            let (lo, hi) = stock_box(&world);
            assert!((lo.x + 20.0).abs() < 1e-9 && (hi.x - 60.0).abs() < 1e-9, "stock at {lo:?}..{hi:?}");
            let (warns, _) = generated(&world, &mut h, SrcRef::Stock, params.clone());
            assert!(gouges(&warns) > 0, "{label} rotated {rotated}: the second part is kept: {warns:?}");
            assert_post_refused(&world, &format!("two-parts-{label}-{rotated}"), false);
        }
    }
}
