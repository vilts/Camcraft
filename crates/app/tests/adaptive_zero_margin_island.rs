//! Issue #2438: a whole-model 2D Adaptive in a zero-margin stock around
//! an island on a plate:
//! 1. With default area controls (tool kept inside the sharp stock
//!    outline), the four sharp stock corners are left uncut; the op
//!    names that unreached material in its notes (and MCP `generate` /
//!    `verify` expose those notes).
//! 2. With `Boundary` `extension` (e.g. `extension: 6`), only the
//!    exterior boundary grows past the stock - the island stays
//!    untouched, the four stock corners are cleared to Bottom Z, and
//!    `verify` is clean.
//!
//! Gate map (#2438): covers app::operation (area, boundary, boundary_ctx,
//! generate, keep_law), app::mcp (area, job, render), app::script::api,
//! motion::machining2d (unreached), app::stock_sim, sim::verify.
//! Provenance: our own island-on-plate fixture matching #2438; no vendor
//! material consulted.

mod area_support;
mod common;
mod pocket_support;

use area_support::*;
use common::solid_part::{cap, rect, walls};
use pocket_support::{rounded_rect, write_stl};
use serde_json::{json, Value};

use camcraft_app::geometry::{Mesh, Triangle};
use camcraft_app::mcp::Server;
use camcraft_app::posting;
use camcraft_app::stock_sim::simulate_document;

/// 100 x 60 x 10 plate (z 0..10) centred on the origin with a 60 x 30 x 5
/// R5-cornered island on top (z 10..15). Watertight, outward-wound.
fn island_on_plate() -> Mesh {
    let plate = rect(-50.0, -30.0, 50.0, 30.0);
    let island = rounded_rect(30.0, 15.0, 5.0, 8);
    let mut f: Vec<Triangle> = Vec::new();
    cap(&plate, None, 0.0, false, &mut f);
    walls(&plate, 0.0, 10.0, true, &mut f);
    cap(&plate, Some(&island), 10.0, true, &mut f);
    walls(&island, 10.0, 15.0, true, &mut f);
    cap(&island, None, 15.0, true, &mut f);
    Mesh::from_facets(f)
}

fn call(s: &mut Server, id: &mut i64, name: &str, args: Value) -> Value {
    *id += 1;
    let msg = json!({ "jsonrpc": "2.0", "id": *id, "method": "tools/call",
        "params": { "name": name, "arguments": args } });
    let reply = s.handle(&msg).expect("request gets response");
    let res = &reply["result"];
    assert_ne!(res["isError"], json!(true), "{name} refused: {}", res["content"][0]["text"]);
    res["structuredContent"].clone()
}

fn setup_mcp(tag: &str) -> (Server, i64, std::path::PathBuf) {
    let dir = std::env::temp_dir().join(format!("camcraft-2438-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("temp dir");
    let stl = dir.join("flange.stl");
    write_stl(&island_on_plate(), &stl);
    let mut s = Server::new();
    let mut id = 0;
    let _ = s.handle(&json!({ "jsonrpc": "2.0", "id": 0, "method": "initialize", "params": {} }));
    call(&mut s, &mut id, "import_model", json!({ "name": "flange", "path": stl.display().to_string() }));
    call(&mut s, &mut id, "set_stock", json!({ "box": { "dx": 100.0, "dy": 60.0, "dz": 15.0 } }));
    call(&mut s, &mut id, "add_tool", json!({ "catalog": TOOL }));
    (s, id, dir)
}

fn add_adaptive_mcp(s: &mut Server, id: &mut i64, source: &str) {
    call(
        s,
        id,
        "add_operation",
        json!({
            "kind": "2D Adaptive",
            "tool": TOOL,
            "source": source,
            "name": "Rough",
            "params": {
                "top_z": { "reference": "top_of_model", "offset": 0.0 },
                "bottom_z": { "reference": "bottom_of_model", "offset": 10.0 },
                "stepdown": 5.0
            }
        }),
    );
}

/// Signed distance from (x, y) outside the 60 x 30 R5 island centered at the origin.
fn dist_to_island(x: f64, y: f64) -> f64 {
    let dx = (x.abs() - 25.0).max(0.0);
    let dy = (y.abs() - 10.0).max(0.0);
    if dx > 0.0 && dy > 0.0 {
        dx.hypot(dy) - 5.0
    } else {
        (x.abs() - 30.0).max(y.abs() - 15.0)
    }
}

/// The verify findings other than the advisory Adaptive fallback to the
/// Offset rings (#3035): what "verify is clean" means here.
fn binding(ver: &Value) -> Vec<&Value> {
    let all = ver["findings"].as_array().expect("verify lists its findings");
    all.iter().filter(|x| x["kind"] != json!("strategy_fallback")).collect()
}

#[test]
fn zero_margin_adaptive_around_island_names_uncut_stock_corners() {
    let (mut s, mut id, dir) = setup_mcp("default");
    add_adaptive_mcp(&mut s, &mut id, "flange");
    let generated = call(&mut s, &mut id, "generate", json!({}));
    let ver = call(&mut s, &mut id, "verify", json!({}));
    assert_eq!(ver["ok"], json!(true), "{ver}");
    assert_eq!(binding(&ver), Vec::<&Value>::new(), "{ver}");

    let rendered = call(&mut s, &mut id, "render_stock", json!({ "width": 160, "height": 120 }));
    assert!(
        rendered["rest"]["max"].as_f64().is_some_and(|v| v > 4.0),
        "default keeps tool inside stock corners: {rendered}"
    );

    let gen_notes = generated["generated"][0]["notes"].as_array().expect("generate exposes op notes");
    assert!(
        gen_notes.iter().any(|n| n.as_str().is_some_and(|t| t.contains("corner") && t.contains("10.000 mm tool"))),
        "generate notes missing corner warning: {generated}"
    );
    let ver_notes = ver["operations"][0]["notes"].as_array().expect("verify exposes op notes");
    assert!(
        ver_notes.iter().any(|n| n.as_str().is_some_and(|t| t.contains("corner"))),
        "verify notes missing corner warning: {ver}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

fn check_extension_clears_corners(tag: &str, source: &str) {
    let (mut s, mut id, dir) = setup_mcp(tag);
    add_adaptive_mcp(&mut s, &mut id, source);
    call(
        &mut s,
        &mut id,
        "set_operation_area",
        json!({ "operation": "Rough", "area": { "extension": 6.0 } }),
    );
    let generated = call(&mut s, &mut id, "generate", json!({}));
    let ver = call(&mut s, &mut id, "verify", json!({}));
    assert_eq!(ver["ok"], json!(true), "{tag}: verify must be clean: {ver}");
    assert_eq!(binding(&ver), Vec::<&Value>::new(), "{tag}: no findings: {ver}");

    // Extension cleared all stock corners: no corner/narrow-band note emitted.
    if let Some(notes) = generated["generated"][0]["notes"].as_array() {
        assert!(
            notes.iter().all(|n| n.as_str().is_none_or(|t| !t.contains("corner") && !t.contains("narrower"))),
            "{tag}: unexpected unreached note: {notes:?}"
        );
    }

    let rendered = call(&mut s, &mut id, "render_stock", json!({ "width": 160, "height": 120 }));
    assert_eq!(rendered["gouge"]["cells"], json!(0), "{tag}: zero gouge cells: {rendered}");
    assert!(
        rendered["rest"]["max"].as_f64().is_some_and(|v| v < 0.05),
        "{tag}: corners cleared to floor: {rendered}"
    );

    let nc = dir.join("out.nc");
    let posted = call(&mut s, &mut id, "post", json!({ "output": nc.display().to_string(), "post": "Fanuc" }));
    assert!(posted["lines"].as_u64().is_some_and(|n| n > 0), "{tag}: {posted}");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn extension_on_island_on_plate_grows_only_exterior_clears_corners_part_source() {
    check_extension_clears_corners("ext-part", "flange");
}

#[test]
fn extension_on_island_on_plate_grows_only_exterior_clears_corners_stock_source() {
    check_extension_clears_corners("ext-stock", "stock");
}

/// With +2 mm stock on top (100 x 60 x 17, matching #2438's repro), the
/// ladder slices at Z 15 and Z 10; `extension: 6` must keep every feed
/// move >= R off the island and pass verify + post cleanly.
#[test]
fn extension_with_top_allowance_keeps_island_untouched_and_clears_corners() {
    let world = new_world();
    let mut h = History::default();
    h.execute(&world, ImportModel::from_mesh("flange", island_on_plate()));
    h.execute(
        &world,
        SetStock::new(&world, StockShape::Box { dx: 100.0, dy: 60.0, dz: 17.0 }, Location::default()).expect("stock"),
    );
    let tool = catalog().into_iter().find(|(n, _)| *n == TOOL).map(|(_, t)| t).expect("catalog");
    h.execute(&world, AddTool::new(&world, TOOL, tool).expect("tool"));

    let mut params = operation::retype_params(&pocket(15.0, 10.0), OpType::Adaptive2D.ordinal());
    if let OpParams::Adaptive2D { top_z, bottom_z, stepdown, .. } = &mut params {
        *top_z = Height::fixed(15.0);
        *bottom_z = Height::fixed(10.0);
        *stepdown = 5.0;
    }
    let area = AreaControls { extension: 6.0, ..Default::default() };
    h.execute(&world, CreateOperation::new(&world, "Rough", TOOL, SrcRef::Entity("flange".into()), params.clone()).expect("create"));
    h.execute(&world, EditOperationParams::with_area(&world, "Rough", params, area).expect("area"));
    h.execute(&world, camcraft_app::commands::GenerateOperation::new(&world, "Rough").expect("generate"));

    let op_e = model::find_by_name(&world, "Rough").expect("op");
    let tp = model::toolpath_of(&world, &op_e).expect("cached");
    // #3035: only the advisory fallback to the rings.
    assert!(
        tp.warnings.iter().all(|w| matches!(w, camcraft_app::verify::Warning::StrategyFallback { .. })),
        "verify must be clean: {:?}",
        tp.warnings
    );
    assert!(
        tp.path.notes.iter().all(|n| !n.contains("corner") && !n.contains("narrower")),
        "extension cleared corners: {:?}",
        tp.path.notes
    );
    // `rounded_rect(..., 8)` places chords `5 * (1 - cos(pi/16)) ~= 0.096 mm`
    // inside the smooth R5 corner; allow that facet chord sag + planner tol.
    let chord_sag = 5.0 * (1.0 - (std::f64::consts::PI / 16.0).cos());
    for m in tp.path.moves.iter().filter(|m| m.kind != MoveKind::Rapid) {
        if m.from.z.min(m.to.z) < 15.0 - 1e-4 {
            for (a, b) in m.chords(0.05) {
                for k in 0..=4 {
                    let t = f64::from(k) / 4.0;
                    let x = a.x + (b.x - a.x) * t;
                    let y = a.y + (b.y - a.y) * t;
                    let d = dist_to_island(x, y);
                    assert!(d >= R - chord_sag - 0.05, "cutter entered island at ({x:.2}, {y:.2}), d={d:.3}");
                }
            }
        }
    }
    let sim = simulate_document(&world).expect("sim runs");
    assert_eq!(sim.dev.gouge_cells, 0, "zero gouge cells");
    let dir = std::env::temp_dir().join(format!("camcraft-2438-top17-{}", std::process::id()));
    let _ = std::fs::create_dir_all(&dir);
    let nc = dir.join("out.nc");
    posting::post_setup(&world, &nc).expect("clean program posts");
    let _ = std::fs::remove_dir_all(&dir);
}
