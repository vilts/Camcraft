//! #1805: machined walls and cavities survive side-on stock carry.
//! Provenance: our own flat-tool fixtures and rigid quarter-turn algebra.
//! Gate map (#1805): covers camcraft_sim::stock_sim (carry_side, carried,
//! carry, field_turn, field_floor, carried_read).

use camcraft_sim::cl::{Move, MoveKind};
use camcraft_sim::geo2d::Point2;
use camcraft_sim::geometry::Point3;
use camcraft_sim::stock_sim::{
    Body, CarriedStock, CarryTurn, DEV_TOL, Dexel, FIELD_BAND, FIELD_SIDE_CARRY_REFUSAL, FieldBody,
    Lattice, SimOp,
};

const X_P90: [[f64; 3]; 3] = [[1.0, 0.0, 0.0], [0.0, 0.0, -1.0], [0.0, 1.0, 0.0]];
const X_M90: [[f64; 3]; 3] = [[1.0, 0.0, 0.0], [0.0, 0.0, 1.0], [0.0, -1.0, 0.0]];

fn stock() -> CarriedStock {
    CarriedStock::Column(Body::on_floor(
        Dexel::block(0.0, 0.0, 12.0, 10.0, 6.0, 1.0).expect("block"),
        0.0,
    ))
}

fn cut(stock: &mut CarriedStock, y: f64, z: f64, radius: f64) {
    let moves = [Move::new(
        MoveKind::Feed,
        Point3::new(-3.0, y, z),
        Point3::new(15.0, y, z),
        Some(300.0),
    )];
    stock
        .simulate(&[SimOp {
            moves: &moves,
            radius,
            ball: false,
            cone: None,
            corner: None,
        }])
        .expect("flat cut");
}

fn turn(stock: CarriedStock, rotation: [[f64; 3]; 3]) -> CarriedStock {
    stock
        .carry_through(&CarryTurn::from_matrix(rotation, [0.0; 3]).expect("quarter turn"))
        .expect("carry")
}

fn top(body: &Body, x: f64, y: f64) -> f64 {
    body.dex
        .top_at(body.dex.col(Point2::new(x, y)).expect("column"))
}

#[test]
fn a_contoured_wall_keeps_its_off_grid_position_after_a_side_turn() {
    let mut stock = stock();
    // The tool flank leaves the wall at y = 8.2 - 2 = 6.2, between
    // column centres. The old column pitch must not become wall error.
    cut(&mut stock, 8.2, 2.0, 2.0);
    let body = turn(stock, X_P90).into_body().expect("body");
    let wall = top(&body, 5.5, -4.5);
    assert!(
        wall >= 6.2 - 1e-9 && wall - 6.2 <= DEV_TOL,
        "turned wall is {wall}; expected 6.2 within {DEV_TOL}"
    );
}

#[test]
fn a_pocket_remains_cut_after_a_side_turn_and_return() {
    let mut stock = stock();
    // A slot across X, y 4..6, has a floor at 2. Standing the plate on
    // its side puts that cavity between two material intervals.
    cut(&mut stock, 5.0, 2.0, 1.0);
    let before = stock.clone().into_body().expect("before");
    assert_eq!(
        top(&before, 5.5, 4.5),
        2.0,
        "the fixture actually cuts the pocket"
    );
    let returned = turn(turn(stock, X_P90), X_M90)
        .into_body()
        .expect("returned body");
    let floor = top(&returned, 5.5, 4.5);
    assert!(
        (floor - 2.0).abs() <= DEV_TOL,
        "the side-on carry filled the pocket: its floor is {floor}"
    );
    assert!(
        (top(&returned, 5.5, 2.5) - 6.0).abs() <= DEV_TOL,
        "material beside the cavity must remain"
    );
}

#[test]
fn the_field_engine_keeps_its_named_refusal_for_a_machined_side_turn() {
    let lat = Lattice {
        x0: 0.0,
        y0: 0.0,
        cell: 1.0,
        nx: 12,
        ny: 10,
    };
    let mut stock = CarriedStock::Field(Box::new(
        FieldBody::block(lat, 0.0, 6.0, FIELD_BAND).expect("field block"),
    ));
    cut(&mut stock, 8.2, 2.0, 2.0);
    let turn = CarryTurn::from_matrix(X_P90, [0.0; 3]).expect("quarter turn");
    assert_eq!(
        stock
            .carry_through(&turn)
            .expect_err("unproved field carry"),
        FIELD_SIDE_CARRY_REFUSAL
    );
}

fn spans_at(body: &Body, x: f64, y: f64) -> Vec<(f64, f64)> {
    let k = body.dex.col(Point2::new(x, y)).expect("column");
    body.material_spans_at(k).expect("material intervals")
}

fn contains(spans: &[(f64, f64)], z: f64) -> bool {
    spans.iter().any(|&(lo, hi)| lo <= z && z <= hi)
}

#[test]
fn a_subcell_cavity_keeps_separate_material_intervals_side_on() {
    let mut stock = stock();
    cut(&mut stock, 5.5, 2.0, 0.15);
    let body = turn(stock, X_P90).into_body().expect("body");
    let spans = spans_at(&body, 5.5, -4.5);
    assert_eq!(
        spans.len(),
        2,
        "the cavity must separate material: {spans:?}"
    );
    assert!(
        !contains(&spans, 5.5),
        "the pocket centre is air: {spans:?}"
    );
    assert!(
        contains(&spans, 5.35 - 1e-7),
        "material just below the cavity is retained: {spans:?}"
    );
    assert!(
        contains(&spans, 5.65 + 1e-7),
        "material just above the cavity is retained: {spans:?}"
    );
    assert!(
        (spans[0].1 - 5.35).abs() <= DEV_TOL && (spans[1].0 - 5.65).abs() <= DEV_TOL,
        "{spans:?}"
    );
}

#[test]
fn a_cut_after_the_side_turn_keeps_the_earlier_cavity() {
    let mut stock = stock();
    cut(&mut stock, 5.0, 2.0, 1.0);
    let mut stock = turn(stock, X_P90);
    cut(&mut stock, -4.5, 7.0, 1.0);
    let body = stock.into_body().expect("body");
    let spans = spans_at(&body, 5.5, -4.5);
    assert_eq!(spans.len(), 2, "{spans:?}");
    assert!(
        contains(&spans, 3.9) && contains(&spans, 6.9),
        "both remaining walls stand: {spans:?}"
    );
    assert!(
        !contains(&spans, 5.0) && !contains(&spans, 7.1),
        "both cuts remove material: {spans:?}"
    );
    assert!(top(&body, 5.5, -4.5) >= 7.0 && top(&body, 5.5, -4.5) - 7.0 <= DEV_TOL);
}

#[test]
fn a_clip_after_the_side_turn_trims_intervals_without_filling_the_cavity() {
    let mut stock = stock();
    cut(&mut stock, 5.0, 2.0, 1.0);
    let mut stock = turn(stock, X_P90);
    stock.clip_under(&Dexel::block(0.0, -6.0, 12.0, 0.0, 7.0, 1.0).expect("clip"));
    let body = stock.into_body().expect("body");
    let spans = spans_at(&body, 5.5, -4.5);
    assert_eq!(spans.len(), 2, "{spans:?}");
    assert!(contains(&spans, 3.9) && contains(&spans, 6.9), "{spans:?}");
    assert!(
        !contains(&spans, 5.0) && !contains(&spans, 7.1),
        "{spans:?}"
    );
    assert_eq!(top(&body, 5.5, -4.5), 7.0);
}

#[test]
fn a_clip_before_the_side_turn_remains_a_bound_in_the_new_frame() {
    let mut stock = stock();
    cut(&mut stock, 5.0, 2.0, 1.0);
    stock.clip_under(&Dexel::block(0.0, 0.0, 12.0, 10.0, 4.0, 1.0).expect("clip"));
    let body = turn(stock, X_P90).into_body().expect("body");
    assert!(
        spans_at(&body, 5.5, -4.5).is_empty(),
        "the clip removed old z > 4"
    );
    let spans = spans_at(&body, 5.5, -3.5);
    assert_eq!(
        spans.len(),
        2,
        "the clipped pocket keeps its cavity: {spans:?}"
    );
    assert!(contains(&spans, 3.9) && contains(&spans, 6.1), "{spans:?}");
    assert!(!contains(&spans, 5.0), "{spans:?}");
}

#[test]
fn all_side_turns_with_translation_return_the_same_machined_wall() {
    let mut original = stock();
    cut(&mut original, 8.2, 2.0, 2.0);
    cut(&mut original, 3.0, 3.0, 0.75);
    let mut checked = 0;
    for permutation in [[0, 2, 1], [1, 2, 0], [2, 0, 1], [2, 1, 0]] {
        for sx in [-1.0, 1.0] {
            for sy in [-1.0, 1.0] {
                for sz in [-1.0, 1.0] {
                    let mut rotation = [[0.0; 3]; 3];
                    for (i, sign) in [sx, sy, sz].into_iter().enumerate() {
                        rotation[i][permutation[i]] = sign;
                    }
                    let shift = [17.0, -9.0, 23.0];
                    let Ok(forward) = CarryTurn::from_matrix(rotation, shift) else {
                        continue;
                    };
                    let inverse = std::array::from_fn(|i| std::array::from_fn(|j| rotation[j][i]));
                    let back_shift = std::array::from_fn(|i| {
                        -(0..3).map(|j| inverse[i][j] * shift[j]).sum::<f64>()
                    });
                    let backward =
                        CarryTurn::from_matrix(inverse, back_shift).expect("inverse turn");
                    let returned = original
                        .clone()
                        .carry_through(&forward)
                        .expect("there")
                        .carry_through(&backward)
                        .expect("back")
                        .into_body()
                        .expect("body");
                    for (y, expected) in [(1.5, 6.0), (2.5, 3.0), (5.5, 6.0), (7.5, 2.0)] {
                        let actual = top(&returned, 5.5, y);
                        assert!(
                            actual >= expected - 1e-9 && actual - expected <= DEV_TOL,
                            "{rotation:?} y {y}: {actual} vs {expected}"
                        );
                    }
                    checked += 1;
                }
            }
        }
    }
    assert_eq!(checked, 16);
}

#[test]
fn exact_side_carry_refuses_non_finite_stock_floors_by_name() {
    let turn = CarryTurn::from_matrix(X_P90, [0.0; 3]).expect("quarter turn");
    for bad in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        let stock = CarriedStock::Column(Body::on_floor(
            Dexel::block(0.0, 0.0, 12.0, 10.0, 6.0, 1.0).expect("block"),
            bad,
        ));
        let error = stock
            .carry_through(&turn)
            .expect_err("non-finite stock floor");
        assert!(error.contains("finite stock floor"), "{bad}: {error}");
    }
}

#[test]
fn exact_side_carry_refuses_a_lattice_that_is_too_tall() {
    let stock = CarriedStock::Column(Body::on_floor(
        Dexel::block(0.0, 0.0, 2.0, 2.0, 5000.0, 1.0).expect("bar"),
        0.0,
    ));
    let turn = CarryTurn::from_matrix(X_P90, [0.0; 3]).expect("quarter turn");
    let error = stock
        .carry_through(&turn)
        .expect_err("turned lattice is too large");
    assert!(error.contains("lattice limit"), "{error}");
}

#[test]
fn an_exact_material_read_refuses_an_out_of_range_column() {
    let body = turn(stock(), X_P90).into_body().expect("body");
    for k in [body.dex.nx * body.dex.ny, usize::MAX] {
        let error = body.material_spans_at(k).expect_err("bad column index");
        assert!(error.contains("outside the lattice"), "{error}");
    }
}

#[test]
fn a_cut_through_after_the_side_turn_still_convicts_the_part() {
    let mut stock = turn(stock(), X_P90);
    cut(&mut stock, -3.5, -1.0, 1.0);
    let body = stock.into_body().expect("body");
    let part =
        camcraft_sim::geometry::cube(Point3::new(0.0, -6.0, 0.0), Point3::new(12.0, 0.0, 10.0));
    let verdict = body.deviate(&part.facets, DEV_TOL).expect("verdict");
    assert!(
        verdict.gouge_cells > 0,
        "an emptied column cannot acquit the part"
    );
    assert!(
        verdict.max_gouge >= 10.0,
        "the whole part thickness was cut through: {}",
        verdict.max_gouge
    );
    assert!(
        spans_at(&body, 5.5, -3.5).is_empty(),
        "the pin actually cuts through the stock"
    );
}

#[test]
fn a_clip_inside_a_turned_cavity_reads_the_lower_material_top() {
    let mut stock = stock();
    cut(&mut stock, 5.0, 2.0, 1.0);
    let mut stock = turn(stock, X_P90);
    stock.clip_under(&Dexel::block(0.0, -6.0, 12.0, 0.0, 5.0, 1.0).expect("clip inside cavity"));
    let body = stock.clone().into_body().expect("body");
    let k = body.dex.col(Point2::new(5.5, -4.5)).expect("column");
    assert!(
        (body.dex.top_at(k) - 4.0).abs() <= DEV_TOL,
        "the top must end at the lower wall, not the clip plane: {}",
        body.dex.top_at(k)
    );
    let spans = body.material_spans_at(k).expect("intervals");
    assert_eq!(spans.len(), 1);
    assert!((spans[0].1 - 4.0).abs() <= DEV_TOL, "{spans:?}");
    let part =
        camcraft_sim::geometry::cube(Point3::new(0.0, -6.0, 0.0), Point3::new(12.0, 0.0, 4.0));
    let verdict = stock
        .deviate(&part.facets, DEV_TOL)
        .expect("carried verdict");
    assert!(
        verdict.dev[k].abs() <= DEV_TOL,
        "the clipped cavity cannot report phantom rest: {}",
        verdict.dev[k]
    );
}
