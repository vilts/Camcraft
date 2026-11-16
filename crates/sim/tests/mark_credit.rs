//! #1595: the operation owns its mark. An Engrave or Trace on the
//! finished part is judged against the model with the declared mark cut
//! (`stock_sim::credit_marks`): what the mark's own cutter swept, no
//! deeper than its budget under the model's top, is the finished part;
//! past the budget it is still a gouge charged at its depth under the
//! budget; a column the mark never swept, or one another operation cut
//! deeper, keeps its charge.
//!
//! Gate map (#1595): covers camcraft_sim::stock_sim (mark, edge_break's
//! credit_floors, deviate).

use camcraft_sim::cl::{Move, MoveKind};
use camcraft_sim::geometry::{cube, Point3, Triangle};
use camcraft_sim::stock_sim::{self, credit_marks, deviate, Dexel, Mark, SimOp, DEV_TOL};

fn p(x: f64, y: f64, z: f64) -> Point3 {
    Point3::new(x, y, z)
}

fn feed(from: Point3, to: Point3) -> Move {
    Move { rotary: None, kind: MoveKind::Feed, from, to, feed: Some(500.0), arc: None, axis: None }
}

/// The finished part: a 20 x 20 x 10 block.
fn part() -> Vec<Triangle> {
    cube(p(0.0, 0.0, 0.0), p(20.0, 20.0, 10.0)).facets.to_vec()
}

/// One engraved line across the top face, y = `y`, the tip at `z`.
fn line(y: f64, z: f64) -> Vec<Move> {
    vec![feed(p(4.0, y, z), p(16.0, y, z))]
}

fn op(moves: &[Move], ball: bool) -> SimOp<'_> {
    SimOp { moves, radius: 1.0, ball, cone: None, corner: None }
}

/// The part as stock, cut by `ops` in order.
fn cut(ops: &[SimOp]) -> Dexel {
    let mut d = Dexel::from_facets(&part(), 0.125).expect("the finished part as stock");
    stock_sim::simulate(&mut d, ops).expect("the program cuts");
    d
}

#[test]
fn a_mark_within_its_budget_is_the_finished_part_not_a_gouge() {
    for ball in [false, true] {
        let moves = line(10.0, 9.7);
        let d = cut(&[op(&moves, ball)]);
        let facets = part();
        let mut dev = deviate(&d, &facets, DEV_TOL).expect("verdict");
        assert!(dev.gouge_cells > 0, "the sharp model convicts every engraving - the bug's own reading");
        let credited = credit_marks(&mut dev, &d, &facets, &[Mark { op: op(&moves, ball), budget: 0.3, target: None }]);
        assert!(credited > 0);
        assert_eq!(dev.gouge_cells, 0, "ball {ball}: worst {} at {:?}", dev.max_gouge, dev.max_gouge_at);
        assert!(dev.is_clean());
    }
}

#[test]
fn a_mark_past_its_budget_is_still_a_gouge_at_its_excess() {
    // Cut 1.0 deep on a 0.3 budget: 0.7 under the declared floor.
    let moves = line(10.0, 9.0);
    let d = cut(&[op(&moves, false)]);
    let facets = part();
    let mut dev = deviate(&d, &facets, DEV_TOL).expect("verdict");
    assert!((dev.max_gouge - 1.0).abs() < 1e-9);
    credit_marks(&mut dev, &d, &facets, &[Mark { op: op(&moves, false), budget: 0.3, target: None }]);
    assert!(dev.gouge_cells > 0, "an over-deep mark passed");
    assert!((dev.max_gouge - 0.7).abs() < 1e-9, "charged {}", dev.max_gouge);
    assert!(dev.max_gouge_at.is_some(), "the worst gouge still locates itself");
}

#[test]
fn a_column_the_mark_never_swept_keeps_the_models_verdict() {
    // Two lines cut alike; only the first is a declared mark.
    let (marked, other) = (line(6.0, 9.7), line(14.0, 9.7));
    let d = cut(&[op(&marked, false), op(&other, false)]);
    let facets = part();
    let mut dev = deviate(&d, &facets, DEV_TOL).expect("verdict");
    let before = dev.gouge_cells;
    let credited = credit_marks(&mut dev, &d, &facets, &[Mark { op: op(&marked, false), budget: 0.3, target: None }]);
    assert!(credited > 0 && dev.gouge_cells > 0 && dev.gouge_cells < before, "{} of {before}", dev.gouge_cells);
    let at = dev.max_gouge_at.expect("located");
    assert!((at.y - 14.0).abs() <= 1.0 + 0.125, "the charge left stands on the undeclared line: {at:?}");
    assert!((dev.max_gouge - 0.3).abs() < 1e-9);
}

#[test]
fn another_operations_cut_under_the_mark_is_charged_under_the_marks_floor() {
    // The mark is 0.3 deep; another op then cuts the same line to 0.8.
    let (mark, deep) = (line(10.0, 9.7), line(10.0, 9.2));
    let d = cut(&[op(&mark, false), op(&deep, false)]);
    let facets = part();
    let mut dev = deviate(&d, &facets, DEV_TOL).expect("verdict");
    credit_marks(&mut dev, &d, &facets, &[Mark { op: op(&mark, false), budget: 0.3, target: None }]);
    assert!(dev.gouge_cells > 0, "a gouge under a mark was forgiven");
    assert!((dev.max_gouge - 0.5).abs() < 1e-9, "charged {} under the mark's own floor", dev.max_gouge);
}

#[test]
fn a_budget_wider_than_the_cut_credits_only_what_the_cutter_swept() {
    // A 3 mm budget over a 0.3 mm ball mark: the floor is the ball's own
    // groove, so another op's flat cut to the same depth beside the
    // centre line (deeper than the ball reached there) is still charged.
    let mark = line(10.0, 9.7);
    let flat = line(10.0, 9.7);
    let d = cut(&[op(&mark, true), op(&flat, false)]);
    let facets = part();
    let mut dev = deviate(&d, &facets, DEV_TOL).expect("verdict");
    let before = dev.gouge_cells;
    credit_marks(&mut dev, &d, &facets, &[Mark { op: op(&mark, true), budget: 3.0, target: None }]);
    assert!(dev.gouge_cells > 0, "the flat cutter's corners beside the ball's groove were forgiven");
    assert!(dev.gouge_cells < before, "the ball's own groove is credited: {} of {before}", dev.gouge_cells);
    // The ball (radius 1, 0.3 deep) leaves the surface 0.714 mm off the
    // line; the worst charge left stands outside that, at the flat's edge.
    let at = dev.max_gouge_at.expect("located");
    assert!((at.y - 10.0).abs() > 0.7, "charged beside the ball's groove: {at:?}");
    assert!((dev.max_gouge - 0.3).abs() < 1e-9, "at the model's own depth there: {}", dev.max_gouge);
}

#[test]
fn a_mark_with_no_budget_declares_nothing() {
    let moves = line(10.0, 9.7);
    let d = cut(&[op(&moves, false)]);
    let facets = part();
    for budget in [0.0, -1.0, f64::NAN, f64::INFINITY] {
        let mut dev = deviate(&d, &facets, DEV_TOL).expect("verdict");
        let before = (dev.gouge_cells, dev.max_gouge);
        assert_eq!(credit_marks(&mut dev, &d, &facets, &[Mark { op: op(&moves, false), budget, target: None }]), 0, "{budget}");
        assert_eq!((dev.gouge_cells, dev.max_gouge), before, "{budget}");
    }
}

/// The review's thin plate (PR #1615): 20 x 20 x 2, Z 0..2.
fn plate() -> Vec<Triangle> {
    cube(p(0.0, 0.0, 0.0), p(20.0, 20.0, 2.0)).facets.to_vec()
}

/// The plate as stock, cut by `ops`; the sharp verdict on it.
fn plate_cut(ops: &[SimOp]) -> (Dexel, stock_sim::Deviation) {
    let mut d = Dexel::from_facets(&plate(), 0.125).expect("the plate as stock");
    stock_sim::simulate(&mut d, ops).expect("the program cuts");
    let dev = deviate(&d, &plate(), DEV_TOL).expect("verdict");
    (d, dev)
}

#[test]
fn a_budget_deeper_than_the_plate_never_credits_a_cut_through_it() {
    // Top 2, Bottom -0.9 with a 6 mm cutter states a 2.9 mm "mark" on a
    // 2 mm plate: `top - budget` lies under the plate's underside, and
    // before the floor was bound to the part the whole column was
    // forgiven.
    let moves = line(10.0, -0.9);
    let (d, mut dev) = plate_cut(&[op(&moves, false)]);
    let before = (dev.gouge_cells, dev.max_gouge);
    assert!(before.0 > 0);
    credit_marks(&mut dev, &d, &plate(), &[Mark { op: op(&moves, false), budget: 2.9, target: None }]);
    assert_eq!(dev.gouge_cells, before.0, "every column cut through the plate stays convicted");
    // Charged under the floor the mark had to leave on the underside.
    let floor = stock_sim::MARK_FLOOR;
    assert!((dev.max_gouge - (before.1 - (2.0 - floor))).abs() < 1e-9, "charged {} of {}", dev.max_gouge, before.1);
    assert!(dev.max_gouge >= floor - 1e-9, "at least the floor itself is gone: {}", dev.max_gouge);
}

#[test]
fn a_mark_on_a_thin_plate_is_credited_down_to_the_floor_it_must_leave() {
    let floor = stock_sim::MARK_FLOOR;
    // 0.3 mm deep, and down to exactly the floor: both the finished part.
    for z in [1.7, floor] {
        let moves = line(10.0, z);
        let (d, mut dev) = plate_cut(&[op(&moves, false)]);
        assert!(dev.gouge_cells > 0);
        credit_marks(&mut dev, &d, &plate(), &[Mark { op: op(&moves, false), budget: 2.9, target: None }]);
        assert_eq!(dev.gouge_cells, 0, "tip at {z}: worst {} at {:?}", dev.max_gouge, dev.max_gouge_at);
    }
    // 0.1 mm of plate left: a breakthrough, charged at what is missing
    // from the floor.
    let moves = line(10.0, 0.1);
    let (d, mut dev) = plate_cut(&[op(&moves, false)]);
    credit_marks(&mut dev, &d, &plate(), &[Mark { op: op(&moves, false), budget: 2.9, target: None }]);
    assert!(dev.gouge_cells > 0, "0.1 mm of plate left passed as a mark");
    assert!((dev.max_gouge - (floor - 0.1)).abs() < 1e-9, "charged {}", dev.max_gouge);
}

#[test]
fn a_mark_over_a_void_is_credited_only_down_to_the_roof_it_must_leave() {
    // A bridge: two feet and a 2 mm deck (Z 6..8) over a tunnel along Y.
    let mut part = cube(p(0.0, 0.0, 0.0), p(5.0, 20.0, 6.0)).facets.to_vec();
    part.extend(cube(p(15.0, 0.0, 0.0), p(20.0, 20.0, 6.0)).facets.iter().copied());
    part.extend(cube(p(0.0, 0.0, 6.0), p(20.0, 20.0, 8.0)).facets.iter().copied());
    let run = |z: f64| {
        // Along Y over the middle of the tunnel.
        let moves = vec![feed(p(10.0, 4.0, z), p(10.0, 16.0, z))];
        let mut d = Dexel::from_facets(&part, 0.125).expect("the part as stock");
        stock_sim::simulate(&mut d, &[op(&moves, false)]).expect("cuts");
        let mut dev = deviate(&d, &part, DEV_TOL).expect("verdict");
        credit_marks(&mut dev, &d, &part, &[Mark { op: op(&moves, false), budget: 3.0, target: None }]);
        (dev.gouge_cells, dev.max_gouge)
    };
    assert_eq!(run(6.5).0, 0, "1.5 mm into a 2 mm deck leaves 0.5 mm of roof: a mark");
    let (cells, worst) = run(6.1);
    assert!(cells > 0 && (worst - (stock_sim::MARK_FLOOR - 0.1)).abs() < 1e-9, "0.1 mm of roof left: {cells} cells, worst {worst}");
}

#[test]
fn a_part_targeted_mark_never_credits_a_neighbouring_part() {
    // Part B on x [0, 20], Part A on x [-20, -0.7], both z [0, 10].
    // A mark along B's left edge (x = 0.0, radius 1.0, z = 9.7) sweeps
    // 0.3 mm into B and 0.3 mm into A across the 0.7 mm gap.
    let part_b = cube(p(0.0, 0.0, 0.0), p(20.0, 20.0, 10.0)).facets.to_vec();
    let part_a = cube(p(-20.0, 0.0, 0.0), p(-0.7, 20.0, 10.0)).facets.to_vec();
    let mut both = part_b.clone();
    both.extend(part_a.iter().copied());
    let moves = vec![feed(p(0.0, 4.0, 9.7), p(0.0, 16.0, 9.7))];
    let mut d = Dexel::from_facets(&both, 0.125).expect("both parts as stock");
    stock_sim::simulate(&mut d, &[op(&moves, false)]).expect("cuts");

    // Targeted at B alone: B's columns clear, A's stay convicted at 0.3.
    let mut dev = deviate(&d, &both, DEV_TOL).expect("verdict");
    let credited = credit_marks(&mut dev, &d, &both, &[Mark { op: op(&moves, false), budget: 0.3, target: Some(&part_b) }]);
    assert!(credited > 0, "B's own mark is still credited");
    assert!(dev.gouge_cells > 0, "A's columns must remain convicted");
    assert!((dev.max_gouge - 0.3).abs() < 1e-9, "charged {}", dev.max_gouge);
    let at = dev.max_gouge_at.expect("located");
    assert!(at.x <= -0.7, "remaining gouge must lie on A, got {at:?}");

    // Empty target credits nothing.
    let mut dev_empty = deviate(&d, &both, DEV_TOL).expect("verdict");
    assert_eq!(credit_marks(&mut dev_empty, &d, &both, &[Mark { op: op(&moves, false), budget: 0.3, target: Some(&[]) }]), 0);

    // Stock-source (`target: None`) credits every part, so both clear.
    let mut dev_all = deviate(&d, &both, DEV_TOL).expect("verdict");
    credit_marks(&mut dev_all, &d, &both, &[Mark { op: op(&moves, false), budget: 0.3, target: None }]);
    assert_eq!(dev_all.gouge_cells, 0, "worst {} at {:?}", dev_all.max_gouge, dev_all.max_gouge_at);
}

#[test]
fn a_part_targeted_mark_uses_its_own_top_and_underside() {
    // Part B (z 4..8) rests on Part A (z 0..4) on x [0, 20].
    // Combined undersides only see z = 0, so `target: None` would miss
    // B's own floor at z = 4; `target: Some(&part_b)` reads B's own
    // underside at z = 4 and blocks cutting within MARK_FLOOR of it.
    let part_a = cube(p(0.0, 0.0, 0.0), p(20.0, 20.0, 4.0)).facets.to_vec();
    let part_b = cube(p(0.0, 0.0, 4.0), p(20.0, 20.0, 8.0)).facets.to_vec();
    let mut both = part_a;
    both.extend(part_b.iter().copied());
    let moves = line(10.0, 4.1);
    let mut d = Dexel::from_facets(&both, 0.125).expect("stacked parts as stock");
    stock_sim::simulate(&mut d, &[op(&moves, false)]).expect("cuts");
    let mut dev = deviate(&d, &both, DEV_TOL).expect("verdict");
    credit_marks(&mut dev, &d, &both, &[Mark { op: op(&moves, false), budget: 5.0, target: Some(&part_b) }]);
    assert!(dev.gouge_cells > 0, "0.1 mm left on B's own underside must stay convicted");
    assert!((dev.max_gouge - (stock_sim::MARK_FLOOR - 0.1)).abs() < 1e-9, "charged {}", dev.max_gouge);

    // Part A (z 6..10) overhangs Part B (z 0..4): a mark targeting B
    // must not credit a cut at z = 9.7 into A's top even though B lies
    // underneath in XY.
    let under_b = cube(p(0.0, 0.0, 0.0), p(20.0, 20.0, 4.0)).facets.to_vec();
    let over_a = cube(p(0.0, 0.0, 6.0), p(20.0, 20.0, 10.0)).facets.to_vec();
    let mut stacked = under_b.clone();
    stacked.extend(over_a);
    let top_cut = line(10.0, 9.7);
    let mut d2 = Dexel::from_facets(&stacked, 0.125).expect("overhung parts as stock");
    stock_sim::simulate(&mut d2, &[op(&top_cut, false)]).expect("cuts");
    let mut dev2 = deviate(&d2, &stacked, DEV_TOL).expect("verdict");
    assert_eq!(credit_marks(&mut dev2, &d2, &stacked, &[Mark { op: op(&top_cut, false), budget: 0.3, target: Some(&under_b) }]), 0);
    assert!(dev2.gouge_cells > 0 && (dev2.max_gouge - 0.3).abs() < 1e-9);
}
