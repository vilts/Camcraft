//! #3354: 2D Pocket takes the shared "Pass starts at" (Auto / Inside /
//! Outside) of #3316. On `fig8.step` with EM-0800-4FL and an Outside
//! containment, the ring strategies (Waterline, Offset rings) cut each
//! level core-first on Auto: the first ring is in the middle of the band.
//! Outside starts every level at the stock edge and ends it on the part
//! wall; Inside starts on the part wall. Zigzag rows have no ring order
//! and say so by name. The option rides EditOperationParams through MCP
//! `edit_operation` and the project file; Auto is absent from both and
//! from the cache digest.
//!
//! Gate map: covers app::operation (generate, params digest), app::mcp
//! (edit_operation, undo), app::project (params, params_impl),
//! motion::machining2d (pass_start, pocket, waterscan).
//! Provenance: our own regression of the #3316 report on the pocket
//! kind; no vendor material consulted.

mod area_support;
mod common;

use area_support::*;
use camcraft_app::machining2d::{PASS_START_REST_NOTE, PASS_START_ZIGZAG_NOTE, PassStart, RestParams, RestSource};
use camcraft_app::mcp::Server;
use camcraft_app::operation::HeightReference;
use serde_json::{Value, json};

const EM8: &str = "EM-0800-4FL";
const R8: f64 = 4.0;

fn fig8_path() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../testdata/models/fig8.step")
}

/// The part's largest horizontal face at `z` (the fig8 floor).
fn horizontal_face(world: &World, name: &str, z: f64) -> FeatureSel {
    let e = model::find_by_name(world, name).expect("model");
    let f = model::features_of(world, &e).expect("features");
    let mut best: Option<(f64, FeatureSel)> = None;
    for face in f.faces.iter().filter(|face| face.normal[2].abs() > 0.99) {
        let Some(l) = face.loops.iter().find(|l| l.kind == feature::LoopKind::Outer) else { continue };
        let pts: Vec<[f64; 3]> = l.verts.iter().filter_map(|&v| f.verts.get(v as usize).copied()).collect();
        if pts.is_empty() || pts.iter().any(|p| (p[2] - z).abs() > 1e-6) {
            continue;
        }
        let ext = pts.iter().map(|p| p[0].abs().max(p[1].abs())).fold(0.0, f64::max);
        if best.as_ref().is_none_or(|(b, _)| ext > *b) {
            best = Some((ext, FeatureSel::Face(face.id)));
        }
    }
    best.expect("a horizontal face").1
}

/// fig8.step, EM-0800-4FL, a setup, and a stock box 5 mm past the part
/// in XY and 2 mm over its top. Returns the stock box corners.
fn fig8_world() -> (World, History, String, (Point3, Point3)) {
    let world = new_world();
    let mut h = History::default();
    let imp = ImportModel::new(&fig8_path()).expect("fig8.step imports");
    let name = imp.name.clone();
    h.execute(&world, imp);
    let e = model::find_by_name(&world, &name).expect("part");
    let b = model::entity_world_aabb(&world, &e).expect("part aabb");
    let tool = catalog().into_iter().find(|(n, _)| *n == EM8).map(|(_, t)| t).expect("catalog tool");
    h.execute(&world, AddTool::new(&world, EM8, tool).expect("tool"));
    h.execute(&world, camcraft_app::commands::CreateSetup::new(&world, None, None).expect("setup"));
    let m = 5.0;
    let (dx, dy, dz) = ((b.max.x - b.min.x) + 2.0 * m, (b.max.y - b.min.y) + 2.0 * m, (b.max.z - b.min.z) + 2.0);
    let loc = Location { pos: Point3::new(0.5 * (b.min.x + b.max.x), 0.5 * (b.min.y + b.max.y), b.min.z), ..Default::default() };
    h.execute(&world, SetStock::new(&world, StockShape::Box { dx, dy, dz }, loc).expect("stock"));
    let lo = Point3::new(b.min.x - m, b.min.y - m, b.min.z);
    let hi = Point3::new(b.max.x + m, b.max.y + m, b.max.z + 2.0);
    (world, h, name, (lo, hi))
}

/// A 2D Pocket with `strategy`, `start` and `rest`, Outside containment
/// over the floor face, the #3316 heights.
fn pocket_op(
    world: &World,
    h: &mut History,
    name: &str,
    zmin: f64,
    strategy: PocketStrategy,
    start: PassStart,
    rest: Option<RestParams>,
) -> Toolpath {
    let src = sel(name, horizontal_face(world, name, zmin));
    let mut p = pocket(zmin + 20.0, zmin);
    if let OpParams::Pocket { clearance_z, top_z, bottom_z, stepdown, stepover, strategy: s, pass_start, rest: r, .. } =
        &mut p
    {
        *clearance_z = Height { reference: HeightReference::TopOfStock, offset: 10.0 };
        *top_z = Height { reference: HeightReference::TopOfStock, offset: 0.0 };
        *bottom_z = Height { reference: HeightReference::BottomOfSelection, offset: 1.5 };
        *stepdown = 10.0;
        *stepover = 3.0;
        *s = strategy;
        *pass_start = start;
        *r = rest;
    }
    let area = AreaControls { containment: Some(Containment::Outside), ..Default::default() };
    h.execute(world, CreateOperation::new(world, "P1", EM8, src, p.clone()).expect("create"));
    h.execute(world, EditOperationParams::with_area(world, "P1", p, area).expect("area"));
    h.execute(world, camcraft_app::commands::GenerateOperation::new(world, "P1").expect("generate"));
    let e = model::find_by_name(world, "P1").expect("op");
    model::toolpath_of(world, &e).expect("cached").path.clone()
}

/// Each cut level, by Z: where its first ring starts (the end of the
/// level's first planar feed) and where its last planar feed ends.
fn level_ends(t: &Toolpath) -> Vec<(f64, Point3, Point3)> {
    let mut out: Vec<(f64, Point3, Point3)> = Vec::new();
    for m in t.moves.iter().filter(|m| m.kind == MoveKind::Feed && (m.from.z - m.to.z).abs() < 1e-9) {
        match out.iter_mut().find(|(z, _, _)| (z - m.from.z).abs() < 1e-6) {
            Some(level) => level.2 = m.to,
            None => out.push((m.from.z, m.to, m.to)),
        }
    }
    out
}

/// How far `p` stands outside the stock box in XY (0 inside it).
fn outside_box(p: Point3, (lo, hi): (Point3, Point3)) -> f64 {
    let dx = (lo.x - p.x).max(p.x - hi.x).max(0.0);
    let dy = (lo.y - p.y).max(p.y - hi.y).max(0.0);
    dx.hypot(dy)
}

/// Distance from `pt` to the part's projected outline, in XY.
fn to_part(world: &World, name: &str, pt: Point3) -> f64 {
    let e = model::find_by_name(world, name).expect("part");
    let facets = model::world_facets(world, &e).expect("facets");
    let reg = camcraft_app::geometry::slice::project(&facets, 64).expect("part outline");
    let mut best = f64::INFINITY;
    for l in reg.loops() {
        let pts = l.points();
        for i in 0..pts.len() {
            let (a, b) = (pts[i], pts[(i + 1) % pts.len()]);
            let (dx, dy) = (b.x - a.x, b.y - a.y);
            let dd = dx * dx + dy * dy;
            let t = if dd > 0.0 { (((pt.x - a.x) * dx + (pt.y - a.y) * dy) / dd).clamp(0.0, 1.0) } else { 0.0 };
            best = best.min((pt.x - a.x - dx * t).hypot(pt.y - a.y - dy * t));
        }
    }
    best
}

/// The Outside law on one ring strategy: at least 2 levels, each level
/// starts outside the stock box and ends on the part wall.
fn assert_outside_starts_at_the_stock_edge(strategy: PocketStrategy) {
    let (world, mut h, name, stock) = fig8_world();
    let path = pocket_op(&world, &mut h, &name, stock.0.z, strategy, PassStart::Outside, None);
    let levels = level_ends(&path);
    assert!(levels.len() >= 2, "{strategy:?} levels: {levels:?}");
    for (z, first, last) in &levels {
        let out = outside_box(*first, stock);
        assert!(out >= R8 - 0.05, "{strategy:?} Outside Z {z:.3}: the first ring starts {out:.3} mm outside the stock");
        let d = to_part(&world, &name, *last);
        assert!((d - R8).abs() < 0.05, "{strategy:?} Outside Z {z:.3}: the level ends {d:.3} mm from the part");
    }
}

/// #3354 pin: on both ring strategies, Outside starts each level at the
/// stock edge (the first ring cuts one stepover, never a slot) and ends
/// it on the part wall.
#[test]
#[ignore = "heavy lane: app heavy: pocket_pass_start (94 s in debug, #3587)"]
fn fig8_pocket_outside_starts_each_level_at_the_stock_edge() {
    for strategy in [PocketStrategy::Waterline, PocketStrategy::OffsetRings] {
        assert_outside_starts_at_the_stock_edge(strategy);
    }
}

/// Light twin of `fig8_pocket_outside_starts_each_level_at_the_stock_edge`:
/// the same law on Waterline only (one generation in place of two).
#[test]
#[ignore = "light lane: the Waterline case of the full test only (#3587); the full test is fig8_pocket_outside_starts_each_level_at_the_stock_edge"]
fn fig8_pocket_outside_starts_each_level_at_the_stock_edge_light() {
    assert_outside_starts_at_the_stock_edge(PocketStrategy::Waterline);
}

/// #3354 pin: Inside starts each level on the part wall.
#[test]
fn fig8_pocket_inside_starts_each_level_on_the_part_wall() {
    let (world, mut h, name, stock) = fig8_world();
    let path = pocket_op(&world, &mut h, &name, stock.0.z, PocketStrategy::Waterline, PassStart::Inside, None);
    let levels = level_ends(&path);
    assert!(levels.len() >= 2, "levels: {levels:?}");
    for (z, first, _) in &levels {
        let d = to_part(&world, &name, *first);
        assert!((d - R8).abs() < 0.05, "Inside Z {z:.3}: the first ring starts {d:.3} mm from the part");
    }
}

/// Auto keeps the core-first order (the first ring is not at the stock
/// edge) and adds no note.
#[test]
fn fig8_pocket_auto_keeps_the_core_first_order() {
    let (world, mut h, name, stock) = fig8_world();
    let path = pocket_op(&world, &mut h, &name, stock.0.z, PocketStrategy::Waterline, PassStart::Auto, None);
    let levels = level_ends(&path);
    assert!(
        levels.iter().all(|(_, first, _)| outside_box(*first, stock) < R8 - 0.05),
        "Auto moved to the stock edge: {levels:?}"
    );
    assert!(!path.notes.iter().any(|n| n.starts_with("Pass starts at")), "Auto says nothing: {:?}", path.notes);
}

/// Zigzag rows have no ring order: Inside is not dropped silently, the
/// path says so by name.
#[test]
fn fig8_zigzag_pocket_says_pass_start_is_not_used() {
    let (world, mut h, name, stock) = fig8_world();
    let path = pocket_op(&world, &mut h, &name, stock.0.z, PocketStrategy::Zigzag, PassStart::Inside, None);
    assert!(path.notes.iter().any(|n| n == PASS_START_ZIGZAG_NOTE), "the Zigzag note: {:?}", path.notes);
}

/// Rest machining keeps its own order of the rest areas: Outside is not
/// dropped silently, the path says so by name.
#[test]
fn fig8_rest_pocket_says_pass_start_is_not_used() {
    let (world, mut h, name, stock) = fig8_world();
    let rest = RestParams {
        source: RestSource::EnteredTool,
        previous_tool_diameter: 12.0,
        previous_corner_radius: 0.0,
        previous_stock_to_leave: 0.0,
        allowance: 0.0,
    };
    let path = pocket_op(&world, &mut h, &name, stock.0.z, PocketStrategy::Waterline, PassStart::Outside, Some(rest));
    assert!(path.notes.iter().any(|n| n == PASS_START_REST_NOTE), "the rest note: {:?}", path.notes);
}

fn call(s: &mut Server, id: i64, name: &str, args: Value) -> Value {
    let msg = json!({ "jsonrpc": "2.0", "id": id, "method": "tools/call", "params": { "name": name, "arguments": args } });
    s.handle(&msg).expect("a response")["result"].clone()
}

/// The option through MCP, undo and the project file (the digest is
/// pinned in `operation::tests`, beside the stamp it reads).
#[test]
fn pocket_pass_start_rides_mcp_undo_and_the_file() {
    let mut s = Server::new();
    let ok = |r: &Value| r["isError"] != json!(true);
    assert!(ok(&call(&mut s, 1, "import_model", json!({ "path": fig8_path().display().to_string() }))));
    assert!(ok(&call(&mut s, 2, "add_tool", json!({ "catalog": EM8 }))));
    let add = call(&mut s, 3, "add_operation", json!({ "kind": "2D Pocket", "tool": EM8, "name": "P1", "source": "fig8" }));
    assert!(ok(&add), "{add}");
    assert!(!add.to_string().contains("pass_start"), "Auto is absent: {add}");
    let edit = call(&mut s, 4, "edit_operation", json!({ "name": "P1", "params": { "pass_start": "outside" } }));
    assert!(edit.to_string().contains("\"pass_start\":\"outside\""), "{edit}");
    let bad = call(&mut s, 5, "edit_operation", json!({ "name": "P1", "params": { "pass_start": "sideways" } }));
    assert_eq!(bad["isError"], json!(true), "an unknown word is refused: {bad}");
    let auto = call(&mut s, 6, "edit_operation", json!({ "name": "P1", "params": { "pass_start": "auto" } }));
    assert!(ok(&auto) && !auto.to_string().contains("pass_start"), "Auto is not written: {auto}");
    assert!(ok(&call(&mut s, 7, "undo", json!({}))));
    let job = call(&mut s, 8, "get_job", json!({})).to_string();
    assert!(job.contains("\"pass_start\":\"outside\""), "undo restores Outside: {job}");

    let mut p = pocket(20.0, 0.0);
    assert!(!project::params_to_json(&p).to_string().contains("pass_start"), "Auto stays off the wire");
    if let OpParams::Pocket { pass_start, .. } = &mut p {
        *pass_start = PassStart::Inside;
    }
    let wire = project::params_to_json(&p);
    assert!(wire.to_string().contains("\"pass_start\":\"inside\""), "{wire}");
    assert_eq!(project::params_from_json(&wire).expect("reads back"), p);
}
