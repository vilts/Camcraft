//! #874: the machined body carried through a side-on quarter turn - the
//! level axis re-indexed, the old heights cut into slabs, the turned
//! faces never read short, and the named refusal of every turn off the
//! quarter turns.
//! Provenance: our own fixtures (blocks with a step, a pocket or an
//! emptied strip) over the rigid-body algebra; no vendor material
//! consulted.
//!
//! Gate map (#874): covers camcraft_sim::stock_sim (carry_side: SideTurn,
//! CarryTurn, Body::carry_side, CarriedStock::carry_through,
//! FIELD_SIDE_CARRY_REFUSAL; carry: CARRY_TURN_REFUSAL).
//! Lane shape (M4.4): all tests here run in milliseconds.

use camcraft_sim::geo2d::Point2;
use camcraft_sim::geometry::{Point3, Triangle, cube};
use camcraft_sim::stock_sim::{
    Body, CARRY_TURN_REFUSAL, CarriedStock, CarryTurn, DEV_TOL, Dexel, FIELD_BAND, FIELD_SIDE_CARRY_REFUSAL, FieldBody,
    Lattice, LatticeTurn, SideTurn,
};

/// A quarter turn about X by -90 degrees: `(x, y, z) -> (x, z, -y)`.
const X_M90: [[f64; 3]; 3] = [[1.0, 0.0, 0.0], [0.0, 0.0, 1.0], [0.0, -1.0, 0.0]];
/// About X by +90 degrees: `(x, y, z) -> (x, -z, y)`.
const X_P90: [[f64; 3]; 3] = [[1.0, 0.0, 0.0], [0.0, 0.0, -1.0], [0.0, 1.0, 0.0]];
/// About Y by +90 degrees: `(x, y, z) -> (z, y, -x)`.
const Y_P90: [[f64; 3]; 3] = [[0.0, 0.0, 1.0], [0.0, 1.0, 0.0], [-1.0, 0.0, 0.0]];
/// A half turn about Z, then about X by -90: `(x, y, z) -> (-x, z, y)`.
const Z180_X_M90: [[f64; 3]; 3] = [[-1.0, 0.0, 0.0], [0.0, 0.0, 1.0], [0.0, 1.0, 0.0]];
const FLIP_X: [[f64; 3]; 3] = [[1.0, 0.0, 0.0], [0.0, -1.0, 0.0], [0.0, 0.0, -1.0]];

fn side(r: [[f64; 3]; 3], t: [f64; 3]) -> SideTurn {
    SideTurn::from_matrix(r, t).expect("a side-on turn")
}

/// A 60 x 40 grid of 1 mm cells with every column's top at 18.
fn grid() -> Dexel {
    Dexel::block(-30.0, -20.0, 30.0, 20.0, 18.0, 1.0).expect("block")
}

/// The grid with the columns `pick` chooses (by centre) lowered to `z`.
fn lowered(pick: impl Fn(f64, f64) -> bool, z: f64) -> Dexel {
    let mut d = grid();
    for iy in 0..d.ny {
        for ix in 0..d.nx {
            let c = d.center(ix, iy);
            if pick(c.x, c.y) {
                d.lower(ix + iy * d.nx, z);
            }
        }
    }
    d
}

/// The block Z -2..18 with a step: every column past Y 10 cut to Z 10.
fn stepped() -> Body {
    Body::on_floor(lowered(|_, y| y > 10.0, 10.0), -2.0)
}

fn column(b: &Body, x: f64, y: f64) -> (f64, f64) {
    let k = b.dex.col(Point2::new(x, y)).expect("in the grid");
    (b.dex.top_at(k), b.floor_at(k))
}

#[test]
fn an_uncut_block_stands_on_its_edge_exactly() {
    let b = Body::on_floor(grid(), -2.0);
    let c = b.carry_side(&side(X_M90, [0.0; 3])).expect("turned");
    // X stays, the old Z -2..18 is the new Y, the old Y is the height.
    assert_eq!((c.dex.nx, c.dex.ny), (60, 20));
    assert_eq!((c.dex.x0, c.dex.y0), (-30.0, -2.0));
    assert_eq!(column(&c, 0.5, 0.5), (20.0, -20.0), "the block's own faces: lattice edges, read exactly");
    assert!((c.volume() - b.volume()).abs() < 1e-9, "{} vs {}", c.volume(), b.volume());
    assert!(!c.floor_judged());
}

#[test]
fn a_step_turned_side_on_is_never_read_short() {
    let c = stepped().carry_side(&side(X_M90, [0.0; 3])).expect("turned");
    // Height is -Y. Above old Z 10 only the columns up to Y 9.5 stand: the
    // real face lies between the centres 9.5 and 10.5, so the floor is
    // read at -10.5 - half a cell outside the cell edge, never inside.
    assert_eq!(column(&c, 0.5, 10.5), (20.0, -10.5));
    assert_eq!(column(&c, 0.5, 17.5), (20.0, -10.5));
    // At and under old Z 10 every column stands.
    assert_eq!(column(&c, 0.5, 9.5), (20.0, -20.0));
    assert_eq!(column(&c, 0.5, -1.5), (20.0, -20.0));
    let real = 60.0 * 40.0 * 20.0 - 60.0 * 10.0 * 8.0;
    assert!(c.volume() >= real && c.volume() <= real + 60.0 * 8.0 * 1.0, "{} vs {real}", c.volume());
}

#[test]
fn a_step_on_the_low_side_is_never_read_short_either() {
    // Every column below Y -10 cut to Z 10: above it the first standing
    // centre is Y -9.5, so the face is read at the centre before it, -10.5.
    let b = Body::on_floor(lowered(|_, y| y < -10.0, 10.0), -2.0);
    let c = b.carry_side(&side(X_M90, [0.0; 3])).expect("turned");
    assert_eq!(column(&c, 0.5, 10.5), (10.5, -20.0), "the height is -Y");
    assert_eq!(column(&c, 0.5, 9.5), (20.0, -20.0));
}

#[test]
fn the_slabs_run_backwards_when_the_old_z_is_negated() {
    let c = stepped().carry_side(&side(X_P90, [0.0; 3])).expect("turned");
    // New Y is -Z (-18..2), the height is +Y.
    assert_eq!((c.dex.x0, c.dex.y0, c.dex.ny), (-30.0, -18.0, 20));
    assert_eq!(column(&c, 0.5, -10.5), (10.5, -20.0), "above old Z 10 the step shows from the top");
    assert_eq!(column(&c, 0.5, -9.5), (20.0, -20.0));
}

#[test]
fn a_pocket_becomes_a_filled_side_cavity() {
    // One column cut to Z 10: seen side-on the pocket is a cavity between
    // standing columns - the hull fills it, more material, never less.
    let b = Body::on_floor(lowered(|x, y| (x - 10.5).abs() < 0.1 && (y - 5.5).abs() < 0.1, 10.0), -2.0);
    let c = b.carry_side(&side(X_M90, [0.0; 3])).expect("turned");
    assert_eq!(column(&c, 10.5, 14.5), (20.0, -20.0));
    assert!(c.volume() >= b.volume());
}

#[test]
fn a_turn_about_y_reindexes_y_and_moves_with_the_translation() {
    // The strip Y 5..6, X past 20, cut through (below the floor).
    let b = Body::on_floor(lowered(|x, y| x > 20.0 && (y - 5.5).abs() < 0.1, -5.0), -2.0);
    let turn = side(Y_P90, [100.0, 200.0, 300.0]);
    assert_eq!(turn.apply(Point3::new(1.0, 2.0, 3.0)), Point3::new(103.0, 202.0, 299.0));
    let c = b.carry_side(&turn).expect("turned");
    // New X is Z + 100 (98..118), new Y is Y + 200, the height is 300 - X.
    assert_eq!((c.dex.nx, c.dex.ny, c.dex.x0, c.dex.y0), (20, 40, 98.0, 180.0));
    assert_eq!(column(&c, 100.5, 205.5), (330.0, 279.5), "the strip's row stands only up to X 20.5");
    assert_eq!(column(&c, 100.5, 204.5), (330.0, 270.0));
    assert_eq!(column(&c, 100.5, 194.5), (330.0, 270.0), "the mirror row is whole: Y was not reversed");
}

#[test]
fn a_negated_level_axis_is_reindexed_backwards() {
    // The row X 10..11 cut to Z 10 past Y 10; the turn negates X.
    let b = Body::on_floor(lowered(|x, y| (x - 10.5).abs() < 0.1 && y > 10.0, 10.0), -2.0);
    let c = b.carry_side(&side(Z180_X_M90, [0.0; 3])).expect("turned");
    assert_eq!(column(&c, -10.5, 14.5), (10.5, -20.0), "the row lands at -X, its top half a cell out");
    assert_eq!(column(&c, 10.5, 14.5), (20.0, -20.0));
}

#[test]
fn a_face_inside_a_slab_counts_the_slab_whole() {
    // A plate faced to 9.3 on 1 mm cells: ten slabs centred on 0..9.3.
    let d = Dexel::block(-30.0, -20.0, 30.0, 20.0, 9.3, 1.0).expect("plate");
    let b = Body::on_floor(d, 0.0);
    let c = b.carry_side(&side(X_M90, [0.0; 3])).expect("turned");
    assert_eq!(c.dex.ny, 10);
    assert!((c.dex.y0 + 0.35).abs() < 1e-12, "centred: 0.35 over at each end, got {}", c.dex.y0);
    let real = 60.0 * 40.0 * 9.3;
    assert!(c.volume() >= real && c.volume() <= 60.0 * 40.0 * 10.3, "never smaller, under one cell over: {}", c.volume());
    // Both end slabs have their centre inside the real plate.
    assert!(c.dex.y0 + 0.5 > 0.0 && c.dex.y0 + 9.5 < 9.3);
    // A plate thinner than a cell is still there.
    let thin = Body::on_floor(Dexel::block(-30.0, -20.0, 30.0, 20.0, 0.3, 1.0).expect("foil"), 0.0);
    let c = thin.carry_side(&side(X_M90, [0.0; 3])).expect("turned");
    assert_eq!(c.dex.ny, 1);
    assert!(c.volume() >= 60.0 * 40.0 * 0.3);
}

#[test]
fn turned_there_and_back_is_never_smaller() {
    let b = stepped();
    let there = b.carry_side(&side(X_M90, [0.0; 3])).expect("there");
    let back = there.carry_side(&side(X_P90, [0.0; 3])).expect("back");
    assert_eq!((back.dex.nx, back.dex.ny, back.dex.x0, back.dex.y0), (60, 40, -30.0, -20.0));
    for k in 0..b.dex.nx * b.dex.ny {
        assert!(back.dex.top_at(k) >= b.dex.top_at(k) && back.floor_at(k) <= b.floor_at(k), "column {k}");
    }
}

#[test]
fn a_clean_block_turned_side_on_reads_no_gouge_and_no_judged_floor() {
    // A part Z 0..16 in the block faced to 16, flipped first so the
    // incoming floor IS judged; the side-on turn drops that flag.
    let part = cube(Point3::new(-25.0, -15.0, 0.0), Point3::new(25.0, 15.0, 16.0)).facets.to_vec();
    let b = Body::on_floor(lowered(|_, _| true, 16.0), -2.0);
    let flip = LatticeTurn::from_matrix(FLIP_X, [0.0, 0.0, 16.0]).expect("flip");
    let flipped = b.carry(&flip).expect("flipped");
    assert!(flipped.floor_judged());
    let turn = side(X_M90, [0.0; 3]);
    let c = flipped.carry_side(&turn).expect("turned");
    assert!(!c.floor_judged(), "the new floor is a face no tool faced");
    let at = |p: Point3| turn.apply(flip.apply(p));
    let seen: Vec<Triangle> = part.iter().map(|t| Triangle::new(at(t.a), at(t.b), at(t.c))).collect();
    let v = c.deviate(&seen, DEV_TOL).expect("verdict");
    assert_eq!(v.gouge_cells, 0, "worst {}", v.max_gouge);
}

#[test]
fn turns_off_the_quarter_are_refused_by_name() {
    for deg in [10f64, 30.0] {
        let (c, s) = (deg.to_radians().cos(), deg.to_radians().sin());
        for r in [[[c, -s, 0.0], [s, c, 0.0], [0.0, 0.0, 1.0]], [[1.0, 0.0, 0.0], [0.0, c, -s], [0.0, s, c]]] {
            assert_eq!(CarryTurn::from_matrix(r, [0.0; 3]).expect_err("off the quarter"), CARRY_TURN_REFUSAL);
            assert_eq!(SideTurn::from_matrix(r, [0.0; 3]).expect_err("off the quarter"), CARRY_TURN_REFUSAL);
        }
    }
    // Entries of -1, 0, 1 that are no permutation of the axes.
    let doubled = [[1.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 0.0, 1.0]];
    assert_eq!(SideTurn::from_matrix(doubled, [0.0; 3]).expect_err("no permutation"), CARRY_TURN_REFUSAL);
    assert_eq!(CarryTurn::from_matrix(doubled, [0.0; 3]).expect_err("no permutation"), CARRY_TURN_REFUSAL);
    let mirror = [[1.0, 0.0, 0.0], [0.0, 0.0, 1.0], [0.0, 1.0, 0.0]];
    let err = CarryTurn::from_matrix(mirror, [0.0; 3]).expect_err("a mirror");
    assert!(err.contains("mirrors"), "{err}");
    let err = SideTurn::from_matrix(X_M90, [0.0, f64::NAN, 0.0]).expect_err("non-finite");
    assert!(err.contains("not finite"), "{err}");
    let err = SideTurn::from_matrix(FLIP_X, [0.0; 3]).expect_err("a lattice turn");
    assert!(err.contains("not a side-on turn"), "{err}");
}

#[test]
fn the_two_kinds_of_turn_are_told_apart() {
    let flip = CarryTurn::from_matrix(FLIP_X, [0.0, 0.0, 16.0]).expect("flip");
    let lattice = LatticeTurn::from_matrix(FLIP_X, [0.0, 0.0, 16.0]).expect("flip");
    assert_eq!(flip, CarryTurn::Lattice(lattice));
    assert!(!flip.side_on());
    // A digest written from the turn's text does not move for a lattice turn.
    assert_eq!(format!("{flip:?}"), format!("{lattice:?}"));
    let turned = CarryTurn::from_matrix(X_M90, [1.0, 2.0, 3.0]).expect("side-on");
    assert!(turned.side_on());
    assert_eq!(format!("{turned:?}"), format!("{:?}", side(X_M90, [1.0, 2.0, 3.0])));
    let p = Point3::new(1.0, 2.0, 3.0);
    assert_eq!(turned.apply(p), Point3::new(2.0, 5.0, 1.0));
    assert_eq!(flip.apply(p), lattice.apply(p));
    // `carry_through` is `carry` on a lattice turn, `carry_side` on a side turn.
    let b = stepped();
    let through = CarriedStock::Column(b.clone()).carry_through(&flip).expect("flip").into_body().expect("body");
    assert_eq!(column(&through, 0.5, -15.5), column(&b.carry(&lattice).expect("flip"), 0.5, -15.5));
    assert!(through.floor_judged());
    let through = CarriedStock::Column(b).carry_through(&turned).expect("side").into_body().expect("body");
    assert_eq!(column(&through, 1.5, 12.5), (23.0, -7.5), "X + 1, Z + 2 level; 3 - Y the height");
}

#[test]
fn a_body_too_tall_for_the_turned_lattice_is_refused_by_name() {
    let d = Dexel::block(0.0, 0.0, 4.0, 4.0, 5000.0, 1.0).expect("a tall bar");
    let err = Body::on_floor(d, 0.0).carry_side(&side(X_M90, [0.0; 3])).expect_err("too tall");
    assert!(err.contains("5000 cells tall") && err.contains("4096"), "{err}");
}

#[test]
fn the_field_engine_refuses_a_side_on_turn_by_name() {
    let lat = Lattice { x0: 0.0, y0: 0.0, cell: 1.0, nx: 4, ny: 4 };
    let field = CarriedStock::Field(Box::new(FieldBody::block(lat, 0.0, 4.0, FIELD_BAND).expect("field block")));
    let turn = CarryTurn::from_matrix(X_M90, [0.0; 3]).expect("side-on");
    assert_eq!(field.clone().carry_through(&turn).expect_err("refused"), FIELD_SIDE_CARRY_REFUSAL);
    let flip = CarryTurn::from_matrix(FLIP_X, [0.0, 0.0, 4.0]).expect("flip");
    assert!(field.carry_through(&flip).is_ok(), "a lattice turn is still the field's own");
}
