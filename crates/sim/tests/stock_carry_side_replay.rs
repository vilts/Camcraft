//! #1805: side-on carry replays each supported cutter without hiding stock.
//! Provenance: independent original-column reads of our own tool fixtures.
//! Gate map (#1805): covers camcraft_sim::stock_sim (carry_history, carry_exact, cut).

use camcraft_sim::cl::{Move, MoveKind};
use camcraft_sim::geo2d::Point2;
use camcraft_sim::geometry::Point3;
use camcraft_sim::stock_sim::{Body, CarriedStock, CarryTurn, DEV_TOL, Dexel, SimOp};

fn replay(ball: bool, cone: Option<f64>, corner: Option<f64>, arc: bool, rise: f64) {
    let mut stock = CarriedStock::Column(Body::on_floor(
        Dexel::block(0.0, 0.0, 12.0, 10.0, 6.0, 1.0).expect("block"),
        0.0,
    ));
    let motion = if arc {
        Move::new(
            MoveKind::Feed,
            Point3::new(3.2, 5.2, 1.2),
            Point3::new(9.2, 5.2, 1.2 + rise),
            Some(300.0),
        )
        .with_arc(Point2::new(6.2, 5.2), false)
    } else {
        Move::new(
            MoveKind::Feed,
            Point3::new(-3.0, 5.2, 1.2),
            Point3::new(15.0, 5.2, 1.2),
            Some(300.0),
        )
    };
    let moves = [motion];
    stock
        .simulate(&[SimOp {
            moves: &moves,
            radius: 2.0,
            ball,
            cone,
            corner,
        }])
        .expect("cut");
    let before = stock.clone().into_body().expect("before");
    let there = CarryTurn::from_matrix(
        [[1.0, 0.0, 0.0], [0.0, 0.0, -1.0], [0.0, 1.0, 0.0]],
        [0.0; 3],
    )
    .expect("there");
    let back = CarryTurn::from_matrix(
        [[1.0, 0.0, 0.0], [0.0, 0.0, 1.0], [0.0, -1.0, 0.0]],
        [0.0; 3],
    )
    .expect("back");
    let returned = stock
        .carry_through(&there)
        .expect("there")
        .carry_through(&back)
        .expect("back")
        .into_body()
        .expect("body");
    assert_eq!(
        (returned.dex.nx, returned.dex.ny),
        (before.dex.nx, before.dex.ny)
    );
    for k in 0..before.dex.nx * before.dex.ny {
        let (old, new) = (before.dex.top_at(k), returned.dex.top_at(k));
        assert!(
            new >= old - 1e-8,
            "column {k}: replay hid material, {new} < {old}"
        );
        assert!(
            new - old <= DEV_TOL,
            "column {k}: replay added excess rest, {new} vs {old}"
        );
    }
}

#[test]
fn a_straight_ball_cut_survives_side_on_replay() {
    replay(true, None, None, false, 0.0);
}

#[test]
fn a_straight_bull_cut_survives_side_on_replay() {
    replay(false, None, Some(0.5), false, 0.0);
}

#[test]
fn a_straight_cone_cut_survives_side_on_replay() {
    replay(false, Some(1.0), None, false, 0.0);
}

#[test]
fn an_xy_arc_flat_cut_survives_side_on_replay() {
    replay(false, None, None, true, 0.0);
}

#[test]
fn an_xy_arc_ball_cut_survives_side_on_replay() {
    replay(true, None, None, true, 0.0);
}

#[test]
fn an_xy_arc_bull_cut_survives_side_on_replay() {
    replay(false, None, Some(0.5), true, 0.0);
}

#[test]
fn an_xy_arc_cone_cut_survives_side_on_replay() {
    replay(false, Some(1.0), None, true, 0.0);
}

#[test]
fn a_helical_cone_cut_survives_side_on_replay() {
    replay(false, Some(1.0), None, true, 1.0);
}

#[test]
fn a_turned_cone_cut_has_no_material_skin_where_its_shank_starts() {
    let mut stock = CarriedStock::Column(Body::on_floor(
        Dexel::block(0.0, 0.0, 12.0, 10.0, 6.0, 1.0).expect("block"),
        0.0,
    ));
    // Cone radius 2, cotangent 1, tip 1.5: the shank starts at old z 3.5.
    // That plane is inside the swept tool, not a material boundary.
    let moves = [Move::new(
        MoveKind::Feed,
        Point3::new(-3.0, 5.2, 1.5),
        Point3::new(15.0, 5.2, 1.5),
        Some(300.0),
    )];
    stock
        .simulate(&[SimOp {
            moves: &moves,
            radius: 2.0,
            ball: false,
            cone: Some(1.0),
            corner: None,
        }])
        .expect("cone cut");
    let turn = CarryTurn::from_matrix(
        [[1.0, 0.0, 0.0], [0.0, 0.0, -1.0], [0.0, 1.0, 0.0]],
        [0.0; 3],
    )
    .expect("turn");
    let body = stock
        .carry_through(&turn)
        .expect("carry")
        .into_body()
        .expect("body");
    for (old_z, radius) in [(2.5, 1.0), (3.5, 2.0), (4.5, 2.0)] {
        let k = body.dex.col(Point2::new(5.5, -old_z)).expect("column");
        let spans = body.material_spans_at(k).expect("material intervals");
        assert_eq!(
            spans.len(),
            2,
            "old z {old_z}: the cut must leave a cavity, {spans:?}"
        );
        assert!(
            (spans[0].1 - (5.2 - radius)).abs() <= DEV_TOL,
            "old z {old_z}: lower wall {spans:?}"
        );
        assert!(
            (spans[1].0 - (5.2 + radius)).abs() <= DEV_TOL,
            "old z {old_z}: upper wall {spans:?}"
        );
        assert!(
            !spans.iter().any(|&(lo, hi)| lo <= 5.2 && 5.2 <= hi),
            "old z {old_z}: a false skin fills the tool axis"
        );
    }
}

fn flat_path_after_turn(from: Point3, to: Point3) {
    let mut stock = CarriedStock::Column(Body::on_floor(
        Dexel::block(0.0, 0.0, 12.0, 10.0, 6.0, 1.0).expect("block"),
        0.0,
    ));
    let moves = [Move::new(MoveKind::Feed, from, to, Some(300.0))];
    stock
        .simulate(&[SimOp {
            moves: &moves,
            radius: 1.3,
            ball: false,
            cone: None,
            corner: None,
        }])
        .expect("flat cut");
    let before = stock.clone().into_body().expect("before");
    let there = CarryTurn::from_matrix(
        [[1.0, 0.0, 0.0], [0.0, 0.0, -1.0], [0.0, 1.0, 0.0]],
        [0.0; 3],
    )
    .expect("there");
    let back = CarryTurn::from_matrix(
        [[1.0, 0.0, 0.0], [0.0, 0.0, 1.0], [0.0, -1.0, 0.0]],
        [0.0; 3],
    )
    .expect("back");
    let turned = stock.carry_through(&there).expect("carry");
    let body = turned.clone().into_body().expect("side body");
    for ix in 0..12 {
        for iz in 0..6 {
            let (x, z) = (ix as f64 + 0.5, iz as f64 + 0.5);
            let k = body.dex.col(Point2::new(x, -z)).expect("side column");
            let spans = body.material_spans_at(k).expect("side intervals");
            for iy in 0..10 {
                let y = iy as f64 + 0.5;
                let top = before.dex.top_at(ix + iy * 12);
                let material = spans
                    .iter()
                    .any(|&(lo, hi)| lo - 1e-8 <= y && y <= hi + 1e-8);
                if z <= top {
                    assert!(
                        material,
                        "lost material at ({x}, {y}, {z}), top {top}: {spans:?}"
                    );
                } else if z > top + DEV_TOL {
                    assert!(
                        !material,
                        "cut filled at ({x}, {y}, {z}), top {top}: {spans:?}"
                    );
                }
            }
        }
    }
    let returned = turned
        .carry_through(&back)
        .expect("return")
        .into_body()
        .expect("returned body");
    for k in 0..120 {
        let (old, new) = (before.dex.top_at(k), returned.dex.top_at(k));
        assert!(
            new >= old - 1e-8 && new - old <= DEV_TOL,
            "column {k}: {new} vs {old}"
        );
    }
}

#[test]
fn a_rising_flat_move_keeps_its_slope_after_a_side_turn() {
    flat_path_after_turn(Point3::new(-2.0, 5.17, 0.23), Point3::new(14.0, 5.17, 4.23));
}

#[test]
fn a_falling_flat_move_keeps_its_slope_after_a_side_turn() {
    flat_path_after_turn(Point3::new(-2.0, 5.17, 4.23), Point3::new(14.0, 5.17, 0.23));
}

#[test]
fn a_diagonal_flat_move_keeps_its_footprint_after_a_side_turn() {
    flat_path_after_turn(Point3::new(-2.0, -1.2, 1.23), Point3::new(14.0, 11.2, 1.23));
}

#[test]
fn a_cut_after_a_high_lattice_flip_reaches_the_current_stock_top() {
    let stock = CarriedStock::Column(Body::on_floor(
        Dexel::block(0.0, 0.0, 12.0, 10.0, 6.0, 1.0).expect("block"),
        0.0,
    ));
    let flip = CarryTurn::from_matrix(
        [[1.0, 0.0, 0.0], [0.0, -1.0, 0.0], [0.0, 0.0, -1.0]],
        [0.0, 0.0, 100.0],
    )
    .expect("translated flip");
    let mut stock = stock.carry_through(&flip).expect("flip");
    let moves = [Move::new(
        MoveKind::Feed,
        Point3::new(-3.0, -5.17, 96.0),
        Point3::new(15.0, -5.17, 96.0),
        Some(300.0),
    )];
    stock
        .simulate(&[SimOp {
            moves: &moves,
            radius: 0.6,
            ball: false,
            cone: None,
            corner: None,
        }])
        .expect("cut after flip");
    let before = stock.clone().into_body().expect("before side turn");
    let k = before.dex.col(Point2::new(5.5, -5.5)).expect("cut column");
    assert_eq!(
        before.dex.top_at(k),
        96.0,
        "the fixture cuts the flipped stock"
    );
    let there = CarryTurn::from_matrix(
        [[1.0, 0.0, 0.0], [0.0, 0.0, -1.0], [0.0, 1.0, 0.0]],
        [0.0; 3],
    )
    .expect("there");
    let back = CarryTurn::from_matrix(
        [[1.0, 0.0, 0.0], [0.0, 0.0, 1.0], [0.0, -1.0, 0.0]],
        [0.0; 3],
    )
    .expect("back");
    let turned = stock.carry_through(&there).expect("side turn");
    let body = turned.clone().into_body().expect("side body");
    let k = body
        .dex
        .col(Point2::new(5.5, -99.5))
        .expect("column near old top");
    let spans = body.material_spans_at(k).expect("side intervals");
    assert!(
        !spans.iter().any(|&(lo, hi)| lo <= -5.5 && -5.5 <= hi),
        "the shank must remove stock up to current z 100, not stop at the old top: {spans:?}"
    );
    let returned = turned
        .carry_through(&back)
        .expect("return")
        .into_body()
        .expect("returned body");
    let k = returned
        .dex
        .col(Point2::new(5.5, -5.5))
        .expect("returned column");
    assert!(
        (returned.dex.top_at(k) - 96.0).abs() <= DEV_TOL,
        "{}",
        returned.dex.top_at(k)
    );
}
