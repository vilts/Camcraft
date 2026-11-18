//! #2308: a stock-source 2D Chamfer generated clean, then a part added
//! under the stock's edge it breaks. The program sim judges the stock
//! (aligned, and turned a quarter about Z since #2177), and before this
//! fix its verdict was the post's only judge of the added part: the
//! chamfer's declared edge break (#303) was credited over that part's
//! columns, the sim found no gouge, the live re-judge of the kept parts
//! (#1628) did not run, and `G01 X35. Y-35. Z19.` posted along the part's
//! edge. Now the post refuses by name - the op and the part - also for a
//! cut too shallow for the sim's lattice (0.05 mm), and a part added clear
//! of the cut still posts.
//!
//! Provenance: our own plate fixture (`stock_source_holes_support`) over
//! the solid law and the column sim; no vendor material consulted.
//!
//! Gate map: covers app::posting_sim (judge_clause, kept_moved),
//! app::stock_sim (program_breaks), app::operation::kept_rejudge,
//! app::posting.

mod common;
mod pocket_support;
mod stock_source_holes_support;

use camcraft_app::geometry::quat_from_yaw;
use stock_source_holes_support::*;

/// The three stock turns: aligned, a quarter about Z (both judged by
/// the program sim) and a hair off the axes (not judged by the sim).
const TURNS: [(&str, f64); 3] = [("aligned", 0.0), ("quarter", std::f64::consts::FRAC_PI_2), ("hair", 1e-6)];

/// The plate as "part" in the roomy `2 HALF` square stock turned by
/// `yaw` about Z, and the stock-source 1 mm "S1" chamfer generated
/// clean round the stock's top edge.
fn clean_chamfer(yaw: f64) -> (World, History) {
    let (world, mut h) = job(Model::hole(3.0, 8.0), HALF, false);
    if yaw != 0.0 {
        let at = Location { pos: Point3::new(0.0, 0.0, 0.0), rot: quat_from_yaw(yaw) };
        h.execute(&world, SetStock::new(&world, StockShape::Box { dx: 2.0 * HALF, dy: 2.0 * HALF, dz: TOP }, at).expect("stock"));
    }
    let warns = generated(&world, &mut h, CHAMFER, chamfer(1.0));
    let blocking: Vec<_> = warns.iter().filter(|w| w.blocks_post()).collect();
    assert!(blocking.is_empty(), "yaw {yaw}: a clean generation: {blocking:?}");
    (world, h)
}

/// Add the plate "other" over `x0..x1`, `y -5..5`, `z 0..TOP`.
fn add_other(world: &World, h: &mut History, x0: f64, x1: f64) {
    let mut f = Vec::new();
    common::solid_part::plate(&common::solid_part::rect(x0, -5.0, x1, 5.0), None, 0.0, TOP, &mut f);
    h.execute(world, ImportModel::from_mesh("other", Mesh::from_facets(f)));
}

#[test]
fn a_part_added_under_a_judged_stock_source_chamfer_is_refused_by_name() {
    for (label, yaw) in TURNS {
        let (world, mut h) = clean_chamfer(yaw);
        assert_posts(&world, &format!("added-under-{label}-before"));
        // The stock's edge stands at x 35: "other" reaches it, so the
        // chamfer's cone breaks the top edge of "other".
        add_other(&world, &mut h, 30.0, 35.0);
        assert_not_posted(&world, &format!("added-under-{label}"), "S1 now cuts part 'other'");
        assert_not_posted(&world, &format!("added-under-{label}-cause"), "stock-source chamfer breaks into a part");
        // Where the sim judges, it convicts the cut itself: the bevel of
        // the stock's edge is no credit over the part.
        if label != "hair" {
            assert_not_posted(&world, &format!("added-under-{label}-sim"), "gouge: program cuts below the model");
        }
    }
}

#[test]
fn a_part_added_a_hair_under_a_judged_stock_source_chamfer_is_refused_by_name() {
    // "other" reaches 0.05 mm past the cone's foot at x 34: a cut too
    // shallow for the sim's lattice, refused by the live re-judge.
    for (label, yaw) in TURNS.into_iter().filter(|(l, _)| *l != "hair") {
        let (world, mut h) = clean_chamfer(yaw);
        add_other(&world, &mut h, 30.0, 34.05);
        assert_not_posted(&world, &format!("shallow-{label}"), "S1 now cuts part 'other'");
        assert_not_posted(&world, &format!("shallow-{label}-cause"), "stock-source chamfer breaks into a part");
    }
}

#[test]
fn a_part_added_clear_of_a_stock_source_chamfer_still_posts() {
    // "other" stands 10 mm inside the stock's edge: the 6 mm cone never
    // reaches it, on any turn.
    for (label, yaw) in TURNS {
        let (world, mut h) = clean_chamfer(yaw);
        add_other(&world, &mut h, 18.0, 24.0);
        assert_posts(&world, &format!("added-clear-{label}"));
    }
}
