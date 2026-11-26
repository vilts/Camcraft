//! #3355: 2D Adaptive "Pass starts at" (Auto / Inside / Outside), the
//! shared parameter of #3316. With an Outside containment on `fig8.step`
//! and EM-0800-4FL (the #2849 pin 3 case), Outside starts each level
//! with the tool in air past the stock edge and ends it on the part
//! wall. Inside starts at the part wall and cuts outward to the stock
//! edge (the #3316 ring law cuts each connected piece on its own, so the
//! last vertex is one piece's last cut, not always the edge). (The
//! air plan path is pinned in `motion::machining2d::pocket_hsm_start`.) The
//! Trochoidal method refuses both by name. The option rides the undoable
//! EditOperationParams through MCP `edit_operation` and the project
//! file, and Auto is absent from both.
//!
//! Gate map: covers app::operation (generate, params digest, sane),
//! app::mcp (edit_operation, undo), app::project (params, params_impl),
//! motion::machining2d (pocket, pocket_hsm, adaptive air plan), sim::verify.
//! Provenance: our own regression of the maintainer's request; no vendor
//! material consulted.

mod area_support;
mod common;

use area_support::*;
use camcraft_app::machining2d::{AdaptiveMethod, PassStart};
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

/// A 2D Adaptive with an Outside containment over the floor face: one
/// level 5 mm over the floor (the #2849 pin 3 heights), with `start`.
fn adaptive_op(world: &World, name: &str, (lo, hi): (Point3, Point3), start: PassStart, method: AdaptiveMethod) -> Result<(Toolpath, Vec<String>), String> {
    let src = sel(name, horizontal_face(world, name, lo.z));
    let mut p = operation::retype_params(&pocket(hi.z, lo.z + 5.0), OpType::Adaptive2D.ordinal());
    if let OpParams::Adaptive2D { top_z, bottom_z, stepdown, entry, helix_radius, pass_start, method: m, .. } = &mut p {
        *top_z = Height { reference: HeightReference::TopOfStock, offset: 0.0 };
        *bottom_z = Height::fixed(lo.z + 5.0);
        *stepdown = 20.0;
        *entry = Entry3D::Helix;
        *helix_radius = 1.0;
        *pass_start = start;
        *m = method;
    }
    let area = AreaControls { containment: Some(Containment::Outside), ..Default::default() };
    let mut o = op(src, p, area);
    o.tool = EM8.into();
    let (path, warnings) = operation::generate(world, &o, "").map_err(|e| e.to_string())?;
    Ok((path, warnings.iter().map(|w| format!("{w:?}")).collect()))
}

/// Where the level's first planar feed ends and where its last ends.
fn first_last(t: &Toolpath) -> (Point3, Point3) {
    let mut planar = t.moves.iter().filter(|m| m.kind == MoveKind::Feed && (m.from.z - m.to.z).abs() < 1e-9);
    let first = planar.next().expect("a planar feed").to;
    let last = planar.next_back().map_or(first, |m| m.to);
    (first, last)
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

/// #3355 pin: Outside starts the level with the tool's disc outside the
/// stock box (in air, never a slot) and ends it on the part wall. The
/// Outside band stops where the disc touches the stock, so the air plan
/// has no room for its entry gap here: the Offset rings cut the band in
/// the Outside order, the note says so, and the switch raises the
/// #3035 StrategyFallback warning like every other rings fallback.
#[test]
fn fig8_outside_adaptive_starts_in_air_and_ends_on_the_part_wall() {
    let (world, _h, name, stock) = fig8_world();
    let (path, warnings) = adaptive_op(&world, &name, stock, PassStart::Outside, AdaptiveMethod::Spiral).expect("generates");
    let fallback: Vec<&String> = warnings.iter().filter(|w| w.contains("StrategyFallback")).collect();
    assert_eq!(fallback.len(), 1, "the rings fallback warns (#3035): {warnings:?}");
    assert_eq!(warnings.len(), 1, "no other warning: {warnings:?}");
    assert!(path.notes.iter().any(|n| n.starts_with("adaptive: Pass starts at Outside")), "the switch is said: {:?}", path.notes);
    let (first, last) = first_last(&path);
    let out = outside_box(first, stock);
    assert!(out >= R8 - 0.05, "the first pass starts {out:.3} mm outside the stock, in the stock: {first:?}");
    let d = to_part(&world, &name, last);
    assert!((d - R8).abs() < 0.05, "the level ends {d:.3} mm from the part, not on its wall");
}

/// #3355 pin: Inside starts each level on the part wall and cuts outward
/// to the stock edge. The band's hole walls give no full-width slot, so
/// the Offset rings cut it from the part out, and the switch raises the
/// #3035 StrategyFallback warning like every other rings fallback.
/// The #3316 ring law cuts each connected piece of the area on its own
/// ("the walls last" per piece), so the level's last vertex is one
/// piece's last cut, not always the stock edge. The whole stock edge is
/// still cut - checked on both sides of the band.
#[test]
fn fig8_inside_adaptive_starts_on_the_part_wall_and_cuts_out_to_the_edge() {
    let (world, _h, name, stock) = fig8_world();
    let (path, warnings) = adaptive_op(&world, &name, stock, PassStart::Inside, AdaptiveMethod::Spiral).expect("generates");
    let fallback: Vec<&String> = warnings.iter().filter(|w| w.contains("StrategyFallback")).collect();
    assert_eq!(fallback.len(), 1, "the rings fallback warns (#3035): {warnings:?}");
    assert_eq!(warnings.len(), 1, "no other warning: {warnings:?}");
    assert!(path.notes.iter().any(|n| n.starts_with("adaptive: Pass starts at Inside")), "the switch is said: {:?}", path.notes);
    let (first, _last) = first_last(&path);
    let d = to_part(&world, &name, first);
    assert!((d - R8).abs() < 0.05, "the first pass starts {d:.3} mm from the part, not on its wall");
    // The walk goes from the part out, so the level cuts the stock edge
    // in air on both sides of the fig8 band. A stock edge left standing
    // is a short path, and the Prime Directive never posts one.
    let edges: Vec<Point3> = path
        .moves
        .iter()
        .filter(|m| m.kind == MoveKind::Feed && outside_box(m.to, stock) >= R8 - 0.05)
        .map(|m| m.to)
        .collect();
    assert!(edges.iter().any(|p| p.x < 0.0), "no stock-edge cut on the left ({} edge cut(s))", edges.len());
    assert!(edges.iter().any(|p| p.x > 0.0), "no stock-edge cut on the right ({} edge cut(s))", edges.len());
}

/// The Trochoidal method has no ring order: it refuses Inside and
/// Outside by name, never ignores them.
#[test]
fn trochoidal_adaptive_refuses_a_forced_pass_start_by_name() {
    for start in [PassStart::Inside, PassStart::Outside] {
        let (world, _h, name, stock) = fig8_world();
        let Err(err) = adaptive_op(&world, &name, stock, start, AdaptiveMethod::Trochoidal) else {
            panic!("{start:?}: the Trochoidal method cut a forced order instead of a refusal");
        };
        assert!(err.contains("Pass starts at") && err.contains("Trochoidal"), "{start:?}: {err}");
    }
    // The command door says it first (validate early).
    let mut p = operation::retype_params(&pocket(20.0, 0.0), OpType::Adaptive2D.ordinal());
    if let OpParams::Adaptive2D { method, pass_start, .. } = &mut p {
        (*method, *pass_start) = (AdaptiveMethod::Trochoidal, PassStart::Outside);
    }
    let why = p.named_refusal().expect("the command door refuses");
    assert!(why.contains("Pass starts at Outside"), "{why}");
}

fn call(s: &mut Server, id: i64, name: &str, args: Value) -> Value {
    let msg = json!({ "jsonrpc": "2.0", "id": id, "method": "tools/call", "params": { "name": name, "arguments": args } });
    s.handle(&msg).expect("a response")["result"].clone()
}

/// The option through MCP and the project file on 2D Adaptive: kept,
/// `auto` never written, an unknown word refused, undo restores it.
#[test]
fn mcp_sets_adaptive_pass_start_and_undo_restores_it() {
    let mut s = Server::new();
    let ok = |r: &Value| r["isError"] != json!(true);
    let imp = call(&mut s, 1, "import_model", json!({ "path": fig8_path().display().to_string() }));
    assert!(ok(&imp), "{imp}");
    assert!(ok(&call(&mut s, 2, "add_tool", json!({ "catalog": EM8 }))));
    let add = call(&mut s, 3, "add_operation", json!({ "kind": "2D Adaptive", "tool": EM8, "name": "A1", "source": "fig8" }));
    assert!(ok(&add), "{add}");
    assert!(!add.to_string().contains("pass_start"), "Auto is absent: {add}");
    let edit = call(&mut s, 4, "edit_operation", json!({ "name": "A1", "params": { "pass_start": "outside" } }));
    assert!(edit.to_string().contains("\"pass_start\":\"outside\""), "{edit}");
    let bad = call(&mut s, 5, "edit_operation", json!({ "name": "A1", "params": { "pass_start": "sideways" } }));
    assert_eq!(bad["isError"], json!(true), "an unknown word is refused: {bad}");
    let auto = call(&mut s, 6, "edit_operation", json!({ "name": "A1", "params": { "pass_start": "auto" } }));
    assert!(ok(&auto), "auto is a known word: {auto}");
    assert!(!auto.to_string().contains("pass_start"), "Auto is not written: {auto}");
    assert!(ok(&call(&mut s, 7, "undo", json!({}))));
    let job = call(&mut s, 8, "get_job", json!({})).to_string();
    assert!(job.contains("\"pass_start\":\"outside\""), "undo restores Outside: {job}");
    let mut p = operation::retype_params(&pocket(20.0, 0.0), OpType::Adaptive2D.ordinal());
    assert!(!project::params_to_json(&p).to_string().contains("pass_start"), "Auto stays off the wire");
    if let OpParams::Adaptive2D { pass_start, .. } = &mut p {
        *pass_start = PassStart::Inside;
    }
    let wire = project::params_to_json(&p);
    assert!(wire.to_string().contains("\"pass_start\":\"inside\""), "{wire}");
    assert_eq!(project::params_from_json(&wire).expect("reads back"), p);
}
