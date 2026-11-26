//! RepairModel (M13.11): the repair is a command - it lands welded and
//! filled facets as one undo unit, undo restores the exact facets, and
//! every refusal is by name at construction (nothing to repair, bad
//! parameters, non-manifold input).
//!
//! Gate map (M4.4): covers app::commands::repair_model,
//! app::commands::update_model, app::model, camcraft_geometry::geometry::repair.

use camcraft_app::commands::{
    AddTool, CommandError, CreateOperation, History, ImportModel, RepairModel, SetStock, UpdateModel,
};
use camcraft_app::operation::{Height, OpParams, SrcRef};
use camcraft_app::tools::catalog;
use camcraft_app::geometry::repair::open_edge_count;
use camcraft_app::geometry::{cube, Mesh, Point3, Triangle};
use camcraft_app::model;
use camcraft_app::schema::{Location, STOCK_NAME};
use camcraft_app::stock::StockShape;
use camcraft_app::world::new_world;

fn facets(world: &flecs_ecs::prelude::World, name: &str) -> Option<std::sync::Arc<[Triangle]>> {
    model::find_by_name(world, name).and_then(|e| model::facets_of(world, &e))
}

fn holed_cube() -> Mesh {
    // `cube` lays the bottom (z-) quad first: dropping it opens a 4-edge hole.
    Mesh::from_facets(cube(Point3::ZERO, Point3::new(1.0, 1.0, 1.0)).facets[2..].to_vec())
}

#[test]
fn repair_fills_the_hole_and_undo_restores_the_facet_count() {
    let world = new_world();
    let mut history = History::default();
    history.execute(&world, ImportModel::from_mesh("part", holed_cube()));
    let before = facets(&world, "part").expect("imported");
    assert_eq!(before.len(), 10);
    let rev0 = model::find_by_name(&world, "part").map(|e| model::rev_of(&world, &e));

    let command = RepairModel::new(&world, "part", 0.001, 256).expect("repairs");
    assert_eq!((command.report().holes_filled, command.report().triangles_added), (1, 2));
    assert!(command.summary().contains("filled 1 hole"), "{}", command.summary());
    history.execute(&world, command);
    let after = facets(&world, "part").expect("still there");
    assert_eq!(after.len(), 12);
    assert_eq!(open_edge_count(&Mesh::from_shared(after)), 0);

    assert!(history.undo(&world));
    let restored = facets(&world, "part").expect("restored");
    assert_eq!(restored.len(), 10);
    assert_eq!(restored, before, "undo restores the exact previous facets");
    assert_eq!(model::find_by_name(&world, "part").map(|e| model::rev_of(&world, &e)), rev0);

    assert!(history.redo(&world));
    assert_eq!(facets(&world, "part").map(|f| f.len()), Some(12));
}

#[test]
fn a_closed_model_refuses_as_nothing_to_repair() {
    let world = new_world();
    let mut history = History::default();
    history.execute(&world, ImportModel::from_mesh("box", cube(Point3::ZERO, Point3::new(2.0, 2.0, 2.0))));
    let err = RepairModel::new(&world, "box", 0.001, 256).expect_err("nothing to do");
    assert!(matches!(&err, CommandError::Invalid(m) if m.contains("nothing to repair")), "{err}");
    assert!(!history.can_redo() && history.can_undo(), "the refusal never touched the history");
}

#[test]
fn bad_parameters_and_bad_targets_refuse_by_name() {
    let world = new_world();
    let mut history = History::default();
    history.execute(&world, ImportModel::from_mesh("part", holed_cube()));
    assert!(matches!(RepairModel::new(&world, "ghost", 0.001, 256), Err(CommandError::NotFound(_))));
    for tol in [-0.1, f64::NAN, f64::INFINITY] {
        let err = RepairModel::new(&world, "part", tol, 256).expect_err("bad tolerance");
        assert!(err.to_string().contains("weld tolerance"), "{err}");
    }
    let err = RepairModel::new(&world, "part", 0.001, 2).expect_err("bad budget");
    assert!(err.to_string().contains("at least 3 edges"), "{err}");
    // Setup furniture is never repaired as if it were a part.
    history.execute(
        &world,
        SetStock::new(&world, StockShape::Box { dx: 10.0, dy: 10.0, dz: 10.0 }, Location::default())
            .expect("stock"),
    );
    let err = RepairModel::new(&world, STOCK_NAME, 0.001, 256).expect_err("furniture");
    assert!(err.to_string().contains("is not a model"), "{err}");
    assert!(!err.to_string().to_lowercase().contains("furniture"), "{err}");
}

/// #878 review: a tool or an operation reaches the same "not a model"
/// branch as stock does, so its refusal must not call it a setup part.
#[test]
fn tools_and_operations_are_refused_as_not_models_not_setup_parts() {
    let world = new_world();
    let mut history = History::default();
    history.execute(
        &world,
        SetStock::new(&world, StockShape::Box { dx: 10.0, dy: 10.0, dz: 10.0 }, Location::default())
            .expect("stock"),
    );
    let tool = catalog().into_iter().find(|(n, _)| *n == "EM-0400-2FL").map(|(_, t)| t);
    history.execute(&world, AddTool::new(&world, "EM-0400-2FL", tool.expect("catalog")).expect("tool"));
    let params = OpParams::Face {
        holes: Default::default(),
        tilt: None,
        blend_turns: false, blend_radius: None,
        stepover: 3.0,
        clearance_z: Height::fixed(15.0),
        cut_z: Height::fixed(5.0),
        top_z: None,
        stepdown: None,
        feed: 1200.0,
        plunge_feed: 300.0,
        climb: true,
        picks: Default::default(),
    };
    history.execute(
        &world,
        CreateOperation::new(&world, "Face1", "EM-0400-2FL", SrcRef::Stock, params).expect("op"),
    );
    for name in ["EM-0400-2FL", "Face1"] {
        let repair = RepairModel::new(&world, name, 0.001, 256).expect_err("not a model");
        let update = UpdateModel::new(&world, name, cube(Point3::ZERO, Point3::new(1.0, 1.0, 1.0)))
            .err()
            .expect("not a model");
        for err in [repair, update] {
            let text = err.to_string();
            assert!(text.contains("is not a model"), "{text}");
            assert!(!text.contains("part of a setup"), "{text}");
            assert!(!text.to_lowercase().contains("furniture"), "{text}");
        }
    }
}

#[test]
fn non_manifold_input_refuses_by_name_at_construction() {
    let world = new_world();
    let mut history = History::default();
    let mut tris = holed_cube().facets.to_vec();
    tris.push(Triangle::new(
        Point3::new(0.0, 0.0, 1.0),
        Point3::new(1.0, 1.0, 1.0),
        Point3::new(0.5, 0.5, 2.0),
    ));
    history.execute(&world, ImportModel::from_mesh("fin", Mesh::from_facets(tris)));
    let err = RepairModel::new(&world, "fin", 0.001, 256).expect_err("non-manifold");
    assert!(err.to_string().contains("non-manifold edge"), "{err}");
    assert_eq!(facets(&world, "fin").map(|f| f.len()), Some(11), "untouched");
}
