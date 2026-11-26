//! Inside-corner relief (#2400) on the rest passes of the 2D Pocket, in
//! both rest modes (#1446 entered tool, #2033 previous operations). A
//! 30 x 30 pocket with sharp corners; a 6 mm rest tool with a dogbone
//! relief. The previous-operations mode cuts the same dogbone ends as the
//! entered-tool mode, and none with the relief off. A bad relief is
//! refused in both of its branches.
//!
//! Gate map: covers camcraft_motion::machining2d (rest2d emit_rest,
//! rest_prior rest_pocket_previous_with_relief).
//!
//! Provenance: our own fixtures; no other CAM product's code, text or
//! output consulted.

use camcraft_motion::cl::Toolpath;
use camcraft_motion::entry::Entry3D;
use camcraft_motion::geo2d::{Loop, Point2, Region};
use camcraft_motion::links::Links;
use camcraft_motion::load::DEFAULT_OPTIMAL_LOAD;
use camcraft_motion::machining2d::{
    CornerRelief, CornerReliefChoice, CornerReliefKind, CornerReliefOverride, PocketParams, PocketStrategy,
    RestParams, RestSource, pocket_relief_plan, rest_pocket_previous_with_relief, rest_pocket_with_relief,
};
use camcraft_motion::stock_sim::{Dexel, Stock};

const TOP: f64 = 20.0;
const FLOOR: f64 = 15.0;
const TOOL: f64 = 6.0;

fn pocket() -> Region {
    Region::new(vec![
        Loop::new(vec![
            Point2::new(0.0, 0.0),
            Point2::new(30.0, 0.0),
            Point2::new(30.0, 30.0),
            Point2::new(0.0, 30.0),
        ])
        .expect("square loop"),
    ])
    .expect("square region")
}

fn params() -> PocketParams {
    PocketParams {
        tool_diameter: TOOL,
        stepover: 2.0,
        stock_to_leave: 0.0,
        clearance_z: TOP + 5.0,
        top_z: TOP,
        bottom_z: FLOOR,
        stepdown: TOP - FLOOR,
        feed: 1000.0,
        plunge_feed: 250.0,
        climb: true,
        strategy: PocketStrategy::Waterline,
        retract_z: None,
        entry: Entry3D::Plunge,
        helix_radius: 1.0,
        optimal_load: DEFAULT_OPTIMAL_LOAD,
        blend_turns: false,
        blend_radius: None,
        links: Links::PLANAR,
    }
}

fn dogbone() -> CornerRelief {
    CornerRelief { kind: CornerReliefKind::Dogbone, ..Default::default() }
}

fn from_ops() -> RestParams {
    RestParams {
        source: RestSource::PreviousOperations,
        previous_tool_diameter: 0.0,
        previous_corner_radius: 0.0,
        previous_stock_to_leave: 0.0,
        allowance: 0.0,
    }
}

fn entered() -> RestParams {
    RestParams { source: RestSource::EnteredTool, previous_tool_diameter: 16.0, ..from_ops() }
}

/// The dogbone ends of the full pocket for the rest tool.
fn relief_ends() -> Vec<Point2> {
    let plan = pocket_relief_plan(&pocket(), TOOL, 0.0, &dogbone()).expect("relief plan");
    assert_eq!(plan.reliefs.len(), 4, "four sharp corners");
    plan.reliefs.iter().map(|r| r.end).collect()
}

/// Which of `ends` the path reaches at the floor.
fn reached(path: &Toolpath, ends: &[Point2]) -> Vec<bool> {
    assert!(path.all_finite(), "non-finite moves");
    ends.iter()
        .map(|e| path.moves.iter().any(|m| (m.to.x - e.x).hypot(m.to.y - e.y) <= 1e-2 && m.to.z <= FLOOR + 1e-6))
        .collect()
}

fn count(r: &[bool]) -> usize {
    r.iter().filter(|b| **b).count()
}

/// A block whose columns inside `4..26` are cut to the floor: the
/// walls and the sharp corners stand.
fn roughed() -> Stock {
    let mut s = Stock::Column(Dexel::block(-5.0, -5.0, 35.0, 35.0, TOP, 0.25).expect("block"));
    s.set_tops(|_, c, cur| if c.x > 4.0 && c.x < 26.0 && c.y > 4.0 && c.y < 26.0 { FLOOR } else { cur })
        .expect("cut");
    s
}

/// The previous-operations mode keeps the reliefs of the rest pass
/// (#2400). When the earlier cut left the corners, it cuts the same
/// dogbone ends as the entered-tool mode. When nothing was cut, it cuts
/// every end, as the plain pocket does. With the relief off, it cuts none.
#[test]
fn rest_from_the_previous_operations_cuts_the_corner_reliefs() {
    let ends = relief_ends();
    let s = roughed();
    let cols = s.columns().expect("columns");
    let entered_on = rest_pocket_with_relief(&pocket(), &params(), &entered(), &dogbone()).expect("entered + relief");
    let entered_ends = reached(&entered_on, &ends);
    assert!(count(&entered_ends) > 0, "the entered-tool rest pass cuts a dogbone");
    let entered_off =
        rest_pocket_with_relief(&pocket(), &params(), &entered(), &CornerRelief::default()).expect("entered");
    assert_eq!(count(&reached(&entered_off, &ends)), 0, "no relief, no dogbone end");

    let on = rest_pocket_previous_with_relief(&pocket(), &params(), &from_ops(), Some(&cols), &dogbone())
        .expect("rest + relief");
    assert_eq!(reached(&on, &ends), entered_ends, "the same dogbone ends as the entered-tool mode");
    let whole = rest_pocket_previous_with_relief(&pocket(), &params(), &from_ops(), None, &dogbone())
        .expect("whole pocket + relief");
    assert_eq!(count(&reached(&whole, &ends)), 4, "the whole-pocket pass cuts every dogbone end");
    for prior in [Some(&cols), None] {
        let off = rest_pocket_previous_with_relief(&pocket(), &params(), &from_ops(), prior, &CornerRelief::default())
            .expect("rest");
        assert_eq!(count(&reached(&off, &ends)), 0, "no relief, no dogbone end (prior read: {})", prior.is_some());
    }
}

/// The relief refusal of #2400 still fires in the previous-operations mode.
#[test]
fn a_bad_relief_is_refused_in_the_previous_operations_mode() {
    let bad = CornerRelief {
        overrides: vec![CornerReliefOverride { at: Point2::new(f64::NAN, 0.0), choice: CornerReliefChoice::Dogbone }],
        ..dogbone()
    };
    let s = roughed();
    let cols = s.columns().expect("columns");
    for prior in [Some(&cols), None] {
        let why = rest_pocket_previous_with_relief(&pocket(), &params(), &from_ops(), prior, &bad)
            .expect_err("a non-finite override is refused")
            .to_string();
        assert!(why.contains("non-finite coordinates"), "{why}");
    }
}
