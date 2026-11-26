//! Provenance: original Circular ramp fixture and stock-simulation safety checks for #1807.
//! Gate map: covers app::operation (generate), app::stock_sim, camcraft_motion::machining2d (circular).

mod common;

use camcraft_app::cl::MoveKind;
use camcraft_app::commands::{
    AddTool, CreateOperation, GenerateOperation, History, ImportModel, SetStock,
};
use camcraft_app::geo2d::Point2;
use camcraft_app::geometry::Mesh;
use camcraft_app::machining3d::Entry3D;
use camcraft_app::model;
use camcraft_app::operation::{Height, OpParams, SrcRef};
use camcraft_app::stock::StockShape;
use camcraft_app::stock_sim::simulate_document;
use camcraft_app::tools::catalog;
use common::solid_part::{circle, plate, rect};

fn circular_world(stepover: f64, climb: bool) -> flecs_ecs::prelude::World {
    let world = camcraft_app::world::new_world();
    let mut history = History::default();
    let mut facets = Vec::new();
    plate(
        &rect(-5.0, -5.0, 5.0, 5.0),
        Some((&circle((0.0, 0.0), 3.2, 64), 1.0)),
        0.0,
        5.5,
        &mut facets,
    );
    history.execute(
        &world,
        ImportModel::from_mesh("plate", Mesh::from_facets(facets)),
    );
    history.execute(
        &world,
        SetStock::new(
            &world,
            StockShape::Box {
                dx: 10.0,
                dy: 10.0,
                dz: 6.0,
            },
            Default::default(),
        )
        .expect("stock"),
    );
    let tool = catalog()
        .into_iter()
        .find(|(n, _)| *n == "EM-0400-2FL")
        .map(|(_, t)| t)
        .expect("catalog tool");
    history.execute(
        &world,
        AddTool::new(&world, "EM-0400-2FL", tool).expect("tool"),
    );
    let params = OpParams::Circular {
        pass_start: Default::default(),
        links: camcraft_app::links::Links::PLANAR,
        center: Point2::new(0.0, 0.0),
        radius: 3.0,
        vault: false,
        stepover,
        stock_to_leave: 0.0,
        clearance_z: Height::fixed(10.0),
        retract_z: None,
        top_z: Height::fixed(5.492),
        bottom_z: Height::fixed(1.5),
        stepdown: 1.0,
        feed: 500.0,
        plunge_feed: 250.0,
        climb,
        entry: Entry3D::Ramp,
        helix_radius: 1.0,
        tilt: None,
        picks: Default::default(),
    };
    history.execute(
        &world,
        CreateOperation::new(&world, "Circular", "EM-0400-2FL", SrcRef::Stock, params)
            .expect("operation"),
    );
    history.execute(
        &world,
        GenerateOperation::new(&world, "Circular").expect("generate"),
    );
    world
}

#[test]
fn circular_stay_down_keeps_stock_entry_safe_and_does_not_gouge() {
    for climb in [true, false] {
        for (stepover, rings) in [(1.0, 1), (0.5, 2)] {
            let world = circular_world(stepover, climb);
            let entity = model::find_by_name(&world, "Circular").expect("operation entity");
            let cached = model::toolpath_of(&world, &entity).expect("generated path");
            assert!(
                !cached.warnings.iter().any(|w| w.blocks_post()),
                "the stock entry must verify: {:?}",
                cached.warnings
            );
            let first = cached
                .path
                .moves
                .iter()
                .find(|m| m.kind != MoveKind::Rapid && m.to.z < m.from.z)
                .expect("ramp entry");
            assert!(
                first.from.z >= 6.0 && first.from.z <= 6.1 + 1e-9,
                "the first ramp starts just above the stock: {first:?}"
            );
            let lifts: Vec<_> = cached
                .path
                .moves
                .iter()
                .filter(|m| m.kind == MoveKind::Rapid && m.to.z > m.from.z + 1e-9)
                .collect();
            assert_eq!(
                lifts.len(),
                rings,
                "only a ring's final level retracts: {lifts:?}"
            );
            let simulation =
                simulate_document(&world).expect("a verified circular program simulates");
            assert_eq!(
                simulation.dev.gouge_cells, 0,
                "the Circular does not remove part material"
            );
        }
    }
}

#[test]
fn circular_ramp_stay_down_matches_golden() {
    let world = circular_world(0.5, true);
    let entity = model::find_by_name(&world, "Circular").expect("operation");
    let cached = model::toolpath_of(&world, &entity).expect("generated path");
    common::write_golden(
        "circular_ramp_stay_down.cl",
        &common::gen_to_cltext(&cached.path),
    );
}
