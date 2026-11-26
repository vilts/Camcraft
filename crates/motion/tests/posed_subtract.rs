//! M13.7: motion's direct `Dexel::subtract` calls keep the column
//! engine's door. The dexel atom stands upright on +Z, so a posed
//! cutting move lowered through it would be a tilted tool cut
//! vertically - a silent wrong stock. The feed pass (the one direct
//! subtract that reads a caller's path) refuses it by name, with the
//! stock sim's own refusal text; an upright path and a posed rapid
//! (which cuts nothing) still pass.
//!
//! Provenance: our own fixtures, specced from the Prime Directive; no
//! vendor material.
//!
//! Gate map (M13.7): covers camcraft_motion::machining3d (feed
//! optimize_feed, entry run_rest via subtract_upright),
//! camcraft_sim::stock_sim (first_posed, POSED_REFUSAL).

use camcraft_motion::cl::MoveKind;
use camcraft_motion::geo2d::{Loop, Point2, Region};
use camcraft_motion::geometry::{Point3, Triangle};
use camcraft_motion::machining2d::{GenError, PocketStrategy};
use camcraft_motion::machining3d::{Entry3D, Pocket3DParams, optimize_feed, pocket3d};
use camcraft_motion::stock_sim::POSED_REFUSAL;

fn p(x: f64, y: f64, z: f64) -> Point3 {
    Point3::new(x, y, z)
}

/// An axis-aligned box, outward-wound.
fn boxf(min: [f64; 3], max: [f64; 3]) -> Vec<Triangle> {
    let v = |i: usize| p([min[0], max[0]][i & 1], [min[1], max[1]][(i >> 1) & 1], [min[2], max[2]][(i >> 2) & 1]);
    let quad = |a: usize, b: usize, c: usize, d: usize| [Triangle::new(v(a), v(b), v(c)), Triangle::new(v(a), v(c), v(d))];
    [quad(4, 5, 7, 6), quad(0, 2, 3, 1), quad(0, 1, 5, 4), quad(2, 6, 7, 3), quad(1, 3, 7, 5), quad(2, 0, 4, 6)]
        .into_iter()
        .flatten()
        .collect()
}

fn footprint() -> Region {
    let pts = [(-20.0, -20.0), (20.0, -20.0), (20.0, 20.0), (-20.0, 20.0)];
    let lp = Loop::new(pts.iter().map(|&(x, y)| Point2::new(x, y)).collect());
    match lp.and_then(|l| Region::new(vec![l])) {
        Ok(r) => r,
        Err(e) => panic!("fixture region: {e}"),
    }
}

fn params() -> Pocket3DParams {
    Pocket3DParams {
        pass_start: Default::default(),
        links: camcraft_motion::links::Links::ROUGH3D,
        tool_diameter: 8.0,
        stepover: 3.0,
        stock_to_leave: 0.0,
        clearance_z: 5.0,
        top_z: 0.0,
        bottom_z: -10.0,
        stepdown: 5.0,
        feed: 500.0,
        plunge_feed: 100.0,
        climb: true,
        strategy: PocketStrategy::Zigzag,
        retract_z: None,
        entry: Entry3D::Plunge,
        helix_radius: 1.0,
        optimal_load: camcraft_motion::load::DEFAULT_OPTIMAL_LOAD,
    }
}

#[test]
fn a_posed_cut_through_the_direct_dexel_subtract_is_refused_by_name() {
    let f = boxf([-20.0, -20.0, -20.0], [20.0, 20.0, -10.0]);
    let (region, prm) = (footprint(), params());
    let upright = match pocket3d(&f, &region, &prm) {
        Ok(t) => t,
        Err(e) => panic!("the slab pocket generates: {e}"),
    };
    assert!(optimize_feed(&upright, &f, &region, &prm).is_ok(), "an upright path runs the feed pass");

    // A posed rapid cuts nothing (RS-274 G0): not refused.
    let mut rapid_posed = upright.clone();
    let Some(r) = rapid_posed.moves.iter().position(|m| m.kind == MoveKind::Rapid) else {
        panic!("the pocket has a rapid");
    };
    rapid_posed.moves[r].axis = Some([0.6, 0.0, 0.8]);
    assert!(optimize_feed(&rapid_posed, &f, &region, &prm).is_ok(), "a posed rapid is not a cut");

    // One tilted cutting move: the column engine would cut it upright.
    let mut posed = upright.clone();
    let Some(k) = posed.moves.iter().position(|m| m.kind == MoveKind::Feed) else {
        panic!("the pocket has a feed");
    };
    posed.moves[k].axis = Some([0.6, 0.0, 0.8]);
    match optimize_feed(&posed, &f, &region, &prm) {
        Err(GenError::Invalid(msg)) => {
            assert!(msg.contains(POSED_REFUSAL), "refused by the engine's own name: {msg}");
            assert!(msg.contains(&format!("move {k}")), "the refusal names the move: {msg}");
        }
        other => panic!("a posed cut must be refused, got {other:?}"),
    }
}
