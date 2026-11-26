//! M7.3: the column reader across the seam (`Stock::columns`). The
//! retirement march's generator-side consumers - the rest ladder's
//! and the re-machining mask's `above(z)`, the feed sampler's
//! per-point top - ask a stock STRICT questions at exactly the
//! planes the program cut, so the field's floors must read ON their
//! plane, bit for bit, or the mask law flips a whole floor. These
//! pin the closed-form floor entry (`field_ray::flat_floor_entry`)
//! and the shared strip law on a small block; the goldens-scale
//! proof is app's heavy `stock_engine_agreement`.
//! Provenance: this project's own parity law (M7.1 C) - nothing from
//! vendor binaries.
//!
//! Gate map (M4.4): covers camcraft_sim::stock_sim::columns and the
//! flat floor entry of camcraft_sim::stock_sim::field_ray.

use camcraft_sim::cl::{Move, MoveKind};
use camcraft_sim::geo2d::Point2;
use camcraft_sim::geometry::Point3;
use camcraft_sim::stock_sim::{Backend, SimOp, Stock, FIELD_BAND};

fn p(x: f64, y: f64, z: f64) -> Point3 {
    Point3::new(x, y, z)
}

fn mv(kind: MoveKind, from: Point3, to: Point3) -> Move {
    Move { rotary: None, kind, from, to, feed: Some(500.0), arc: None, axis: None }
}

/// A plunge, a level pass at -1.3 (not a lattice plane), a second
/// plunge deeper - the floors a pocket's ladder leaves.
fn program() -> Vec<Move> {
    vec![
        mv(MoveKind::Rapid, p(2.0, 2.0, 5.0), p(2.0, 2.0, 1.0)),
        mv(MoveKind::Feed, p(2.0, 2.0, 1.0), p(2.0, 2.0, -1.3)),
        mv(MoveKind::Feed, p(2.0, 2.0, -1.3), p(7.0, 2.0, -1.3)),
        mv(MoveKind::Feed, p(7.0, 2.0, -1.3), p(7.0, 6.0, -1.3)),
        mv(MoveKind::Feed, p(7.0, 6.0, -1.3), p(7.0, 6.0, -2.5)),
    ]
}

fn both() -> (Stock, Stock) {
    let block = |b| Stock::block(b, 0.0, 0.0, 9.0, 9.0, 0.0, 0.5).expect("the test block");
    let (mut c, mut f) = (block(Backend::Column), block(Backend::Field { band: FIELD_BAND }));
    let moves = program();
    let ops = [SimOp { moves: &moves, radius: 1.5, ball: false, cone: None, corner: None }];
    c.simulate(&ops).expect("column program");
    f.simulate(&ops).expect("field program");
    (c, f)
}

#[test]
fn a_flat_floor_reads_on_its_own_plane_on_both_arms() {
    let (c, f) = both();
    let (cc, cf) = (c.columns().expect("column reader"), f.columns().expect("field reader"));
    assert_eq!((cc.nx(), cc.ny()), (cf.nx(), cf.ny()), "one lattice");
    let mut floors = 0;
    for iy in 0..cc.ny() {
        for ix in 0..cc.nx() {
            let (a, b) = (cc.top_at(ix, iy), cf.top_at(ix, iy));
            // Bit for bit: the floors, the second plunge's pit, and
            // the untouched stock top.
            assert_eq!(a.to_bits(), b.to_bits(), "column [{ix},{iy}]: column {a:e} vs field {b:e}");
            if a == -1.3 {
                floors += 1;
            }
        }
    }
    assert!(floors > 20, "the level pass left a floor ({floors} columns)");
    // The per-point read is the same column on both arms.
    for q in [Point2::new(4.3, 2.1), Point2::new(7.0, 6.0), Point2::new(0.1, 8.9), Point2::new(9.5, 1.0)] {
        assert_eq!(cc.top_over(q).map(f64::to_bits), cf.top_over(q).map(f64::to_bits), "at {q:?}");
    }
}

#[test]
fn a_column_on_the_flank_is_cut_on_both_arms() {
    // The tie the contour golden found: a cell center EXACTLY one tool
    // radius from a level pass. The column's reach is inclusive (the
    // disc touches the center, the column drops to the floor); the
    // field's flank must say the same, while the floor plane itself
    // stays steel on both.
    let block = |b| Stock::block(b, 0.0, 0.0, 6.0, 6.0, 0.0, 0.5).expect("the test block");
    let (mut c, mut f) = (block(Backend::Column), block(Backend::Field { band: FIELD_BAND }));
    // Centers sit at k + 0.25 / k + 0.75; a pass along y = 1.75 with
    // r = 1.5 has the y = 3.25 row on its flank, exactly.
    let moves = [mv(MoveKind::Feed, p(1.0, 1.75, -1.0), p(5.0, 1.75, -1.0))];
    let ops = [SimOp { moves: &moves, radius: 1.5, ball: false, cone: None, corner: None }];
    c.simulate(&ops).expect("column program");
    f.simulate(&ops).expect("field program");
    let (cc, cf) = (c.columns().expect("column reader"), f.columns().expect("field reader"));
    let row = cc.col(Point2::new(3.25, 3.25)).expect("on the lattice").1;
    for ix in 0..cc.nx() {
        let (a, b) = (cc.top_at(ix, row), cf.top_at(ix, row));
        assert_eq!(a.to_bits(), b.to_bits(), "flank row column {ix}: column {a:e} vs field {b:e}");
    }
    assert_eq!(cc.top_at(6, row), -1.0, "the flank row was cut (the column's inclusive reach)");
}

#[test]
fn the_masks_agree_at_the_planes_the_program_cut() {
    let (c, f) = both();
    let (cc, cf) = (c.columns().expect("column reader"), f.columns().expect("field reader"));
    for z in [0.0, -1.3, -2.5, -0.75] {
        let (rc, rf) = (cc.above(z).expect("column mask"), cf.above(z).expect("field mask"));
        let lost = rc.difference(&rf).expect("difference").area();
        let gained = rf.difference(&rc).expect("difference").area();
        assert!(lost <= 1e-9 && gained <= 1e-9, "above({z}): column {} vs field {} (column-only {lost}, field-only {gained})", rc.area(), rf.area());
    }
    // The mask at a floor's own plane excludes that floor: the strict law.
    let full = cc.above(-2.6).expect("mask").area();
    assert!(cf.above(-1.3).expect("mask").area() < full, "the -1.3 floor is not above -1.3");
}

/// #3819: a caller that cuts one move for each `simulate` call (a verifier
/// that reads the stock before each cutting move) gets the one-call stock
/// to the bit. The history now grows in place (pinned in `cut_history_tests`).
#[test]
fn one_move_for_each_call_is_the_one_call_cut() {
    let moves: Vec<Move> = (0..4000)
        .map(|k| {
            let (y, z) = (0.25 + (k % 200) as f64 * 0.1, 9.0 - (k / 200) as f64 * 0.2);
            mv(MoveKind::Feed, p(-1.0, y, z), p(21.0, y, z))
        })
        .collect();
    fn op(m: &[Move]) -> [SimOp<'_>; 1] {
        [SimOp { moves: m, radius: 0.5, ball: false, cone: None, corner: None }]
    }
    let block = || Stock::block(Backend::Column, 0.0, 0.0, 20.0, 20.0, 10.0, 0.1).expect("the test block");
    let mut one = block();
    one.simulate(&op(&moves)).expect("one call");
    let mut each = block();
    for m in &moves {
        each.simulate(&op(std::slice::from_ref(m))).expect("one move");
    }
    for i in 0..200 {
        for j in 0..200 {
            let (x, y) = (0.05 + i as f64 * 0.1, 0.05 + j as f64 * 0.1);
            assert_eq!(one.top(x, y).to_bits(), each.top(x, y).to_bits(), "column ({x}, {y})");
        }
    }
}
