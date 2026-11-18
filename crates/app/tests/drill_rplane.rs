//! Issue #91: an axial drill whose R plane (Top of Part Z, where the
//! cycle's rapid ends) lies below the stock top over the hole - the
//! rapid down to R crosses uncut stock, a collision - must never post.
//! Provenance: our own fixtures, specced from the Prime Directive and the
//! shop rule "never rapid into uncut stock"; no vendor material.
//!
//! Gate map (M4.4): covers app::operation (drill_reach's R-plane law,
//! the Z arm of generate's Drill), app::posting (the gate).
//! Lane shape (M4.4): all tests here run in seconds.

use flecs_ecs::prelude::World;

use camcraft_app::commands::{AddTool, CreateOperation, GenerateOperation, History, ImportModel, SetStock};
use camcraft_app::drilling::{CycleChoice, Hole};
use camcraft_app::geo2d::Point2;
use camcraft_app::geometry::{Mesh, Point3, Triangle, cube};
use camcraft_app::operation::{self, Height, HeightReference, OpParams, SrcRef};
use camcraft_app::posting;
use camcraft_app::schema::{Location, Operation};
use camcraft_app::stock::StockShape;
use camcraft_app::tools::catalog;
use camcraft_app::world::new_world;

const TOP: f64 = 20.0;
const FLOOR: f64 = 12.0;

fn flip(t: &Triangle) -> Triangle {
    Triangle::new(t.a, t.c, t.b)
}

fn quad(a: Point3, b: Point3, c: Point3, d: Point3, f: &mut Vec<Triangle>) {
    f.push(Triangle::new(a, b, c));
    f.push(Triangle::new(a, c, d));
}

/// A 40 x 40 x [-10, 20] block with a square hole `2 half` wide from
/// the top at the origin, blind to `FLOOR`. Outward-wound.
fn block(half: f64) -> Mesh {
    let mut f = Vec::new();
    let outer = cube(Point3::new(-20.0, -20.0, -10.0), Point3::new(20.0, 20.0, TOP));
    f.extend(outer.facets[4..12].iter().cloned());
    f.extend(outer.facets[0..2].iter().cloned());
    let o = [(-20.0, -20.0), (20.0, -20.0), (20.0, 20.0), (-20.0, 20.0)];
    let i = [(-half, -half), (half, -half), (half, half), (-half, half)];
    let p = |(x, y): (f64, f64)| Point3::new(x, y, TOP);
    for k in 0..4 {
        let n = (k + 1) % 4;
        quad(p(o[k]), p(o[n]), p(i[n]), p(i[k]), &mut f);
    }
    let inner = cube(Point3::new(-half, -half, FLOOR), Point3::new(half, half, TOP));
    f.extend(inner.facets[4..12].iter().map(flip));
    f.extend(inner.facets[0..2].iter().map(flip));
    Mesh::from_facets(f)
}

/// A drafted casting for stock-from-model: x, y in [-25, 25], bottom
/// Z -10, its top a 1-degree slope `z = base + (x + 5) tan 1deg` - the
/// low end at x -25, the far vertex 50 mm away at x 25.
fn casting(base: f64) -> Mesh {
    let s = 1.0_f64.to_radians().tan();
    let z = |x: f64| base + (x + 5.0) * s;
    let (lo, hi) = (-25.0, 25.0);
    let b = |x: f64, y: f64| Point3::new(x, y, -10.0);
    let t = |x: f64, y: f64| Point3::new(x, y, z(x));
    let mut f = Vec::new();
    quad(b(lo, lo), b(lo, hi), b(hi, hi), b(hi, lo), &mut f);
    quad(t(lo, lo), t(hi, lo), t(hi, hi), t(lo, hi), &mut f);
    quad(b(lo, lo), b(hi, lo), t(hi, lo), t(lo, lo), &mut f);
    quad(b(hi, lo), b(hi, hi), t(hi, hi), t(hi, lo), &mut f);
    quad(b(hi, hi), b(lo, hi), t(lo, hi), t(hi, hi), &mut f);
    quad(b(lo, hi), b(lo, lo), t(lo, lo), t(lo, hi), &mut f);
    Mesh::from_facets(f)
}

/// The stock a fixture stands under.
enum Stock {
    None,
    /// A 40 x 40 box over the block whose top stands at this Z.
    Box(f64),
    /// A box of this footprint (dx, dy) centred at (cx, cy), z -10..top.
    BoxAt(f64, f64, f64, f64, f64),
    /// [`casting`] at this base, as stock-from-model.
    Drafted(f64),
}

/// The block (hole `2 half` wide) under `stock`, with the named catalog
/// tool on the rack.
fn world_of(half: f64, stock: Stock, tool: &str) -> (World, History) {
    let world = new_world();
    let mut h = History::default();
    h.execute(&world, ImportModel::from_mesh("block", block(half)));
    let at = |x: f64, y: f64| Location { pos: Point3::new(x, y, -10.0), ..Default::default() };
    let cmd = match stock {
        Stock::None => None,
        Stock::Box(top) => Some(SetStock::new(&world, StockShape::Box { dx: 40.0, dy: 40.0, dz: top + 10.0 }, at(0.0, 0.0))),
        Stock::BoxAt(dx, dy, cx, cy, top) => Some(SetStock::new(&world, StockShape::Box { dx, dy, dz: top + 10.0 }, at(cx, cy))),
        Stock::Drafted(base) => {
            h.execute(&world, ImportModel::from_mesh("casting", casting(base)));
            Some(SetStock::new(&world, StockShape::FromModel { source: "casting".into() }, Location::default()))
        }
    };
    if let Some(cmd) = cmd {
        h.execute(&world, cmd.expect("stock"));
    }
    let t = catalog().into_iter().find(|(n, _)| *n == tool).map(|(_, t)| t).expect("catalog");
    h.execute(&world, AddTool::new(&world, tool, t).expect("tool"));
    (world, h)
}

/// The block under a 40 x 40 box stock whose top stands at `stock_top`.
fn world_with(stock_top: f64) -> (World, History) {
    world_of(HALF, Stock::Box(stock_top), "DR-0400")
}

const HALF: f64 = 2.05;

fn params_at(cycle: CycleChoice, radius: f64, top: Height) -> OpParams {
    let hole = Hole { xy: Point2::new(0.0, 0.0), radius, entry_z: TOP, floor_z: FLOOR, through: false, stepped: false };
    OpParams::Drill {
        break_through: None,
        clearance_z: Height::fixed(TOP + 15.0),
        retract_z: None,
        top_z: top,
        bottom_z: None,
        cycle,
        peck: 0.0,
        dwell: 0.0,
        feed: 120.0,
        plunge_feed: 80.0,
        holes: vec![hole],
        cross: vec![],
    }
}

fn params(cycle: CycleChoice) -> OpParams {
    params_at(cycle, 2.0, Height::fixed(TOP))
}

fn op_with(tool: &str, p: OpParams) -> Operation {
    Operation { tool: tool.into(), src: SrcRef::Entity("block".into()), params: p, stamp: None, setup: None, area: Default::default(), coolant: Default::default(), after: Default::default(), suppressed: false }
}

fn raw_op(p: OpParams) -> Operation {
    op_with("DR-0400", p)
}

/// The R-plane refusal's words: it names the plane, the material top
/// it is under and the hole.
fn assert_r_refusal(e: &str, r: &str, what: &str, top: &str, ctx: &str) {
    let words = [format!("R plane Z {r}"), format!("below the {what} top Z {top}"), "hole at (0.000, 0.000)".into(), "cross uncut stock".into(), "raise Top of Part Z".into()];
    for s in &words {
        assert!(e.contains(s.as_str()), "{ctx}: the refusal names '{s}': {e}");
    }
}

fn tmp_nc(tag: &str) -> std::path::PathBuf {
    let mut p = std::env::temp_dir();
    p.push(format!("camcraft-rplane-{}-{}.nc", tag, std::process::id()));
    let _ = std::fs::remove_file(&p);
    p
}

const FANUC: &str = include_str!("../../../assets/posts/fanuc.cps");
const HEIDENHAIN: &str = include_str!("../../../assets/posts/heidenhain.cps");

/// Generate through the command path, then post through `cps`.
fn post_with(stock_top: f64, cycle: CycleChoice, cps: &str, tag: &str) -> Result<String, String> {
    let (world, mut h) = world_with(stock_top);
    let op = CreateOperation::new(&world, "D1", "DR-0400", SrcRef::Entity("block".into()), params(cycle));
    h.execute(&world, op.map_err(|e| format!("{e:?}"))?);
    h.execute(&world, GenerateOperation::new(&world, "D1").map_err(|e| format!("{e:?}"))?);
    let nc = tmp_nc(tag);
    let r = posting::post_script(&world, &nc, cps).map(|p| p.program);
    let _ = std::fs::remove_file(&nc);
    r
}

#[test]
fn an_r_plane_below_the_unfaced_stock_top_is_refused_by_name() {
    // Stock top Z 25 over a part top Z 20; R = Top of Part Z = 20: the
    // rapid 35 -> 20 crosses 5 mm of uncut stock. Before #91 this
    // posted G81 R20 (and the expanded chain's G00 Z+20) silently.
    for cycle in [CycleChoice::Drill, CycleChoice::Peck] {
        let (world, _h) = world_with(TOP + 5.0);
        let err = operation::generate(&world, &raw_op(params(cycle)), "").err();
        // #677: the refusal marks the hole on the stock top it is under.
        assert_eq!(err.as_ref().and_then(|e| e.location()), Some(Point3::new(0.0, 0.0, TOP + 5.0)), "{cycle:?}: {err:?}");
        let e = err.map(|e| e.to_string());
        let e = e.unwrap_or_else(|| panic!("{cycle:?}: a rapid down through 5 mm of stock generated"));
        assert_r_refusal(&e, "20.000", "stock", "25.000", &format!("{cycle:?}"));
    }
}

#[test]
fn every_axial_cycle_kind_refuses_an_r_plane_below_the_stock_top() {
    // Spot, tap and bore cycles rapid onto the same R plane: each is
    // held to the same law, by name (the tool of its kind, a hole it
    // fits).
    for (cycle, tool, radius, half) in [
        (CycleChoice::Spot, "DR-0400", 2.0, HALF),
        (CycleChoice::Tap, "TAP-M4", 2.0, 2.3),
        (CycleChoice::Bore, "BR-1000", 5.0, 5.05),
    ] {
        let (world, _h) = world_of(half, Stock::Box(TOP + 5.0), tool);
        let op = op_with(tool, params_at(cycle, radius, Height::fixed(TOP)));
        let e = operation::generate(&world, &op, "").err().map(|e| e.to_string());
        let e = e.unwrap_or_else(|| panic!("{cycle:?}: a rapid down through 5 mm of stock generated"));
        assert_r_refusal(&e, "20.000", "stock", "25.000", &format!("{cycle:?}"));
        // The same cycle with R on the stock top generates.
        let op = op_with(tool, params_at(cycle, radius, Height::fixed(TOP + 5.0)));
        let r = operation::generate(&world, &op, "");
        assert!(r.is_ok(), "{cycle:?}: R on the stock top generates: {:?}", r.err().map(|e| e.to_string()));
    }
}

#[test]
fn a_stockless_drill_with_r_below_the_part_top_is_refused_by_name() {
    // No stock entity: the part itself is the material over the hole.
    // R 15 under the part top Z 20 rapids 5 mm into the part.
    let (world, _h) = world_of(HALF, Stock::None, "DR-0400");
    let op = raw_op(params_at(CycleChoice::Drill, 2.0, Height::fixed(TOP - 5.0)));
    let e = operation::generate(&world, &op, "").err().map(|e| e.to_string());
    let e = e.unwrap_or_else(|| panic!("a stockless rapid 5 mm into the part generated"));
    assert_r_refusal(&e, "15.000", "part", "20.000", "stockless");
    // R on the part top still generates.
    let r = operation::generate(&world, &raw_op(params(CycleChoice::Drill)), "");
    assert!(r.is_ok(), "R on the part top generates: {:?}", r.err().map(|e| e.to_string()));
}

#[test]
fn a_hole_beside_the_stock_footprint_is_not_held_to_its_top() {
    // A tall box stock standing off to the side (x 30..50): nothing of
    // it is over the hole at the origin, so R on the part top passes.
    let (world, _h) = world_of(HALF, Stock::BoxAt(20.0, 40.0, 40.0, 0.0, TOP + 20.0), "DR-0400");
    let r = operation::generate(&world, &raw_op(params(CycleChoice::Drill)), "");
    assert!(r.is_ok(), "stock beside the hole refuses nothing: {:?}", r.err().map(|e| e.to_string()));
}

#[test]
fn r_from_top_of_stock_exactly_posts() {
    // Stock top Z 22 (the hole 10 deep from it, inside the flute).
    let (world, mut h) = world_with(TOP + 2.0);
    let p = params_at(CycleChoice::Drill, 2.0, Height::of(HeightReference::TopOfStock, 0.0));
    let op = CreateOperation::new(&world, "D1", "DR-0400", SrcRef::Entity("block".into()), p);
    h.execute(&world, op.expect("create"));
    h.execute(&world, GenerateOperation::new(&world, "D1").expect("generate"));
    let nc = tmp_nc("tos");
    let r = posting::post_script(&world, &nc, FANUC).map(|p| p.program);
    let _ = std::fs::remove_file(&nc);
    let text = r.unwrap_or_else(|e| panic!("R at Top of Stock posts: {e}"));
    assert!(text.contains("G81") && text.contains("R22"), "the canned cycle at R22: {text}");
}

/// The highest Z named in a refusal's "top Z <n>".
fn named_top(e: &str, what: &str) -> f64 {
    let key = format!("below the {what} top Z ");
    let tail = e.split(key.as_str()).nth(1).unwrap_or("");
    tail.split_whitespace().next().and_then(|w| w.parse().ok()).unwrap_or(f64::NAN)
}

#[test]
fn a_drafted_stock_top_is_read_at_the_hole_not_at_its_far_vertex() {
    // A 1-degree draft over a 50 mm top: at the hole (x 0) the casting
    // stands at 21 + 5 tan 1deg = 21.087, its far vertex (x 25) at
    // 21.524. R half a millimetre over the local surface is honest; the
    // refusal of an R under it names the local height, not the far one.
    let local = 21.0 + 5.0 * 1.0_f64.to_radians().tan();
    let (world, _h) = world_of(HALF, Stock::Drafted(21.0), "DR-0400");
    let op = raw_op(params_at(CycleChoice::Drill, 2.0, Height::fixed(local + 0.3)));
    let r = operation::generate(&world, &op, "");
    assert!(r.is_ok(), "R 0.3 over the local drafted top generates: {:?}", r.err().map(|e| e.to_string()));
    let op = raw_op(params_at(CycleChoice::Drill, 2.0, Height::fixed(local - 0.1)));
    let e = operation::generate(&world, &op, "").err().map(|e| e.to_string());
    let e = e.unwrap_or_else(|| panic!("R under the drafted top generated"));
    let top = named_top(&e, "stock");
    assert!(top >= local - 1e-6 && top <= local + 0.1, "the local height {local:.3} is named: {e}");
}

#[test]
fn neither_post_releases_an_r_plane_below_the_stock_top() {
    for cycle in [CycleChoice::Drill, CycleChoice::Peck] {
        for (n, cps) in [("fanuc", FANUC), ("heidenhain", HEIDENHAIN)] {
            let r = post_with(TOP + 5.0, cycle, cps, &format!("low-{n}"));
            let e = r.err().unwrap_or_else(|| panic!("{cycle:?} {n}: posted through the stock"));
            assert_r_refusal(&e, "20.000", "stock", "25.000", &format!("{cycle:?} {n}"));
        }
    }
}

#[test]
fn an_r_plane_at_the_stock_top_still_posts_canned_and_expanded() {
    for (cycle, word) in [(CycleChoice::Drill, "G81"), (CycleChoice::Peck, "G73")] {
        let canned = post_with(TOP, cycle, FANUC, "top-fanuc");
        let text = canned.unwrap_or_else(|e| panic!("{cycle:?}: the stock-top R plane posts: {e}"));
        assert!(text.contains(word) && text.contains("R20"), "{cycle:?}: the canned cycle at R20: {text}");
        let expanded = post_with(TOP, cycle, HEIDENHAIN, "top-heid");
        let text = expanded.unwrap_or_else(|e| panic!("{cycle:?}: the expanded chain posts: {e}"));
        assert!(text.contains("G00 X+0 Y+0 Z+20"), "{cycle:?}: the rapid onto R: {text}");
    }
}
