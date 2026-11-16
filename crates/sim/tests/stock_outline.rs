//! #1152 (part of #290): the outline atom - a sinker electrode BODY (a
//! closed outline extruded along Z), grown by the spark gap and swept
//! along straight chords through the column grid. It burns the section
//! itself: not its bounding box (which would burn a gouge the electrode
//! never made), and never less than the section (which would hide one).
//! An oracle that samples the sweep and measures the distance by other
//! means bounds it from both sides, column by column.
//!
//! Gate map (#1152): covers camcraft_sim::stock_sim::dexel_outline
//! (simulate_outline, Dexel::cut_outline) against
//! camcraft_sim::stock_sim::dexel_box (simulate_box).
//! Provenance: our own SYNTHETIC fixtures, built in code. Not exercised
//! on a real EDM control.

use camcraft_sim::cl::{ArcMove, Move, MoveKind};
use camcraft_sim::geo2d::Point2;
use camcraft_sim::geometry::Point3;
use camcraft_sim::stock_sim::{simulate_box, simulate_outline, Dexel, OUTLINE_ARC_REFUSAL};

const TOP: f64 = 10.0;
const CELL: f64 = 0.25;

fn grid() -> Dexel {
    Dexel::block(-20.0, -20.0, 20.0, 20.0, TOP, CELL).expect("a block")
}

fn p(x: f64, y: f64, z: f64) -> Point3 {
    Point3::new(x, y, z)
}

fn mv(kind: MoveKind, from: Point3, to: Point3) -> Move {
    Move { rotary: None, kind, from, to, feed: Some(40.0), arc: None, axis: None }
}

/// An L about the burn point: 10 x 8 with the upper right 6 x 4 gone.
fn ell() -> Vec<[f64; 2]> {
    vec![[-5.0, -4.0], [5.0, -4.0], [5.0, 0.0], [-1.0, 0.0], [-1.0, 4.0], [-5.0, 4.0]]
}

fn top(d: &Dexel, x: f64, y: f64) -> f64 {
    d.top_at(d.col(Point2::new(x, y)).expect("a column"))
}

/// The distance from `q` to the outline's edges, by projection.
fn edge_dist(o: &[[f64; 2]], q: [f64; 2]) -> f64 {
    let n = o.len();
    (0..n)
        .map(|i| {
            let (a, b) = (o[i], o[(i + 1) % n]);
            let (e, r) = ([b[0] - a[0], b[1] - a[1]], [q[0] - a[0], q[1] - a[1]]);
            let t = ((r[0] * e[0] + r[1] * e[1]) / (e[0] * e[0] + e[1] * e[1])).clamp(0.0, 1.0);
            (r[0] - t * e[0]).hypot(r[1] - t * e[1])
        })
        .fold(f64::INFINITY, f64::min)
}

/// Inside by the winding angle - a different rule from the atom's.
fn winds(o: &[[f64; 2]], q: [f64; 2]) -> bool {
    let n = o.len();
    let turn: f64 = (0..n)
        .map(|i| {
            let (a, b) = ([o[i][0] - q[0], o[i][1] - q[1]], [o[(i + 1) % n][0] - q[0], o[(i + 1) % n][1] - q[1]]);
            (a[0] * b[1] - a[1] * b[0]).atan2(a[0] * b[0] + a[1] * b[1])
        })
        .sum();
    turn.abs() > 3.0
}

/// Is `q` (electrode frame) covered by the section grown by `g`, with
/// `slack` added to (or, negative, taken from) the reach?
fn covered(o: &[[f64; 2]], q: [f64; 2], g: f64, slack: f64) -> bool {
    let d = edge_dist(o, q);
    if winds(o, q) {
        // Inside: covered, unless the doubt is to be left out and the
        // point is on the boundary itself.
        slack >= 0.0 || d >= -slack
    } else {
        d <= g + slack
    }
}

#[test]
fn a_plunge_burns_the_section_grown_by_the_gap_and_nothing_else() {
    let (gap, tip) = (0.3, 4.0);
    let mut d = grid();
    let moves = [mv(MoveKind::Rapid, p(2.0, 1.0, 20.0), p(2.0, 1.0, 11.0)), mv(MoveKind::Plunge, p(2.0, 1.0, 11.0), p(2.0, 1.0, tip))];
    simulate_outline(&mut d, &moves, &ell(), gap).expect("burned");
    // Every column, against the section by other means. Columns within
    // a nanometre of the grown boundary may read either way.
    let (mut burned, mut kept) = (0, 0);
    for iy in 0..d.ny {
        for ix in 0..d.nx {
            let c = d.center(ix, iy);
            let q = [c.x - 2.0, c.y - 1.0];
            let z = d.top_at(ix + iy * d.nx);
            if covered(&ell(), q, gap, -1e-9) {
                assert_eq!(z, tip - gap, "({}, {}) is under the electrode: burned to the face less the gap", c.x, c.y);
                burned += 1;
            } else if !covered(&ell(), q, gap, 1e-9) {
                assert_eq!(z, TOP, "({}, {}) is clear of the electrode: untouched", c.x, c.y);
                kept += 1;
            }
        }
    }
    // The L's area 56 plus a 0.3 band: ~ 67 mm² of 0.0625 mm² columns.
    assert!((1000..1150).contains(&burned) && kept > 20_000, "{burned} burned, {kept} kept");
    // The notch the bounding box would have burned stands whole.
    assert_eq!(top(&d, 2.0 + 2.125, 1.0 + 2.125), TOP, "the L's notch is not the electrode");
    // (5, -4) is an outer corner: 0.177 off it is inside the gap.
    assert_eq!(top(&d, 2.0 + 5.125, 1.0 - 4.125), tip - gap);
}

#[test]
fn a_rectangular_outline_burns_what_the_box_atom_burns() {
    // Gap 0: the two atoms state the same solid. With a gap they differ
    // only in the four corner squares (round against square).
    let rect = [[-5.0, -3.0], [5.0, -3.0], [5.0, 3.0], [-5.0, 3.0]];
    let moves = [
        mv(MoveKind::Plunge, p(-3.1, 0.07, 11.0), p(-3.1, 0.07, 6.0)),
        mv(MoveKind::Feed, p(-3.1, 0.07, 6.0), p(4.03, 2.2, 5.0)),
        mv(MoveKind::Feed, p(4.03, 2.2, 5.0), p(4.03, -6.1, 5.5)),
    ];
    let (mut a, mut b) = (grid(), grid());
    simulate_outline(&mut a, &moves, &rect, 0.0).expect("outline");
    simulate_box(&mut b, &moves, 5.0, 3.0, 0.0).expect("box");
    let mut cut = 0;
    for k in 0..a.cells() {
        let (za, zb) = (a.top_at(k), b.top_at(k));
        assert!((za - zb).abs() < 1e-12, "column {k}: outline {za}, box {zb}");
        cut += usize::from(za < TOP);
    }
    assert!(cut > 2000, "the sweep cut {cut} columns");
    // With a gap the outline atom never burns more than the box atom.
    let (mut a, mut b) = (grid(), grid());
    simulate_outline(&mut a, &moves, &rect, 0.4).expect("outline");
    simulate_box(&mut b, &moves, 5.0, 3.0, 0.4).expect("box");
    let mut rounder = 0;
    for k in 0..a.cells() {
        assert!(a.top_at(k) >= b.top_at(k) - 1e-12, "column {k}: a round corner burns no more than a square one");
        rounder += usize::from(a.top_at(k) > b.top_at(k) + 1e-9);
    }
    assert!(rounder > 0, "the gap's corners are round, not square");
}

#[test]
fn a_sloping_sweep_is_bounded_from_both_sides_by_a_sampled_oracle() {
    // A lateral move that also descends, then one that climbs: the
    // lowest face height over a column is where the section first or
    // last covers it. The oracle walks each chord in 1500 steps.
    let gap = 0.2;
    let chords = [(p(-6.0, -3.0, 8.0), p(5.5, 2.5, 5.0)), (p(5.5, 2.5, 5.0), p(-2.0, 6.0, 7.5))];
    let moves: Vec<Move> = chords.iter().map(|(a, b)| mv(MoveKind::Feed, *a, *b)).collect();
    let mut d = grid();
    simulate_outline(&mut d, &moves, &ell(), gap).expect("burned");
    const STEPS: usize = 1500;
    let (mut judged, mut lowered) = (0, 0);
    for iy in (0..d.ny).step_by(5) {
        for ix in (0..d.nx).step_by(5) {
            let c = d.center(ix, iy);
            // `sure`: the lowest height at a sample certainly covered.
            // `maybe`: the lowest at a sample possibly covered, less
            // one step's descent.
            let (mut sure, mut maybe) = (TOP, TOP);
            for (a, b) in &chords {
                let step_z = (b.z - a.z).abs() / STEPS as f64;
                let step_xy = (b.x - a.x).hypot(b.y - a.y) / STEPS as f64;
                for s in 0..=STEPS {
                    let t = s as f64 / STEPS as f64;
                    let at = [a.x + (b.x - a.x) * t, a.y + (b.y - a.y) * t];
                    let z = a.z + (b.z - a.z) * t - gap;
                    let q = [c.x - at[0], c.y - at[1]];
                    if covered(&ell(), q, gap, -1e-9) {
                        sure = sure.min(z);
                    }
                    if covered(&ell(), q, gap, step_xy) {
                        maybe = maybe.min(z - step_z);
                    }
                }
            }
            let z = d.top_at(ix + iy * d.nx);
            assert!(z <= sure + 1e-9, "({}, {}): the atom left {z}, but the electrode reached {sure} there - under-burned", c.x, c.y);
            assert!(z >= maybe - 1e-9, "({}, {}): the atom burned to {z}, below anything the electrode reached ({maybe}) - over-burned", c.x, c.y);
            judged += 1;
            lowered += usize::from(z < TOP);
        }
    }
    assert!(judged == 1024 && lowered > 100, "{judged} judged, {lowered} lowered");
}

#[test]
fn every_refusal_is_by_name_and_leaves_the_grid_untouched() {
    let burn = [mv(MoveKind::Plunge, p(0.0, 0.0, 11.0), p(0.0, 0.0, 4.0))];
    let refused = |moves: &[Move], outline: &[[f64; 2]], gap: f64| {
        let mut d = grid();
        let why = simulate_outline(&mut d, moves, outline, gap).expect_err("refused").to_string();
        assert!(d == grid(), "a refusal lowers nothing: {why}");
        why
    };
    // The section itself: the record's own law, said at the sim's door.
    assert!(refused(&burn, &[[0.0, 0.0], [1.0, 0.0]], 0.1).contains("at least 3 points"));
    let tie = [[0.0, 0.0], [4.0, 0.0], [1.0, 3.0], [4.0, 3.0]];
    assert!(refused(&burn, &tie, 0.1).contains("crosses itself"));
    // The gap.
    for gap in [-0.1, f64::NAN, f64::INFINITY] {
        assert!(refused(&burn, &ell(), gap).contains("spark gap"), "{gap}");
    }
    // The moves: non-finite, posed, an arc.
    let nan = [mv(MoveKind::Plunge, p(0.0, 0.0, 11.0), p(0.0, f64::NAN, 4.0))];
    assert!(refused(&nan, &ell(), 0.1).contains("non-finite"));
    let posed = [Move { axis: Some([0.0, 0.6, 0.8]), ..burn[0] }];
    assert!(refused(&posed, &ell(), 0.1).contains("electrode move 0"));
    let arc = [burn[0], Move { arc: Some(ArcMove::xy(Point2::new(1.0, 0.0), true)), ..mv(MoveKind::Feed, p(0.0, 0.0, 4.0), p(2.0, 0.0, 4.0)) }];
    let why = refused(&arc, &ell(), 0.1);
    assert!(why.contains("electrode move 1") && why.contains(OUTLINE_ARC_REFUSAL), "{why}");
    // A rapid touches nothing, arc or not; and the sound burn burns.
    let mut d = grid();
    let rapid = [Move { arc: Some(ArcMove::xy(Point2::new(1.0, 0.0), true)), ..mv(MoveKind::Rapid, p(0.0, 0.0, 4.0), p(2.0, 0.0, 4.0)) }];
    simulate_outline(&mut d, &rapid, &ell(), 0.1).expect("a rapid");
    assert!(d == grid(), "a rapid lowers nothing");
    simulate_outline(&mut d, &burn, &ell(), 0.1).expect("a burn");
    assert_eq!(top(&d, -3.125, -2.125), 4.0 - 0.1);
}

/// Garbage in: any outline, gap and moves either burn or refuse by
/// name - never a panic or a hang - and a burn only ever lowers.
#[test]
fn garbage_outlines_and_moves_refuse_or_burn_and_never_raise_a_column() {
    // A small xorshift: the same garbage on every run.
    let mut state = 0x1152_0BAD_5EED_u64;
    let mut next = move || {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        state
    };
    let num = |r: u64| match r % 11 {
        0 => f64::NAN,
        1 => f64::INFINITY,
        2 => -1e300,
        3 => 0.0,
        4 => 1e-12,
        _ => ((r >> 8) % 4000) as f64 / 100.0 - 20.0,
    };
    let (mut burned, mut refused) = (0, 0);
    for round in 0..400 {
        let n = (next() % 9) as usize;
        let outline: Vec<[f64; 2]> = (0..n).map(|_| [num(next()), num(next())]).collect();
        let gap = if next() % 4 == 0 { num(next()) } else { 0.1 };
        let moves: Vec<Move> = (0..(next() % 4))
            .map(|_| {
                let kind = if next() % 5 == 0 { MoveKind::Rapid } else { MoveKind::Feed };
                mv(kind, p(num(next()), num(next()), num(next())), p(num(next()), num(next()), num(next())))
            })
            .collect();
        let mut d = Dexel::block(-20.0, -20.0, 20.0, 20.0, TOP, 1.0).expect("a block");
        match simulate_outline(&mut d, &moves, &outline, gap) {
            Ok(()) => {
                burned += 1;
                assert!((0..d.cells()).all(|k| d.top_at(k) <= TOP), "round {round}: a burn raised a column");
            }
            Err(e) => {
                refused += 1;
                assert!(!e.to_string().is_empty());
                assert!(d == Dexel::block(-20.0, -20.0, 20.0, 20.0, TOP, 1.0).expect("a block"), "round {round}: a refusal lowered a column");
            }
        }
    }
    assert!(burned > 0 && refused > 100, "{burned} burned, {refused} refused");
}
