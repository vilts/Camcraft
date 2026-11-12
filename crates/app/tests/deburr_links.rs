//! #304 through the operation layer: a deburr's edge runs are linked
//! by a vertical retract out of each break, then the traverse at the
//! clearance plane. Ridden on a finished part (the stock is the part,
//! so every rapid that crosses it crosses metal), the generator's
//! verify finds no rapid in stock.
//!
//! Gate map (#304): covers motion::finishing3d (deburr), app::operation
//! (generate), sim::verify.

use camcraft_app::cl::MoveKind;
use camcraft_app::commands::{
    AddTool, CreateOperation, GenerateOperation, History, ImportModel, SetStock,
};
use camcraft_app::geometry::{Point3, cube};
use camcraft_app::model;
use camcraft_app::operation::{Height, OpParams, SrcRef};
use camcraft_app::stock::StockShape;
use camcraft_app::tools::catalog;
use camcraft_app::verify::Warning;
use camcraft_app::world::new_world;

/// The 90-degree chamfer mill the deburr suite rides.
const CONE: &str = "CM-0600-90";
/// The rapids plane, above the brick's 10 mm top.
const CLEARANCE: f64 = 25.0;

/// The brick and a second block standing beside it, one model: two
/// top rims, so the ride is two runs and has a link to test (#303 took
/// the vertical corner edges out of the ride - they were the other runs).
fn two_bricks() -> camcraft_app::geometry::Mesh {
    let mut f: Vec<_> = cube(Point3::new(-20.0, -10.0, 0.0), Point3::new(20.0, 10.0, 10.0)).facets.to_vec();
    f.extend(cube(Point3::new(25.0, -5.0, 0.0), Point3::new(35.0, 5.0, 10.0)).facets.iter().copied());
    camcraft_app::geometry::Mesh::from_facets(f)
}

#[test]
fn a_deburr_on_a_finished_brick_links_its_runs_without_a_rapid_in_stock() {
    let world = new_world();
    let mut history = History::default();
    history.execute(
        &world,
        ImportModel::from_mesh("brick", two_bricks()),
    );
    // The stock is the brick itself: a finished part, the deburr's
    // own premise, so a rapid through it is a rapid through the part.
    history.execute(
        &world,
        SetStock::new(
            &world,
            StockShape::FromModel {
                source: "brick".into(),
            },
            Default::default(),
        )
        .expect("stock"),
    );
    let tool = catalog()
        .into_iter()
        .find(|(n, _)| *n == CONE)
        .map(|(_, t)| t)
        .expect("catalog cone");
    history.execute(&world, AddTool::new(&world, CONE, tool).expect("tool"));
    // The operations suite's ride: a 2-wide break, tips in z 6..8 (the
    // top rim and the top ends of the four vertical edges).
    let params = OpParams::Deburr {
        width: 2.0,
        clearance_z: Height::fixed(CLEARANCE),
        top_z: Height::fixed(8.0),
        bottom_z: Height::fixed(6.0),
        feed: 500.0,
        plunge_feed: 100.0,
    };
    history.execute(
        &world,
        CreateOperation::new(&world, "BRK", CONE, SrcRef::Entity("brick".into()), params)
            .expect("create"),
    );
    history.execute(
        &world,
        GenerateOperation::new(&world, "BRK").expect("generate"),
    );
    let e = model::find_by_name(&world, "BRK").expect("op");
    let tp = model::toolpath_of(&world, &e).expect("cached path");

    let runs = tp
        .path
        .moves
        .iter()
        .filter(|m| m.kind == MoveKind::Plunge)
        .count();
    assert!(
        runs >= 2,
        "the brick must ride as several runs for its links to be tested: {runs}"
    );
    let rapids_in_stock: Vec<_> = tp
        .warnings
        .iter()
        .filter(|w| matches!(w, Warning::RapidInStock { .. }))
        .collect();
    assert!(
        rapids_in_stock.is_empty(),
        "a link rapids through the part: {rapids_in_stock:?}"
    );
    for m in tp
        .path
        .moves
        .iter()
        .filter(|m| m.kind == MoveKind::Rapid && m.from.z < CLEARANCE - 1e-9)
    {
        assert!(
            (m.from.x - m.to.x).abs() < 1e-9 && (m.from.y - m.to.y).abs() < 1e-9,
            "a rapid leaves cutting depth diagonally: {:?} -> {:?}",
            m.from,
            m.to
        );
    }
}
