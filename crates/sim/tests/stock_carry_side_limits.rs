//! #1805: exact carry refuses work that exceeds its explicit budgets.
//! Provenance: our own stock and outside-cut fixtures at the budget limits.
//! Gate map (#1805): covers camcraft_sim::stock_sim (carry_exact, carry_history).

use camcraft_sim::cl::{Move, MoveKind};
use camcraft_sim::geometry::Point3;
use camcraft_sim::stock_sim::{Body, CarriedStock, CarryTurn, Dexel, SimOp};

fn side() -> CarryTurn {
    CarryTurn::from_matrix(
        [[1.0, 0.0, 0.0], [0.0, 0.0, -1.0], [0.0, 1.0, 0.0]],
        [0.0; 3],
    )
    .expect("side turn")
}

fn cut(stock: &mut CarriedStock, moves: &[Move]) {
    stock
        .simulate(&[SimOp {
            moves,
            radius: 0.1,
            ball: false,
            cone: None,
            corner: None,
        }])
        .expect("valid cuts outside stock");
}

#[test]
fn exact_side_carry_refuses_an_excessive_read_before_building_the_lattice() {
    // Four million columns, with three retained cuts, need sixteen million
    // reads. Each axis itself remains below the 4096-cell lattice limit.
    let mut stock = CarriedStock::Column(Body::on_floor(
        Dexel::block(0.0, 0.0, 2000.0, 2000.0, 2000.0, 1.0).expect("large sparse block"),
        0.0,
    ));
    let outside = Move::new(
        MoveKind::Feed,
        Point3::new(2100.0, 2100.0, 0.0),
        Point3::new(2101.0, 2100.0, 0.0),
        Some(300.0),
    );
    cut(&mut stock, &[outside; 3]);
    assert_eq!(
        stock.carry_through(&side()).err().as_deref(),
        Some("exact side-on carry exceeds its read budget - use a coarser stock grid")
    );
}

#[test]
fn exact_side_carry_refuses_a_ray_with_too_many_retained_cuts() {
    // The one-column read stays below the overall read budget. The cuts
    // miss the stock, so the ray remains occupied until its budget ends.
    let mut stock = CarriedStock::Column(Body::on_floor(
        Dexel::block(0.0, 0.0, 1.0, 1.0, 1.0, 1.0).expect("one-cell block"),
        0.0,
    ));
    let outside = Move::new(
        MoveKind::Feed,
        Point3::new(3.0, 3.0, 0.0),
        Point3::new(4.0, 3.0, 0.0),
        Some(300.0),
    );
    cut(&mut stock, &vec![outside; 100_001]);
    assert_eq!(
        stock.carry_through(&side()).err().as_deref(),
        Some("exact side-on carry exhausted its ray budget")
    );
}

fn refused_cut_keeps_stock(radius: f64, from: Point3, to: Point3, message: &str) {
    let mut stock = CarriedStock::Column(Body::on_floor(
        Dexel::block(0.0, 0.0, 12.0, 10.0, 6.0, 1.0).expect("block"),
        0.0,
    ));
    let before = stock
        .clone()
        .carry_through(&side())
        .expect("uncut side turn")
        .into_body()
        .expect("body");
    let moves = [Move::new(MoveKind::Feed, from, to, Some(300.0))];
    let error = stock
        .simulate(&[SimOp {
            moves: &moves,
            radius,
            ball: false,
            cone: None,
            corner: None,
        }])
        .expect_err("invalid exact sweep");
    assert!(error.to_string().contains(message), "{error}");
    let after = stock
        .carry_through(&side())
        .expect("a refused cut leaves carry valid")
        .into_body()
        .expect("body");
    assert_eq!(after.volume(), before.volume());
    for k in 0..before.dex.nx * before.dex.ny {
        assert_eq!(
            after.dex.top_at(k),
            before.dex.top_at(k),
            "top changed at column {k}"
        );
        assert_eq!(
            after.floor_at(k),
            before.floor_at(k),
            "floor changed at column {k}"
        );
    }
}

#[test]
fn exact_carry_refuses_radius_squared_overflow_without_changing_stock() {
    refused_cut_keeps_stock(
        1e200,
        Point3::new(0.0, 0.0, 1.0),
        Point3::new(1.0, 0.0, 1.0),
        "exact side-on carry tool coordinates exceed finite geometric arithmetic",
    );
}

#[test]
fn exact_carry_refuses_chord_length_overflow_without_changing_stock() {
    for end in [1e200, f64::MAX] {
        refused_cut_keeps_stock(
            1.0,
            Point3::new(-end, 0.0, 1.0),
            Point3::new(end, 0.0, 1.0),
            "exact side-on carry tool coordinates exceed finite geometric arithmetic",
        );
    }
}

#[test]
fn exact_carry_refuses_flank_overflow_without_changing_stock() {
    refused_cut_keeps_stock(
        f64::MAX,
        Point3::new(0.0, 0.0, 1.0),
        Point3::new(1.0, 0.0, 1.0),
        "exact side-on carry tool span is not finite",
    );
}
