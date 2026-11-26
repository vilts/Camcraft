//! The guards of `rest_prior_adaptive` (#1446), each switched off alone:
//! its pin holds with every guard on and fails without that guard.

use super::*;
use crate::entry::Entry3D;
use crate::geo2d::{Loop, Point2};
use crate::load::DEFAULT_OPTIMAL_LOAD;
use crate::machining2d::pocket::{pocket, PocketStrategy};
use crate::stock_sim::{Dexel, SimOp, Stock};

const CELL: f64 = 0.1;
const TOP: f64 = 0.0;
const BOTTOM: f64 = -4.0;

/// A 40 x 30 pocket with 3 mm corners, inside a 50 x 40 block.
fn region() -> Region {
    let r = Region::new(vec![Loop::new(vec![
        Point2::new(5.0, 5.0),
        Point2::new(45.0, 5.0),
        Point2::new(45.0, 35.0),
        Point2::new(5.0, 35.0),
    ])
    .unwrap()])
    .unwrap();
    r.offset(-3.0).unwrap().offset(3.0).unwrap()
}

fn pp(d: f64, strategy: PocketStrategy) -> PocketParams {
    PocketParams {
        blend_radius: None,
        links: crate::links::Links::PLANAR,
        tool_diameter: d,
        stepover: d * 0.4,
        stock_to_leave: 0.0,
        clearance_z: 5.0,
        retract_z: Some(1.0),
        top_z: TOP,
        bottom_z: BOTTOM,
        stepdown: 2.0,
        feed: 400.0,
        plunge_feed: 80.0,
        climb: true,
        strategy,
        entry: Entry3D::Plunge,
        helix_radius: 1.0,
        optimal_load: DEFAULT_OPTIMAL_LOAD,
        blend_turns: false,
    }
}

/// The block after a 12 mm tool roughed the pocket to the bottom.
fn roughed() -> Stock {
    let mut s = Stock::Column(Dexel::block(0.0, 0.0, 50.0, 40.0, TOP, CELL).unwrap());
    let rough = pocket(&region(), &pp(12.0, PocketStrategy::Waterline)).unwrap();
    s.simulate(&[SimOp { moves: &rough.moves, radius: 6.0, ball: false, cone: None, corner: None }]).unwrap();
    s
}

fn from_ops(allowance: f64) -> RestParams {
    RestParams {
        source: RestSource::PreviousOperations,
        previous_tool_diameter: 0.0,
        previous_corner_radius: 0.0,
        previous_stock_to_leave: 0.0,
        allowance,
    }
}

fn said(r: Result<Toolpath, GenError>) -> String {
    r.map_or_else(|e| e.to_string(), |p| format!("a path of {} move(s)", p.moves.len()))
}

/// The source pin: an entered previous tool is refused by name.
fn source_pin(g: Guards) -> bool {
    let s = roughed();
    let cols = s.columns().unwrap();
    let entered = RestParams { source: RestSource::EnteredTool, previous_tool_diameter: 12.0, ..from_ops(0.0) };
    said(guarded(&region(), &pp(4.0, PocketStrategy::Adaptive), &entered, Some(&cols), g)).contains(REST_ADAPTIVE_ENTERED)
}

/// The stock pin: no stock of the previous operations is refused by name.
fn stock_pin(g: Guards) -> bool {
    said(guarded(&region(), &pp(4.0, PocketStrategy::Adaptive), &from_ops(0.0), None, g)).contains(REST_ADAPTIVE_NO_STOCK)
}

/// The allowance pin: the allowance changes the plan (more material stands).
fn allowance_pin(g: Guards) -> bool {
    let s = roughed();
    let cols = s.columns().unwrap();
    let p = pp(4.0, PocketStrategy::Adaptive);
    let tight = guarded(&region(), &p, &from_ops(0.0), Some(&cols), Guards::ALL).unwrap();
    let wide = guarded(&region(), &p, &from_ops(0.6), Some(&cols), g).unwrap();
    tight.moves != wide.moves
}

#[test]
fn each_guard_is_pinned_and_its_removal_fails_the_pin() {
    assert!(source_pin(Guards::ALL) && stock_pin(Guards::ALL) && allowance_pin(Guards::ALL));
    assert!(!source_pin(Guards { source: false, ..Guards::ALL }), "the source guard is not pinned");
    assert!(!stock_pin(Guards { stock: false, ..Guards::ALL }), "the stock guard is not pinned");
    assert!(!allowance_pin(Guards { allowance: false, ..Guards::ALL }), "the allowance guard is not pinned");
}

#[test]
fn the_allowance_never_makes_the_standing_material_smaller() {
    let s = roughed();
    let cols = s.columns().unwrap();
    let zs = rest_levels(&pp(4.0, PocketStrategy::Adaptive)).unwrap();
    let stock = rest_stock_from_columns(&cols, &region(), &zs).unwrap().with_film(reading_film(&cols)).unwrap();
    let wide = grown(&stock, &region(), 0.6).unwrap();
    assert_eq!(wide.film(), stock.film());
    for z in zs {
        let (a, b) = (stock.standing_at(z).unwrap(), wide.standing_at(z).unwrap());
        assert!(a.difference(&b).unwrap().area().abs() < 1e-6, "Z{z}: the allowance removed material");
        assert!(b.difference(&region()).unwrap().area().abs() < 1e-6, "Z{z}: material outside the pocket");
    }
    assert!(wide.standing_at(BOTTOM).unwrap().area().abs() > stock.standing_at(BOTTOM).unwrap().area().abs());
    assert_eq!(grown(&stock, &region(), 0.0).unwrap(), stock);
}

#[test]
fn bad_rest_values_are_refused_by_name() {
    let s = roughed();
    let cols = s.columns().unwrap();
    let p = pp(4.0, PocketStrategy::Adaptive);
    for bad in [-0.1, f64::NAN] {
        let why = said(rest_adaptive_previous(&region(), &p, &from_ops(bad), Some(&cols)));
        assert!(why.contains("rest allowance") || why.contains("must be finite"), "{why}");
    }
}
