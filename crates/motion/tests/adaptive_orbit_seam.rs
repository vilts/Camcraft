//! Issue #980: the HSM plan's walk orbits the entry from its start and
//! leaves the orbit where it closed - the start point itself. Computed
//! as `a0 + TAU`, the orbit's closing vertex stood an ulp off its start,
//! and the plan's crossing law (the orientation predicate, `guard`) read
//! the spiral's first chord out of it as a proper crossing of the
//! orbit's first chord: the plan was refused and the level cut with
//! Offset rings. The fixture is the center area the Z 10 level of the
//! user's `Milling-test.step` hands the planner since #839's 0.005 mm
//! STEP chord (dumped from the 3-D ladder, 287 vertices), grown back by
//! the tool radius into a pocket boundary; before the fix its plan read
//! "crosses itself at (-2.09, -0.13)".
//!
//! Gate map (M4.4): covers camcraft_motion::machining2d (adaptive rings
//! circle, guard, pocket_hsm).

use camcraft_motion::entry::Entry3D;
use camcraft_motion::geo2d::{Loop, Point2, Region};
use camcraft_motion::load::DEFAULT_OPTIMAL_LOAD;
use camcraft_motion::machining2d::{PocketParams, PocketStrategy, pocket};

/// The tool radius of the user's job (EM-1000-4FL, D10).
const R: f64 = 5.0;

fn params() -> PocketParams {
    PocketParams {
        tool_diameter: 2.0 * R,
        stepover: 4.0,
        stock_to_leave: 0.0,
        clearance_z: 5.0,
        retract_z: Some(1.0),
        top_z: 0.0,
        bottom_z: -5.0,
        stepdown: 5.0,
        feed: 1000.0,
        plunge_feed: 300.0,
        climb: true,
        strategy: PocketStrategy::Adaptive,
        entry: Entry3D::Helix,
        helix_radius: 2.0,
        optimal_load: DEFAULT_OPTIMAL_LOAD,
        blend_turns: false,
        links: camcraft_motion::links::Links::PLANAR,
    }
}

#[test]
fn the_users_z10_bore_level_keeps_its_hsm_plan() {
    let pts: Vec<Point2> = include_str!("fixtures/bore_z10_center.txt")
        .lines()
        .filter_map(|l| {
            let mut it = l.split_whitespace().map(str::parse::<f64>);
            match (it.next(), it.next()) {
                (Some(Ok(x)), Some(Ok(y))) => Some(Point2::new(x, y)),
                _ => None,
            }
        })
        .collect();
    assert_eq!(pts.len(), 287, "the Z 10 center outline");
    let Ok(l) = Loop::new(pts) else { panic!("fixture loop") };
    let Ok(center) = Region::new(vec![l]) else { panic!("fixture region") };
    let Ok(reg) = center.offset(R) else { panic!("fixture boundary") };
    let tp = match pocket(&reg, &params()) {
        Ok(tp) => tp,
        Err(e) => panic!("the bore plans: {e}"),
    };
    // The HSM plan stands: no fallback of any kind is named.
    assert!(!tp.notes.iter().any(|n| n.contains("crosses itself") || n.contains("Offset rings")), "{:?}", tp.notes);
    assert!(tp.moves.iter().any(|m| m.arc.is_some()), "the HSM plan posts its fitted arcs: {:?}", tp.notes);
}
