//! A 2D Pocket rest pass with inside-corner reliefs (#2400) cuts every
//! relief whose corner the rest area touches (#3146).
//!
//! Gate map: covers camcraft_motion::machining2d (corner_relief, corner_relief_spur, rest2d).

use camcraft_motion::cl::Toolpath;
use camcraft_motion::entry::Entry3D;
use camcraft_motion::geo2d::{Loop, Point2, Region};
use camcraft_motion::links::Links;
use camcraft_motion::load::DEFAULT_OPTIMAL_LOAD;
use camcraft_motion::machining2d::{
    CornerRelief, CornerReliefChoice, CornerReliefKind, CornerReliefOverride, PocketParams, PocketStrategy,
    RestParams, pocket_relief_plan, rest_pocket_with_relief,
};

fn square(side: f64) -> Region {
    let l = Loop::new(vec![
        Point2::new(0.0, 0.0),
        Point2::new(side, 0.0),
        Point2::new(side, side),
        Point2::new(0.0, side),
    ])
    .expect("square loop");
    Region::new(vec![l]).expect("square region")
}

fn params(strategy: PocketStrategy) -> PocketParams {
    PocketParams {
        tool_diameter: 6.0,
        stepover: 3.0,
        stock_to_leave: 0.0,
        clearance_z: 25.0,
        top_z: 20.0,
        bottom_z: 15.0,
        stepdown: 5.0,
        feed: 1000.0,
        plunge_feed: 250.0,
        climb: true,
        strategy,
        retract_z: None,
        entry: Entry3D::Plunge,
        helix_radius: 1.0,
        optimal_load: DEFAULT_OPTIMAL_LOAD,
        blend_turns: false,
        blend_radius: None,
        links: Links::PLANAR,
    }
}

fn rest(previous_tool_diameter: f64) -> RestParams {
    RestParams { source: Default::default(), previous_tool_diameter, previous_corner_radius: 0.0, previous_stock_to_leave: 0.0, allowance: 0.0 }
}

fn visits_at_floor(path: &Toolpath, target: Point2) -> bool {
    path.moves
        .iter()
        .any(|mv| (mv.to.x - target.x).hypot(mv.to.y - target.y) <= 1e-2 && mv.to.z <= 15.0 + 1e-6)
}

/// #3146: the relief starts are on the boundary of the rest center area.
/// A plain point-in-region test kept only one of the four; each dogbone
/// end must be cut, for each rest strategy.
#[test]
fn rest_pass_cuts_all_four_dogbones_of_a_sharp_square_pocket() {
    let reg = square(30.0);
    let relief = CornerRelief { kind: CornerReliefKind::Dogbone, ..Default::default() };
    let plan = pocket_relief_plan(&reg, 6.0, 0.0, &relief).expect("relief plan");
    assert_eq!(plan.reliefs.len(), 4, "a sharp square has four dogbones");
    for strategy in [PocketStrategy::Waterline, PocketStrategy::OffsetRings, PocketStrategy::Zigzag] {
        let path = rest_pocket_with_relief(&reg, &params(strategy), &rest(16.0), &relief)
            .unwrap_or_else(|e| panic!("{strategy:?}: rest pass failed: {e}"));
        assert!(path.all_finite(), "{strategy:?}: non-finite moves");
        let missed: Vec<Point2> = plan.reliefs.iter().map(|r| r.end).filter(|&e| !visits_at_floor(&path, e)).collect();
        assert!(missed.is_empty(), "{strategy:?}: rest pass missed dogbone ends {missed:?}");
        assert!(
            !path.notes.iter().any(|n| n.contains("corner relief")),
            "{strategy:?}: no relief was dropped, so no relief note: {:?}",
            path.notes
        );
    }
}

/// #3146: a relief that the rest pass does not cut is named in a note.
/// The forced dogbone sits on a 176 deg corner in the middle of the floor
/// edge. The previous 16 mm tool leaves there only a film under
/// `REST_FILM`, so the rest area does not reach that corner.
#[test]
fn rest_pass_notes_a_forced_relief_the_rest_area_does_not_reach() {
    let dip = Point2::new(30.0, -1.05);
    let l = Loop::new(vec![
        Point2::new(0.0, 0.0),
        dip,
        Point2::new(60.0, 0.0),
        Point2::new(60.0, 30.0),
        Point2::new(0.0, 30.0),
    ])
    .expect("bent loop");
    let reg = Region::new(vec![l]).expect("bent region");
    let relief = CornerRelief {
        kind: CornerReliefKind::None,
        overrides: vec![CornerReliefOverride { at: dip, choice: CornerReliefChoice::Dogbone }],
        ..Default::default()
    };
    let plan = pocket_relief_plan(&reg, 6.0, 0.0, &relief).expect("relief plan");
    assert_eq!(plan.reliefs.len(), 1, "the override forces one dogbone");
    let path = rest_pocket_with_relief(&reg, &params(PocketStrategy::Waterline), &rest(16.0), &relief)
        .expect("rest pass");
    assert!(!visits_at_floor(&path, plan.reliefs[0].end), "the rest pass does not reach the dip");
    assert!(
        path.notes.iter().any(|n| n.contains("corner relief at (30.000, -1.050) is not cut")),
        "the dropped relief must be named in a note: {:?}",
        path.notes
    );
}
