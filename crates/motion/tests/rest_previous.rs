//! 2D rest machining from the previous operations (#2033): the rest area
//! holds every column that an independent simulation of the earlier
//! operations leaves standing at a cut depth, the rest path cuts it and
//! keeps the walls, a shallow earlier pass leaves the whole pocket, and
//! the two rest modes refuse each other's planner by name.
//!
//! Gate map (#2033): covers camcraft_motion::machining2d (rest_prior,
//! rest2d).

use camcraft_motion::cl::Toolpath;
use camcraft_motion::entry::Entry3D;
use camcraft_motion::geo2d::{Loop, Point2, Region};
use camcraft_motion::load::DEFAULT_OPTIMAL_LOAD;
use camcraft_motion::machining2d::{
    cut_levels, pocket, rest_plan_stock, rest_pocket, rest_pocket_previous, rest_stock_from_columns, GenError,
    PocketParams, PocketStrategy, RestParams, RestSource, PRIOR_Z_TOL, REST_NEEDS_STOCK,
};
use camcraft_motion::stock_sim::{Dexel, SimOp, Stock};

const CELL: f64 = 0.25;
const TOP: f64 = 0.0;
const BOTTOM: f64 = -4.0;
const TOOL_D: f64 = 4.0;

fn rect(x0: f64, y0: f64, x1: f64, y1: f64) -> Region {
    Region::new(vec![
        Loop::new(vec![Point2::new(x0, y0), Point2::new(x1, y0), Point2::new(x1, y1), Point2::new(x0, y1)]).unwrap(),
    ])
    .unwrap()
}

/// A 40 x 30 pocket with 3 mm corners, inside a 50 x 40 block.
fn pocket_region() -> Region {
    rect(5.0, 5.0, 45.0, 35.0).offset(-3.0).unwrap().offset(3.0).unwrap()
}

fn block() -> Stock {
    Stock::Column(Dexel::block(0.0, 0.0, 50.0, 40.0, TOP, CELL).unwrap())
}

fn pp(d: f64, bottom: f64, stock_to_leave: f64) -> PocketParams {
    PocketParams {
        blend_radius: None,
        links: camcraft_motion::links::Links::PLANAR,
        tool_diameter: d,
        stepover: d * 0.4,
        stock_to_leave,
        clearance_z: 5.0,
        retract_z: None,
        top_z: TOP,
        bottom_z: bottom,
        stepdown: 2.0,
        feed: 400.0,
        plunge_feed: 80.0,
        climb: true,
        strategy: PocketStrategy::Waterline,
        entry: Entry3D::Plunge,
        helix_radius: 1.0,
        optimal_load: DEFAULT_OPTIMAL_LOAD,
        blend_turns: false,
    }
}

fn previous_ops() -> RestParams {
    RestParams {
        source: RestSource::PreviousOperations,
        previous_tool_diameter: 0.0,
        previous_corner_radius: 0.0,
        previous_stock_to_leave: 0.0,
        allowance: 0.0,
    }
}

fn cut(stock: &mut Stock, path: &Toolpath, d: f64, corner: Option<f64>) {
    stock.simulate(&[SimOp { moves: &path.moves, radius: d * 0.5, ball: false, cone: None, corner }]).unwrap();
}

/// The centers of the columns inside `region` whose top stands above `z`.
fn standing(stock: &Stock, region: &Region, z: f64) -> Vec<Point2> {
    let cols = stock.columns().unwrap();
    let mut out = Vec::new();
    for iy in 0..cols.ny() {
        for ix in 0..cols.nx() {
            let c = cols.center(ix, iy);
            if region.contains(c) && cols.top_at(ix, iy) > z + PRIOR_Z_TOL {
                out.push(c);
            }
        }
    }
    out
}

/// The earlier operations of each scenario, simulated on a fresh block.
fn scenarios() -> Vec<(&'static str, Region, Stock)> {
    let region = pocket_region();
    let mut out = Vec::new();
    // A 12 mm flat end mill to the full depth.
    let mut s = block();
    cut(&mut s, &pocket(&region, &pp(12.0, BOTTOM, 0.0)).unwrap(), 12.0, None);
    out.push(("flat 12 mm", region.clone(), s));
    // The same with 0.5 mm left on the walls.
    let mut s = block();
    cut(&mut s, &pocket(&region, &pp(12.0, BOTTOM, 0.5)).unwrap(), 12.0, None);
    out.push(("wall allowance", region.clone(), s));
    // A 12 mm bull-nose tool, corner radius 2 mm: its fillet ring stands.
    let mut s = block();
    cut(&mut s, &pocket(&region, &pp(12.0, BOTTOM, 0.0)).unwrap(), 12.0, Some(2.0));
    out.push(("bull nose", region.clone(), s));
    // Mixed tools and a partial depth: 12 mm to Z-2 everywhere, then
    // 8 mm to Z-4 over two disconnected patches.
    let mut s = block();
    cut(&mut s, &pocket(&region, &pp(12.0, -2.0, 0.0)).unwrap(), 12.0, None);
    for patch in [rect(6.0, 6.0, 20.0, 34.0), rect(30.0, 6.0, 44.0, 20.0)] {
        cut(&mut s, &pocket(&patch, &pp(8.0, BOTTOM, 0.0)).unwrap(), 8.0, None);
    }
    out.push(("mixed tools, partial depth, disconnected cuts", region.clone(), s));
    // An island in the pocket.
    let island = region.difference(&rect(20.0, 15.0, 30.0, 25.0)).unwrap();
    let mut s = block();
    cut(&mut s, &pocket(&island, &pp(12.0, BOTTOM, 0.0)).unwrap(), 12.0, None);
    out.push(("island", island, s));
    out
}

#[test]
fn the_rest_region_holds_every_column_the_simulated_earlier_operations_left() {
    let zs = cut_levels(TOP, BOTTOM, 2.0).unwrap();
    for (name, region, stock) in scenarios() {
        let cols = stock.columns().unwrap();
        let proved = rest_stock_from_columns(&cols, &region, &zs).unwrap();
        let plan = rest_plan_stock(&region, TOOL_D, 0.0, 0.0, &proved, &zs).unwrap();
        // The independent reading: a column that stands above any cut
        // depth (so above the deepest) is rest material.
        let left = standing(&stock, &region, BOTTOM);
        assert!(!left.is_empty(), "{name}: the earlier operations leave material for the rest pass");
        let missed: Vec<_> = left.iter().filter(|c| !plan.rest.contains(**c)).collect();
        assert!(missed.is_empty(), "{name}: {} standing column(s) are not in the rest area, first {:?}", missed.len(), missed[0]);
        // And the reading is not the trivial one: cleared floor is not rest.
        let (rest, whole) = (plan.rest.area().abs(), region.area().abs());
        assert!(rest < whole * 0.75, "{name}: {rest:.1} of {whole:.1} mm2 reads as rest");
    }
}

#[test]
fn the_rest_path_cuts_what_was_left_and_keeps_the_walls() {
    for (name, region, mut stock) in scenarios() {
        let path = {
            let cols = stock.columns().unwrap();
            rest_pocket_previous(&region, &pp(TOOL_D, BOTTOM, 0.0), &previous_ops(), Some(&cols))
                .unwrap_or_else(|e| panic!("{name}: {e}"))
        };
        assert!(path.notes.iter().any(|n| n.contains("reads the stock that the previous operations leave")), "{name}: {:?}", path.notes);
        cut(&mut stock, &path, TOOL_D, None);
        // The floor is at depth wherever the 4 mm tool reaches (the
        // corners are 3 mm, so everywhere a cell inside the wall).
        let inner = region.offset(-2.0 * CELL).unwrap();
        let left = standing(&stock, &inner, BOTTOM);
        assert!(left.is_empty(), "{name}: {} column(s) still stand after the rest pass, first {:?}", left.len(), left.first());
        // No column outside the pocket was cut.
        let cols = stock.columns().unwrap();
        let outer = region.offset(2.0 * CELL).unwrap();
        for iy in 0..cols.ny() {
            for ix in 0..cols.nx() {
                let c = cols.center(ix, iy);
                assert!(outer.contains(c) || cols.top_at(ix, iy) >= TOP - PRIOR_Z_TOL, "{name}: the wall at {c:?} was cut");
            }
        }
    }
}

#[test]
fn a_shallow_earlier_pass_leaves_the_whole_pocket_for_the_rest_pass() {
    let region = pocket_region();
    let mut stock = block();
    cut(&mut stock, &pocket(&region, &pp(12.0, -2.0, 0.0)).unwrap(), 12.0, None);
    let path = {
        let cols = stock.columns().unwrap();
        rest_pocket_previous(&region, &pp(TOOL_D, BOTTOM, 0.0), &previous_ops(), Some(&cols)).unwrap()
    };
    assert!(path.notes.iter().any(|n| n.contains("the whole pocket is cut")), "{:?}", path.notes);
    cut(&mut stock, &path, TOOL_D, None);
    assert!(standing(&stock, &region.offset(-2.0 * CELL).unwrap(), BOTTOM).is_empty());
}

#[test]
fn no_earlier_cut_is_the_whole_pocket_and_a_cleared_pocket_is_refused_by_name() {
    let region = pocket_region();
    let whole = rest_pocket_previous(&region, &pp(TOOL_D, BOTTOM, 0.0), &previous_ops(), None).unwrap();
    assert!(whole.notes.iter().any(|n| n.contains("no earlier operation cut")), "{:?}", whole.notes);
    let plain = pocket(&region, &pp(TOOL_D, BOTTOM, 0.0)).unwrap();
    assert_eq!(whole.moves.len(), plain.moves.len(), "the whole pocket is the plain pocket's passes");
    // Everything at depth already: nothing to cut.
    let mut cleared = Stock::Column(Dexel::block(0.0, 0.0, 50.0, 40.0, TOP, CELL).unwrap());
    cleared.set_tops(|_, _, _| BOTTOM).unwrap();
    let cols = cleared.columns().unwrap();
    match rest_pocket_previous(&region, &pp(TOOL_D, BOTTOM, 0.0), &previous_ops(), Some(&cols)) {
        Err(GenError::Empty(why)) => assert!(why.contains("no rest material") && why.contains("Z-4.000"), "{why}"),
        other => panic!("expected the no-rest refusal, got {:?}", other.map(|p| p.moves.len())),
    }
}

#[test]
fn the_two_rest_modes_never_use_each_others_planner() {
    let region = pocket_region();
    let stock = block();
    let cols = stock.columns().unwrap();
    // Previous-operations mode is never planned from the entered tool,
    // even when the entered tool is a legal one.
    let typed = RestParams { previous_tool_diameter: 12.0, ..previous_ops() };
    let why = rest_pocket(&region, &pp(TOOL_D, BOTTOM, 0.0), &typed).unwrap_err().to_string();
    assert!(why.contains(REST_NEEDS_STOCK), "{why}");
    // Entered-tool mode is not planned from the stock.
    let entered = RestParams { source: RestSource::EnteredTool, ..typed };
    let why = rest_pocket_previous(&region, &pp(TOOL_D, BOTTOM, 0.0), &entered, Some(&cols)).unwrap_err().to_string();
    assert!(why.contains("reads the entered previous tool"), "{why}");
    // The previous tool is not read in previous-operations mode, the
    // allowance is.
    assert!(previous_ops().validate(TOOL_D).is_ok());
    let why = RestParams { allowance: -0.1, ..previous_ops() }.validate(TOOL_D).unwrap_err().to_string();
    assert!(why.contains("rest allowance"), "{why}");
    // Adaptive stays refused by name.
    let adaptive = PocketParams { strategy: PocketStrategy::Adaptive, ..pp(TOOL_D, BOTTOM, 0.0) };
    let why = rest_pocket_previous(&region, &adaptive, &previous_ops(), Some(&cols)).unwrap_err().to_string();
    assert!(why.contains("Adaptive"), "{why}");
}

#[test]
fn the_allowance_only_grows_the_rest_area() {
    let (_, region, stock) = scenarios().swap_remove(0);
    let cols = stock.columns().unwrap();
    let zs = cut_levels(TOP, BOTTOM, 2.0).unwrap();
    let proved = rest_stock_from_columns(&cols, &region, &zs).unwrap();
    let tight = rest_plan_stock(&region, TOOL_D, 0.0, 0.0, &proved, &zs).unwrap();
    let grown = rest_plan_stock(&region, TOOL_D, 0.0, 0.5, &proved, &zs).unwrap();
    assert!(grown.rest.area().abs() > tight.rest.area().abs());
    assert!(tight.rest.difference(&grown.rest).unwrap().area().abs() < 1e-6, "the allowance removed rest material");
}
