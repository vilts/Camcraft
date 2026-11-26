//! #1864 review: 2D Contour on DXF sketches - a mixed closed and open pick
//! under Boundary edits is refused by name, and loops from separate sketches
//! keep their Auto side when only Reverse changes.
//!
//! Gate map (M4.4): covers app::operation (contour_sketch, contour_sketch_pick,
//! generate), app::commands (EditOperationParams).
//! Provenance: our own fixtures over our own commands; DXF geometry uses
//! public Autodesk ASCII DXF group codes.

use std::path::{Path, PathBuf};

use flecs_ecs::prelude::World;

use camcraft_app::cl::MoveKind;
use camcraft_app::commands::{
    AddTool, Command, CommandError, CreateOperation, DxfOptions, EditOperationParams,
    GenerateOperation, History, ImportDxf, SetStock,
};
use camcraft_app::geometry::sketch_chains::ChainId;
use camcraft_app::machining2d::Lead;
use camcraft_app::model;
use camcraft_app::operation::{
    self, AreaControls, ChainOverride, FeatureSel, GeoSel, Height, HeightReference, OpParams,
    OpType, SrcRef,
};
use camcraft_app::schema::Location;
use camcraft_app::sketch;
use camcraft_app::stock::StockShape;
use camcraft_app::tools::catalog;
use camcraft_app::world::new_world;

const TOOL: &str = "EM-0400-2FL"; // 4 mm endmill -> tool radius 2 mm

fn tmp_dir(tag: &str) -> PathBuf {
    let mut p = std::env::temp_dir();
    p.push(format!("camcraft-1864-mixed-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&p);
    std::fs::create_dir_all(&p).expect("temp dir");
    p
}

const HEAD: &str = "0\nSECTION\n2\nHEADER\n9\n$INSUNITS\n70\n4\n0\nENDSEC\n0\nSECTION\n2\nENTITIES\n";
const TAIL: &str = "0\nENDSEC\n0\nEOF\n";

/// Closed R8 circle at (25, 55) and the open line (20, 10) -> (100, 10).
fn mixed_dxf() -> String {
    format!(
        "{HEAD}0\nCIRCLE\n8\nholes\n10\n25\n20\n55\n40\n8\n0\nLINE\n8\nopen\n10\n20\n20\n10\n11\n100\n21\n10\n{TAIL}"
    )
}

/// Closed rectangle 0..120 x 0..80.
fn rect_dxf() -> String {
    format!(
        "{HEAD}0\nLWPOLYLINE\n8\ncut\n90\n4\n70\n1\n10\n0\n20\n0\n10\n120\n20\n0\n10\n120\n20\n80\n10\n0\n20\n80\n{TAIL}"
    )
}

/// Closed R10 circle at (60, 40): inside the rectangle when both sit on the stock.
fn ring_dxf() -> String {
    format!("{HEAD}0\nCIRCLE\n8\nring\n10\n60\n20\n40\n40\n10\n{TAIL}")
}

fn run(world: &World, history: &mut History, cmd: Result<impl Command + 'static, CommandError>, what: &str) {
    history.execute(world, cmd.unwrap_or_else(|e| panic!("{what}: {e}")));
}

fn sheet_world(tag: &str, dxfs: &[(&str, String)]) -> (PathBuf, World, History) {
    let dir = tmp_dir(tag);
    let world = new_world();
    let mut history = History::default();
    let shape = StockShape::Box { dx: 300.0, dy: 240.0, dz: 10.0 };
    run(&world, &mut history, SetStock::new(&world, shape, Location::default()), "sheet stock");
    let tool = catalog().into_iter().find(|(n, _)| *n == TOOL).map(|(_, t)| t).expect("catalog tool");
    run(&world, &mut history, AddTool::new(&world, TOOL, tool), "tool");
    for (name, text) in dxfs {
        let path = write_dxf(&dir, name, text);
        run(&world, &mut history, ImportDxf::new(&world, None, &path, &DxfOptions::default()), "import dxf");
    }
    (dir, world, history)
}

fn write_dxf(dir: &Path, name: &str, text: &str) -> PathBuf {
    let path = dir.join(name);
    std::fs::write(&path, text).expect("write dxf");
    path
}

fn contour_params() -> OpParams {
    let mut params = operation::seed_params(OpType::Contour.ordinal());
    if let OpParams::Contour { clearance_z, retract_z, top_z, bottom_z, stepdown, lead, tolerance, .. } = &mut params {
        *clearance_z = Height::of(HeightReference::TopOfStock, 10.0);
        *retract_z = Height::of(HeightReference::TopOfStock, 5.0);
        *top_z = Height::of(HeightReference::TopOfStock, 0.0);
        *bottom_z = Height::of(HeightReference::TopOfStock, -2.0);
        *stepdown = 2.0;
        *lead = Lead::None;
        *tolerance = 0.005;
    }
    params
}

fn only_chain(world: &World, sketch_name: &str) -> ChainId {
    let (_, sk) = sketch::find(world, sketch_name).unwrap_or_else(|| panic!("sketch '{sketch_name}'"));
    let ids: Vec<ChainId> = sk.data.chains().all().map(|c| c.id()).collect();
    assert_eq!(ids.len(), 1, "one chain in '{sketch_name}'");
    ids[0]
}

fn chain_on_layer(world: &World, sketch_name: &str, layer: &str) -> ChainId {
    let (_, sk) = sketch::find(world, sketch_name).unwrap_or_else(|| panic!("sketch '{sketch_name}'"));
    let on = sk.data.chains().by_layer(layer);
    assert_eq!(on.len(), 1, "one chain on layer '{layer}'");
    on[0].id()
}

/// The CL arc radii whose centre is near `(cx, cy)`.
fn arc_radii_at(world: &World, op: &str, cx: f64, cy: f64) -> Vec<f64> {
    let e = model::find_by_name(world, op).expect("op");
    let cache = model::toolpath_of(world, &e).expect("cached toolpath");
    cache
        .path
        .moves
        .iter()
        .filter(|m| m.kind == MoveKind::Feed)
        .filter_map(|m| m.arc.map(|a| (a, a.radius_sweep(m.from, m.to).0)))
        .filter(|(a, _)| (a.center.x - cx).abs() < 0.02 && (a.center.y - cy).abs() < 0.02)
        .map(|(_, r)| r)
        .collect()
}

#[test]
fn mixed_closed_and_open_sketch_pick_with_boundary_edits_is_refused_by_name() {
    let (_dir, world, mut history) = sheet_world("mixed", &[("mixed.dxf", mixed_dxf())]);
    let circle = chain_on_layer(&world, "mixed", "holes");
    let line = chain_on_layer(&world, "mixed", "open");
    let picks = vec![
        GeoSel { entity: "mixed".into(), feature: FeatureSel::SketchChain(circle) },
        GeoSel { entity: "mixed".into(), feature: FeatureSel::SketchChain(line) },
    ];
    let params = contour_params();
    run(&world, &mut history, CreateOperation::new(&world, "Cut", TOOL, SrcRef::Selection(picks), params.clone()), "create");
    // Without Boundary edits the mix is legal: the circle and the line both cut.
    run(&world, &mut history, GenerateOperation::new(&world, "Cut"), "generate without boundary edits");

    // A Boundary edit shapes the closed region only; the open line would ride
    // straight through what the edit keeps out. It must be refused by name.
    let area = AreaControls { offset: 1.0, ..AreaControls::default() };
    run(&world, &mut history, EditOperationParams::with_area(&world, "Cut", params, area), "boundary edit");
    let err = GenerateOperation::new(&world, "Cut")
        .err()
        .expect("a mixed closed + open sketch pick with Boundary edits must be refused");
    let msg = err.to_string();
    assert!(msg.contains("Boundary settings"), "{msg}");
    assert!(msg.contains("open sketch chain") && msg.contains("mixed"), "names the open chain: {msg}");
}

#[test]
fn nested_loops_from_separate_sketches_keep_their_side_when_only_reverse_changes() {
    let (_dir, world, mut history) =
        sheet_world("nested", &[("rect.dxf", rect_dxf()), ("ring.dxf", ring_dxf())]);
    let rect = only_chain(&world, "rect");
    let ring = only_chain(&world, "ring");
    let picks = vec![
        GeoSel { entity: "rect".into(), feature: FeatureSel::SketchChain(rect) },
        GeoSel { entity: "ring".into(), feature: FeatureSel::SketchChain(ring) },
    ];
    let params = contour_params();
    run(&world, &mut history, CreateOperation::new(&world, "Plain", TOOL, SrcRef::Selection(picks.clone()), params.clone()), "create plain");
    run(&world, &mut history, GenerateOperation::new(&world, "Plain"), "generate plain");

    let mut reversed = params;
    if let OpParams::Contour { chain_overrides, .. } = &mut reversed {
        *chain_overrides = vec![ChainOverride { entity: "ring".into(), chain: ring, side: None, reverse: true }];
    }
    run(&world, &mut history, CreateOperation::new(&world, "Reversed", TOOL, SrcRef::Selection(picks), reversed), "create reversed");
    run(&world, &mut history, GenerateOperation::new(&world, "Reversed"), "generate reversed");

    // The ring is an outer loop in its own sketch: Auto cuts it Outside, at
    // R10 + 2 = 12, whatever its direction.
    for op in ["Plain", "Reversed"] {
        let radii = arc_radii_at(&world, op, 60.0, 40.0);
        assert!(!radii.is_empty(), "{op}: ring arcs present");
        assert!(radii.iter().all(|r| (r - 12.0).abs() < 0.02), "{op}: ring cut Outside at R12, got {radii:?}");
    }
}
