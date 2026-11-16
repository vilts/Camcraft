//! #1805: a fixture can contact a machined face after a side-on carry.
//! Provenance: our own flat-tool wall and contacting rectangular fixture.
//! Gate map (#1805): covers app::fixture_carried (mesh_penetration), app::stock_sim (stock_grid),
//! camcraft_sim::stock_sim (carry_side, carried).

use camcraft_app::fixture::FixtureBox;
use camcraft_app::fixture_carried::{MESH_VISIT_BUDGET, mesh_penetration};
use camcraft_sim::cl::{Move, MoveKind};
use camcraft_sim::fixture_mesh::FixtureMesh;
use camcraft_sim::geometry::{Point3, cube};
use camcraft_sim::stock_sim::{Body, CarriedStock, CarryTurn, Dexel, SimOp};

fn penetration(body: &Body, fixture: &FixtureBox) -> Option<f64> {
    let mesh = FixtureMesh::build(&cube(fixture.min, fixture.max).facets).expect("fixture mesh");
    let mut budget = MESH_VISIT_BUDGET;
    mesh_penetration(body, fixture, &mesh, &mut budget)
}

#[test]
fn a_fixture_touching_the_turned_machined_wall_is_contact() {
    let mut stock = CarriedStock::Column(Body::on_floor(
        Dexel::block(0.0, 0.0, 12.0, 10.0, 6.0, 1.0).expect("block"),
        0.0,
    ));
    // A radius-2 tool at y 8.2 cuts a wall at y 6.2 above the floor z 2.
    let moves = [Move::new(
        MoveKind::Feed,
        Point3::new(-3.0, 8.2, 2.0),
        Point3::new(15.0, 8.2, 2.0),
        Some(300.0),
    )];
    stock
        .simulate(&[SimOp {
            moves: &moves,
            radius: 2.0,
            ball: false,
            cone: None,
            corner: None,
        }])
        .expect("contour");
    let turn = CarryTurn::from_matrix(
        [[1.0, 0.0, 0.0], [0.0, 0.0, -1.0], [0.0, 1.0, 0.0]],
        [0.0; 3],
    )
    .expect("side turn");
    let body = stock
        .carry_through(&turn)
        .expect("carry")
        .into_body()
        .expect("body");
    let contact = FixtureBox {
        min: Point3::new(2.0, -5.0, 6.2),
        max: Point3::new(10.0, -3.0, 8.0),
    };
    assert_eq!(
        penetration(&body, &contact),
        None,
        "the fixture touches the real machined face at z 6.2"
    );
    let collision = FixtureBox {
        min: Point3::new(2.0, -5.0, 6.0),
        max: contact.max,
    };
    assert!(
        penetration(&body, &collision).is_some(),
        "moving the fixture 0.2 mm into the wall must still be refused"
    );
}

#[test]
fn a_fixture_fits_the_turned_cavity_but_not_the_material_beside_it() {
    let mut stock = CarriedStock::Column(Body::on_floor(
        Dexel::block(0.0, 0.0, 12.0, 10.0, 6.0, 1.0).expect("block"),
        0.0,
    ));
    let moves = [Move::new(
        MoveKind::Feed,
        Point3::new(-3.0, 5.0, 2.0),
        Point3::new(15.0, 5.0, 2.0),
        Some(300.0),
    )];
    stock
        .simulate(&[SimOp {
            moves: &moves,
            radius: 1.0,
            ball: false,
            cone: None,
            corner: None,
        }])
        .expect("pocket");
    let turn = CarryTurn::from_matrix(
        [[1.0, 0.0, 0.0], [0.0, 0.0, -1.0], [0.0, 1.0, 0.0]],
        [0.0; 3],
    )
    .expect("side turn");
    let body = stock
        .carry_through(&turn)
        .expect("carry")
        .into_body()
        .expect("body");
    let in_cavity = FixtureBox {
        min: Point3::new(2.0, -5.0, 4.2),
        max: Point3::new(10.0, -3.0, 5.8),
    };
    assert_eq!(
        penetration(&body, &in_cavity),
        None,
        "the cavity remains empty for workholding"
    );
    let in_material = FixtureBox {
        min: Point3::new(2.0, -5.0, 3.8),
        max: in_cavity.max,
    };
    assert!(
        penetration(&body, &in_material).is_some(),
        "the fixture crosses the cavity wall by 0.2 mm"
    );
}

#[test]
fn the_app_stock_grid_keeps_cut_history_through_placement_and_first_carry() {
    use camcraft_app::commands::{History, SetStock};
    use camcraft_app::schema::Location;
    use camcraft_app::stock::StockShape;
    use camcraft_app::stock_sim::stock_grid;
    use camcraft_app::world::new_world;
    use camcraft_sim::geo2d::Point2;
    use camcraft_sim::stock_sim::Stock;

    let world = new_world();
    let mut history = History::default();
    history.execute(
        &world,
        SetStock::new(
            &world,
            StockShape::Box {
                dx: 12.0,
                dy: 10.0,
                dz: 6.0,
            },
            Location {
                pos: Point3::new(6.0, 5.0, -2.0),
                ..Default::default()
            },
        )
        .expect("stock"),
    );
    let grid = stock_grid(&world, None, 8.0).expect("placed stock grid");
    assert_eq!(
        (grid.x0, grid.y0, grid.stock_z, grid.cell),
        (0.0, 0.0, 4.0, 1.0)
    );
    // The app's Held::Plain path cuts this grid before first_body supplies
    // the finite stock floor at the first setup change.
    let mut plain = Stock::Column(grid);
    let moves = [Move::new(
        MoveKind::Feed,
        Point3::new(-3.0, 8.2, 0.0),
        Point3::new(15.0, 8.2, 0.0),
        Some(300.0),
    )];
    plain
        .simulate(&[SimOp {
            moves: &moves,
            radius: 2.0,
            ball: false,
            cone: None,
            corner: None,
        }])
        .expect("contour");
    let body = Body::on_floor(plain.column().expect("column grid").clone(), -2.0);
    let turn = CarryTurn::from_matrix(
        [[1.0, 0.0, 0.0], [0.0, 0.0, -1.0], [0.0, 1.0, 0.0]],
        [0.0; 3],
    )
    .expect("side turn");
    let carried = CarriedStock::Column(body)
        .carry_through(&turn)
        .expect("carry")
        .into_body()
        .expect("body");
    let k = carried.dex.col(Point2::new(5.5, -2.5)).expect("column");
    let top = carried.dex.top_at(k);
    assert!(
        (top - 6.2).abs() <= 0.01 && top >= 6.2 - 1e-9,
        "placed stock wall is {top}, expected 6.2"
    );
}
