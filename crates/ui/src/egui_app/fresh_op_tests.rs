//! #214: a ribbon quick-add of a depth operation (2D Contour, Pocket, ...)
//! is created whatever the work-offset convention. With G54 Z zeroed on
//! the part's top - the common shop habit - the old default bottom (the
//! G54 plane) resolved level with the top, and `CreateOperation` refused a
//! zero-depth operation before the user could open its Heights tab.
//! Provenance: our own tests over our own seed defaults.

use camcraft_app::commands::{
    AddTool, CreateOperation, History, ImportModel, SetStock, SetWcsOrigin,
};
use camcraft_app::geometry::{IDENTITY_Q, Point3, cube};
use camcraft_app::operation::{Height, HeightReference, KindInfo, OpParams, SrcRef};
use camcraft_app::schema::Location;
use camcraft_app::stock::StockShape;
use camcraft_app::tools::catalog;
use camcraft_app::world::new_world;
use flecs_ecs::prelude::World;

use crate::operations::quick_params;

const TOOL: &str = "EM-0400-2FL";

/// 60x60x20 box stock, a 40x40x10 part standing on z 0, one flat end
/// mill, and G54 placed at the part's TOP (z 10).
fn part_with_g54_on_top() -> World {
    let world = new_world();
    let mut history = History::default();
    let stock = SetStock::new(
        &world,
        StockShape::Box {
            dx: 60.0,
            dy: 60.0,
            dz: 20.0,
        },
        Location::default(),
    );
    history.execute(&world, stock.expect("box stock"));
    history.execute(
        &world,
        ImportModel::from_mesh(
            "part",
            cube(Point3::new(0.0, 0.0, 0.0), Point3::new(40.0, 40.0, 10.0)),
        ),
    );
    let tool = catalog()
        .into_iter()
        .find(|(n, _)| *n == TOOL)
        .map(|(_, t)| t)
        .expect("catalog tool");
    history.execute(&world, AddTool::new(&world, TOOL, tool).expect("tool"));
    let top = Location {
        pos: Point3::new(0.0, 0.0, 10.0),
        rot: IDENTITY_Q,
    };
    history.execute(
        &world,
        SetWcsOrigin::new(&world, top).expect("G54 on the part top"),
    );
    world
}

#[test]
fn a_fresh_depth_operation_is_created_with_g54_on_the_part_top() {
    let world = part_with_g54_on_top();
    for kind in [1usize, 2, 3, 4] {
        let label = KindInfo::ALL.get(kind).map_or("?", |k| k.label);
        let made = CreateOperation::new(
            &world,
            &format!("Op{kind}"),
            TOOL,
            SrcRef::Stock,
            quick_params(kind),
        );
        assert!(
            made.is_ok(),
            "{label} must be created with G54 on the part top: {:?}",
            made.err()
        );
    }
}

/// The old default, for contrast: a bottom on the G54 plane is zero depth
/// here, and creation refuses it - the failure #214 reported.
#[test]
fn a_bottom_on_the_g54_plane_is_refused_when_g54_sits_on_the_part_top() {
    let world = part_with_g54_on_top();
    let mut params = quick_params(1);
    if let OpParams::Contour { bottom_z, .. } = &mut params {
        *bottom_z = Height::of(HeightReference::WorkOffset, 0.0);
    } else {
        panic!("kind 1 is a 2D Contour");
    }
    let err = CreateOperation::new(&world, "Contour1", TOOL, SrcRef::Stock, params)
        .err()
        .map(|e| e.to_string())
        .unwrap_or_default();
    assert!(err.contains("must cut downward"), "{err}");
}

/// #1409: a ribbon quick-add of a lathe Facing faces the stock's
/// allowance down to the part's top - the same heights the MCP seed
/// (`operation::seed_params`) gives - never the mill fold's Top/Bottom
/// of Model, whose floor made the facing plane the part's own bottom.
#[test]
fn a_quick_add_facing_runs_from_the_stock_top_to_the_part_top() {
    let kind = camcraft_app::operation::OpType::Facing.ordinal();
    let OpParams::Facing { top_z, bottom_z, .. } = quick_params(kind) else {
        panic!("the Facing row quick-adds a Facing");
    };
    assert_eq!((top_z.reference, top_z.offset), (HeightReference::TopOfStock, 0.0));
    assert_eq!((bottom_z.reference, bottom_z.offset), (HeightReference::TopOfModel, 0.0));
    assert_eq!(quick_params(kind), camcraft_app::operation::seed_params(kind), "quick-add and MCP seed agree");
}
