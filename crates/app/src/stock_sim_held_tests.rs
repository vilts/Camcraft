//! Unit tests of `stock_sim_held` (#541 review, PR #1131): the sinker
//! electrode's box atom through every arm of `Held::simulate_box`, and
//! the outline atom (#1152) through `Held::simulate_outline`. The
//! program walk never reaches a carried column grid with a burn today
//! (a carry onto any non-mill is refused by name before it), so the
//! carried arm is pinned here, on the holder itself.
//! Provenance: our own synthetic burn (the #541 fixture's numbers).

use super::*;
use crate::geo2d::Point2;
use crate::operation::SinkerDatum;
use crate::sinker::{burn_to_depth, SinkerParams};
use crate::stock_sim::{LatticeTurn, ELECTRODE_FIELD_REFUSAL};

const GAP: f64 = 0.05;

/// A 10 x 6 electrode burned 6 mm into a 20 mm block at the origin.
fn burn_moves() -> Vec<crate::cl::Move> {
    let p = SinkerParams {
        x: 0.0,
        y: 0.0,
        approach_z: 25.0,
        top_z: 20.0,
        floor_z: 14.0,
        model_floor_z: Some(0.0),
        datum: SinkerDatum::WorkTop,
        depth: 6.0,
        electrode_len: 30.0,
        feed: 40.0,
        gap: GAP,
        stand: None,
        orbit: None,
    };
    burn_to_depth(&p).expect("burns").moves
}

fn grid() -> Dexel {
    Dexel::block(-30.0, -30.0, 30.0, 30.0, 20.0, 0.25).expect("grid")
}

fn tops(h: Held) -> Vec<u64> {
    let d = h.into_tops().expect("column tops");
    (0..d.nx * d.ny).map(|k| d.top_at(k).to_bits()).collect()
}

#[test]
fn a_burn_on_a_carried_column_grid_is_the_plain_grid_burn() {
    let moves = burn_moves();
    let mut plain = Held::Plain(Stock::Column(grid()));
    plain.simulate_box(&moves, 5.0, 3.0, GAP).expect("plain burn");

    // Carried: a body on the block's floor, carried through a turn that
    // keeps the lattice (the "Same as" link's identity carry).
    let body = CarriedStock::Column(Body::on_floor(grid(), 0.0)).carry(&LatticeTurn::IDENTITY).expect("identity carry");
    let mut carried = Held::Carried(body);
    carried.simulate_box(&moves, 5.0, 3.0, GAP).expect("carried burn");

    let d = Held::Plain(Stock::Column(grid()));
    let untouched = tops(d);
    let (p, c) = (tops(plain), tops(carried));
    assert_ne!(p, untouched, "the burn cut something");
    assert_eq!(c, p, "the carried grid takes the plain grid's burn, column for column");

    // And the cavity is where the box atom says: the floor inside the
    // face grown by the gap, untouched past it.
    let mut again = Held::Carried(CarriedStock::Column(Body::on_floor(grid(), 0.0)));
    again.simulate_box(&moves, 5.0, 3.0, GAP).expect("burn");
    let d = again.into_tops().expect("tops");
    let top = |x: f64, y: f64| d.top_at(d.col(Point2::new(x, y)).expect("in the grid"));
    assert!((top(0.0, 0.0) - 14.0).abs() < 1e-9 && (top(4.9, 2.9) - 14.0).abs() < 1e-9, "the cavity floor");
    assert_eq!(top(5.3, 0.0), 20.0);
}

#[test]
fn the_field_arms_refuse_a_burn_by_name() {
    let moves = burn_moves();
    let d = grid();
    let field = FieldBody::block(lattice_of_grid(&d), 0.0, 20.0, FIELD_BAND).expect("field block");
    let mut carried = Held::Carried(CarriedStock::Field(Box::new(field)));
    let err = carried.simulate_box(&moves, 5.0, 3.0, GAP).expect_err("the field has no box atom");
    assert_eq!(err, ELECTRODE_FIELD_REFUSAL);

    let plain = Stock::block(Backend::Field { band: FIELD_BAND }, -30.0, -30.0, 30.0, 30.0, 20.0, 0.25).expect("field");
    let err = Held::Plain(plain).simulate_box(&moves, 5.0, 3.0, GAP).expect_err("never a silent no-op");
    assert_eq!(err, ELECTRODE_FIELD_REFUSAL);
}

/// #1152: the outline atom takes the same arms - a column grid plain or
/// carried burns it, and a field refuses it by name.
#[test]
fn the_outline_atom_burns_a_column_grid_and_a_field_refuses_it_by_name() {
    let moves = burn_moves();
    // An L about the burn point: the notch (upper right) is no electrode.
    let ell = [[-5.0, -3.0], [5.0, -3.0], [5.0, 0.0], [0.0, 0.0], [0.0, 3.0], [-5.0, 3.0]];
    let mut plain = Held::Plain(Stock::Column(grid()));
    plain.simulate_outline(&moves, &ell, GAP).expect("plain burn");
    let body = CarriedStock::Column(Body::on_floor(grid(), 0.0)).carry(&LatticeTurn::IDENTITY).expect("identity carry");
    let mut carried = Held::Carried(body);
    carried.simulate_outline(&moves, &ell, GAP).expect("carried burn");
    let d = plain.into_tops().expect("tops");
    let top = |x: f64, y: f64| d.top_at(d.col(Point2::new(x, y)).expect("in the grid"));
    assert!((top(-2.0, 1.0) - 14.0).abs() < 1e-9 && (top(4.9, -2.9) - 14.0).abs() < 1e-9, "the L is burned to the floor");
    assert_eq!(top(2.6, 1.6), 20.0, "the notch stands: the outline is burned, not its bounds");
    let c = carried.into_tops().expect("tops");
    assert!((0..d.nx * d.ny).all(|k| d.top_at(k).to_bits() == c.top_at(k).to_bits()), "the carried grid takes the same burn");

    let field = FieldBody::block(lattice_of_grid(&grid()), 0.0, 20.0, FIELD_BAND).expect("field block");
    let mut carried = Held::Carried(CarriedStock::Field(Box::new(field)));
    assert_eq!(carried.simulate_outline(&moves, &ell, GAP).expect_err("the field has no outline atom"), ELECTRODE_FIELD_REFUSAL);
    let plain = Stock::block(Backend::Field { band: FIELD_BAND }, -30.0, -30.0, 30.0, 30.0, 20.0, 0.25).expect("field");
    assert_eq!(Held::Plain(plain).simulate_outline(&moves, &ell, GAP).expect_err("never a silent no-op"), ELECTRODE_FIELD_REFUSAL);
    // A section that is no outline is the sim's refusal, in its words.
    let err = Held::Plain(Stock::Column(grid())).simulate_outline(&moves, &ell[..2], GAP).expect_err("two points");
    assert!(err.contains("at least 3 points"), "{err}");
}
