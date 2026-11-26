//! M7.2 D (app half): what the program leaves, as a mesh. The traced
//! stock's field extracts to a closed, outward shell whose volume is
//! the stock minus what the Face program removed; it round-trips
//! through our own STL importer facet for facet; and the mesh cache
//! extracts once per field view, never serving a stale one.
//! Provenance: our own fixtures over the M7.1 field engine; nothing
//! from vendor binaries.
//!
//! Gate map (M4.4): covers app::stock_export, sim::stock_sim::iso, app::geometry::import::stl.

use std::sync::Arc;

use camcraft_app::commands::{AddTool, CreateOperation, GenerateOperation, History, ImportModel, SetStock};
use camcraft_app::geometry::import::stl;
use camcraft_app::geometry::{cube, Point3, Triangle, IDENTITY_Q};
use camcraft_app::operation::{Height, OpParams, SrcRef};
use camcraft_app::schema::Location;
use camcraft_app::stock::StockShape;
use camcraft_app::stock_export::{machined_mesh, write_stl, MeshCache};
use camcraft_app::stock_view::field_view_with;
use camcraft_app::tools::catalog;
use camcraft_app::world::new_world;

const HALF: f64 = 6.0;
const COARSE: f64 = (2.0 * HALF) * (2.0 * HALF) * 20.0 / 3.6;

/// A 12x12x20 box faced to z 18 over a 12x12x18 model.
fn faced_world() -> flecs_ecs::prelude::World {
    let world = new_world();
    let mut history = History::default();
    let loc = Location { pos: Point3::ZERO, rot: IDENTITY_Q };
    let cmd = SetStock::new(&world, StockShape::Box { dx: 2.0 * HALF, dy: 2.0 * HALF, dz: 20.0 }, loc);
    history.execute(&world, cmd.expect("box stock"));
    let tool = catalog().into_iter().find(|(n, _)| *n == "EM-0400-2FL").map(|(_, t)| t).expect("catalog entry");
    history.execute(&world, AddTool::new(&world, "EM-0400-2FL", tool).expect("catalog tool"));
    history.execute(&world, ImportModel::from_mesh("part", cube(Point3::new(-HALF, -HALF, 0.0), Point3::new(HALF, HALF, 18.0))));
    let params = OpParams::Face { holes: Default::default(),
        tilt: None,
        stepover: 3.0,
        clearance_z: Height::fixed(25.0),
        cut_z: Height::fixed(18.0),
        top_z: None,
        stepdown: None,
        feed: 1200.0,
        plunge_feed: 300.0,
        climb: true,
        blend_turns: false, blend_radius: None,
        picks: Default::default(),
    };
    history.execute(&world, CreateOperation::new(&world, "Face1", "EM-0400-2FL", SrcRef::Stock, params).expect("create face"));
    history.execute(&world, GenerateOperation::new(&world, "Face1").expect("generate face"));
    world
}

fn volume(tris: &[Triangle]) -> f64 {
    tris.iter().map(|t| t.a.dot(t.b.cross(t.c)) / 6.0).sum()
}

#[test]
fn the_machined_stock_extracts_closed_with_the_faced_volume() {
    let world = faced_world();
    let fv = field_view_with(&world, COARSE).expect("field view");
    let tris = machined_mesh(&fv).expect("a closed shell");
    let v = volume(&tris);
    let (uncut, faced) = (12.0 * 12.0 * 20.0, 12.0 * 12.0 * 18.0);
    // Outward and closed (machined_mesh refuses open edges); the 2 mm
    // skim is gone within the cell's own chamfer of the box edges.
    assert!(v > 0.0, "outward-facing: {v}");
    assert!(v < uncut - 0.5 * 12.0 * 12.0 * 2.0, "the facing removed material: {v} vs uncut {uncut}");
    assert!((v - faced).abs() < 0.08 * faced, "volume {v} vs the faced {faced}");
}

#[test]
fn the_stl_round_trips_through_our_importer_and_the_cache_keys_on_the_view() {
    let world = faced_world();
    let fv = Arc::new(field_view_with(&world, COARSE).expect("field view"));
    let dir = std::env::temp_dir().join(format!("camcraft-stock-export-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("temp dir");
    let path = dir.join("machined.stl");
    let n = write_stl(&fv, &path).expect("written");
    let back = stl::parse_file(&path).expect("our importer reads it");
    assert_eq!(back.facets.len(), n, "facet for facet");
    let _ = std::fs::remove_dir_all(&dir);

    let mut cache = MeshCache::default();
    let a = cache.get(&fv).expect("mesh");
    let b = cache.get(&fv).expect("mesh");
    assert!(Arc::ptr_eq(&a, &b), "the same view is not re-extracted");
    let fv2 = Arc::new(field_view_with(&world, COARSE).expect("field view"));
    let c = cache.get(&fv2).expect("mesh");
    assert!(!Arc::ptr_eq(&a, &c), "a new view is a new mesh, never the stale one");
    cache.clear();
}
