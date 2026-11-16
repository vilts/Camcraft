//! #637 step E3: `CarriedStock` holds the same body on both engines. A
//! block on a lattice whose corner is off the cell pitch, cut, carried
//! through every lattice turn, cut again and clipped: the column arm
//! (`Body`) and the field arm (`FieldBody`, read back) agree column for
//! column - and a column cut through is convicted on both, never read as
//! air.
//!
//! Gate map (#637): covers sim::stock_sim::carried (CarriedStock,
//! FieldBody, clip_columns_under, lattice_of_grid), carried_read
//! (FieldBody::body).
//! Provenance: our own fixtures.

use camcraft_sim::cl::{Move, MoveKind};
use camcraft_sim::geometry::{cube, Point3, Triangle};
use camcraft_sim::stock_sim::{
    lattice_of_grid, Body, CarriedStock, Dexel, FieldBody, Lattice, LatticeTurn, SimOp, FIELD_BAND,
};

const CELL: f64 = 0.5;
/// Both engines read a top at a cell centre: equal to rounding on the
/// stock's planes and a flat tool's floor.
const EPS: f64 = 1e-6;
/// The block: corner off the pitch, floor and top off the Z lattice.
const X0: f64 = 0.3;
const Y0: f64 = -1.7;
const FLOOR: f64 = -2.2;
const TOP: f64 = 5.1;

fn p(x: f64, y: f64, z: f64) -> Point3 {
    Point3::new(x, y, z)
}

/// The same 20 x 12 block on both engines.
fn pair() -> (CarriedStock, CarriedStock) {
    let dex = Dexel::block(X0, Y0, X0 + 20.0, Y0 + 12.0, TOP, CELL).expect("block");
    let field = FieldBody::block(lattice_of_grid(&dex), FLOOR, TOP, FIELD_BAND).expect("field block");
    (CarriedStock::Column(Body::on_floor(dex, FLOOR)), CarriedStock::Field(Box::new(field)))
}

/// A flat slot along X at `y`, tool bottom at `z`, radius 2, reaching
/// past both ends of anything a turn makes of the block.
fn slot(y: f64, z: f64) -> Vec<Move> {
    vec![
        Move::new(MoveKind::Rapid, p(-40.0, y, 30.0), p(-40.0, y, z), None),
        Move::new(MoveKind::Feed, p(-40.0, y, z), p(60.0, y, z), Some(500.0)),
    ]
}

fn cut(s: &mut CarriedStock, moves: &[Move]) {
    s.simulate(&[SimOp { moves, radius: 2.0, ball: false, cone: None, corner: None }]).expect("cut");
}

fn turn(r: [[f64; 3]; 3], t: [f64; 3]) -> LatticeTurn {
    LatticeTurn::from_matrix(r, t).expect("a lattice turn")
}

/// The eight lattice turns, each with a translation off the pitch.
fn turns() -> Vec<LatticeTurn> {
    let z = |s: f64| [0.0, 0.0, s];
    vec![
        turn([[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], z(1.0)], [3.13, -2.4, 1.07]),
        turn([[0.0, -1.0, 0.0], [1.0, 0.0, 0.0], z(1.0)], [10.2, 0.1, 0.0]),
        turn([[-1.0, 0.0, 0.0], [0.0, -1.0, 0.0], z(1.0)], [20.6, 8.6, -0.4]),
        turn([[0.0, 1.0, 0.0], [-1.0, 0.0, 0.0], z(1.0)], [0.0, 20.6, 0.0]),
        turn([[1.0, 0.0, 0.0], [0.0, -1.0, 0.0], z(-1.0)], [0.0, 8.6, 2.9]),
        turn([[-1.0, 0.0, 0.0], [0.0, 1.0, 0.0], z(-1.0)], [20.6, 0.0, 2.9]),
        turn([[0.0, 1.0, 0.0], [1.0, 0.0, 0.0], z(-1.0)], [0.37, 0.0, 5.0]),
        turn([[0.0, -1.0, 0.0], [-1.0, 0.0, 0.0], z(-1.0)], [12.0, 20.0, 5.0]),
    ]
}

/// Column for column: the lattice, which columns are empty, and the top
/// and floor of every column that holds material.
fn agree(what: &str, col: &CarriedStock, field: &CarriedStock) -> (Body, Body) {
    let (a, b) = (col.clone().into_body().expect("column body"), field.clone().into_body().expect("field body"));
    let (d, e) = (&a.dex, &b.dex);
    assert_eq!((d.nx, d.ny), (e.nx, e.ny), "{what}: lattice");
    assert!((d.x0 - e.x0).abs() <= 1e-9 && (d.y0 - e.y0).abs() <= 1e-9 && d.cell == e.cell, "{what}: lattice origin");
    assert_eq!(a.floor_judged(), b.floor_judged(), "{what}: floor_judged");
    for k in 0..d.nx * d.ny {
        assert_eq!(a.empty_at(k), b.empty_at(k), "{what}: column {k} empty");
        if a.empty_at(k) {
            continue;
        }
        let (ta, tb, fa, fb) = (d.top_at(k), e.top_at(k), a.floor_at(k), b.floor_at(k));
        assert!((ta - tb).abs() <= EPS, "{what}: column {k} top {ta} vs {tb}");
        assert!((fa - fb).abs() <= EPS, "{what}: column {k} floor {fa} vs {fb}");
    }
    let cells = (d.nx * d.ny) as f64 * d.cell * d.cell;
    assert!((a.volume() - b.volume()).abs() <= EPS * cells, "{what}: volume {} vs {}", a.volume(), b.volume());
    (a, b)
}

#[test]
fn a_block_off_the_pitch_reads_the_same_on_both_engines_cut_and_uncut() {
    let (mut col, mut field) = pair();
    let (a, _) = agree("uncut", &col, &field);
    assert!((a.volume() - 20.0 * 12.0 * (TOP - FLOOR)).abs() < 1e-6, "{}", a.volume());
    for s in [&mut col, &mut field] {
        cut(s, &slot(4.0, 2.0));
    }
    agree("one slot", &col, &field);
}

#[test]
fn a_cut_carried_and_cut_again_agrees_through_every_turn() {
    for (i, t) in turns().into_iter().enumerate() {
        let (mut col, mut field) = pair();
        for s in [&mut col, &mut field] {
            cut(s, &slot(4.0, 2.0));
        }
        let (mut col, mut field) = (col.carry(&t).expect("column carry"), field.carry(&t).expect("field carry"));
        let (a, _) = agree(&format!("turn {i} carried"), &col, &field);
        assert_eq!(a.floor_judged(), t.flips(), "turn {i}");
        // Cut the carried stock in the NEW frame: a slot through the
        // middle of what stands, a third of the way into it.
        let (lo, hi) = a.bounds().expect("material");
        let mid = slot((lo.y + hi.y) * 0.5, hi.z - (hi.z - lo.z) / 3.0);
        for s in [&mut col, &mut field] {
            cut(s, &mid);
        }
        agree(&format!("turn {i} cut after the carry"), &col, &field);
    }
}

#[test]
fn two_carries_with_a_cut_between_agree() {
    let ts = turns();
    let (mut col, mut field) = pair();
    for s in [&mut col, &mut field] {
        cut(s, &slot(4.0, 2.0));
    }
    // A flip, a cut on the former bottom, then a quarter turn.
    let (mut col, mut field) = (col.carry(&ts[4]).expect("carry"), field.carry(&ts[4]).expect("carry"));
    let (a, _) = agree("flipped", &col, &field);
    let (lo, hi) = a.bounds().expect("material");
    let mid = slot((lo.y + hi.y) * 0.5 + 1.0, hi.z - 1.5);
    for s in [&mut col, &mut field] {
        cut(s, &mid);
    }
    let (col, field) = (col.carry(&ts[1]).expect("carry"), field.carry(&ts[1]).expect("carry"));
    let (a, b) = agree("flipped, cut, quarter-turned", &col, &field);
    assert!(a.floor_judged() && b.floor_judged(), "a flip stays judged through an upright turn");
}

/// The entered setup's own stock grid: a block over part of the body
/// whose top stands below the body's, or a round.
fn clip_grids() -> Vec<(&'static str, Dexel)> {
    let block = Dexel::block(X0 + 4.0, Y0 + 2.0, X0 + 14.0, Y0 + 9.0, 3.0, CELL).expect("clip block");
    let shape = camcraft_sim::stock::StockShape::Cylinder { diameter: 9.0, height: 4.0 };
    let mut round = Dexel::from_shape(&shape, 0.4).expect("clip round");
    round.x0 += 10.0;
    round.y0 += 4.0;
    vec![("block", block), ("round", round)]
}

#[test]
fn the_clip_lowers_only_under_the_new_stock_on_both_engines() {
    for (name, grid) in clip_grids() {
        let (mut col, mut field) = pair();
        for s in [&mut col, &mut field] {
            cut(s, &slot(4.0, 2.0));
            s.clip_under(&grid);
        }
        let (a, _) = agree(&format!("{name} clip"), &col, &field);
        // Outside the new stock what stood still stands (the column law:
        // never less material than the engine before reported).
        let k = a.dex.col(camcraft_sim::geo2d::Point2::new(X0 + 0.25, Y0 + 0.25)).expect("corner column");
        assert!((a.dex.top_at(k) - TOP).abs() < 1e-9, "{name}: the corner outside the clip keeps its top");
        // A later flip carries the ceiling as a floor bound; a cut after
        // it commutes with the clip.
        let t = turns()[5];
        let (mut col, mut field) = (col.carry(&t).expect("carry"), field.carry(&t).expect("carry"));
        let (a, _) = agree(&format!("{name} clip, flipped"), &col, &field);
        let (lo, hi) = a.bounds().expect("material");
        let mid = slot((lo.y + hi.y) * 0.5, hi.z - 1.0);
        for s in [&mut col, &mut field] {
            cut(s, &mid);
        }
        agree(&format!("{name} clip, flipped, cut"), &col, &field);
    }
}

/// A part filling the block's footprint from the floor up to Z 4.
fn part() -> Vec<Triangle> {
    cube(p(X0, Y0, FLOOR), p(X0 + 20.0, Y0 + 12.0, 4.0)).facets.to_vec()
}

#[test]
fn a_column_cut_through_is_convicted_on_both_engines_never_read_as_air() {
    let (mut col, mut field) = pair();
    // The slot's tool bottom is below the stock's floor: cut through.
    for s in [&mut col, &mut field] {
        cut(s, &slot(4.0, FLOOR - 1.0));
    }
    let (a, b) = agree("cut through", &col, &field);
    let k = a.dex.col(camcraft_sim::geo2d::Point2::new(X0 + 10.25, 4.0)).expect("slot column");
    assert!(a.empty_at(k) && b.empty_at(k));
    // The field reads the stock's own extent there, both ends finite.
    assert!((b.dex.top_at(k) - FLOOR).abs() < 1e-9 && (b.floor_at(k) - TOP).abs() < 1e-9, "{} {}", b.dex.top_at(k), b.floor_at(k));
    for (name, s, deeper) in [("column", &col, 1.0), ("field", &field, 0.0)] {
        let dev = s.deviate(&part(), 0.01).expect("verdict");
        // The part stood 4 - FLOOR tall in the slot: gone, a gouge.
        assert!(dev.dev[k] < -0.01, "{name}: the cut-through column is a gouge, read {}", dev.dev[k]);
        assert!(dev.gouge_cells > 0 && !dev.is_clean(), "{name}");
        assert!((dev.max_gouge - (4.0 - FLOOR + deeper)).abs() < 1e-6, "{name}: max gouge {}", dev.max_gouge);
    }
    // Flipped, the damage is still there on both.
    let t = turns()[4];
    let flipped = t.apply(p(0.0, 0.0, 0.0)).z;
    let model: Vec<Triangle> = cube(p(X0 - 40.0, -40.0, flipped - 4.0), p(X0 + 40.0, 40.0, flipped - FLOOR)).facets.to_vec();
    for (name, s) in [("column", col), ("field", field)] {
        let s = s.carry(&t).expect("carry");
        let dev = s.deviate(&model, 0.01).expect("verdict");
        assert!(dev.gouge_cells > 0 && !dev.is_clean(), "{name}: a cut-through column stays convicted after a flip");
    }
}

#[test]
fn a_degenerate_field_block_is_refused_by_name() {
    let lat = Lattice { x0: 0.0, y0: 0.0, cell: CELL, nx: 8, ny: 8 };
    assert!(FieldBody::block(lat, 1.0, 1.0, FIELD_BAND).is_err(), "no thickness");
    assert!(FieldBody::block(lat, f64::NAN, 1.0, FIELD_BAND).is_err(), "NaN floor");
    assert!(FieldBody::block(Lattice { x0: f64::NAN, ..lat }, 0.0, 1.0, FIELD_BAND).is_err(), "NaN corner");
    assert!(FieldBody::block(Lattice { nx: 0, ..lat }, 0.0, 1.0, FIELD_BAND).is_err(), "empty lattice");
    assert!(FieldBody::block(Lattice { cell: 0.0, ..lat }, 0.0, 1.0, FIELD_BAND).is_err(), "zero cell");
    assert!(FieldBody::mesh(lat, &[], FIELD_BAND).is_err(), "no mesh");
}
