//! Provenance: original diagnostic regressions from issue #1709 and simple box geometry.
//! Gate map (M4.4): covers camcraft_motion::machining3d.

use camcraft_motion::geo2d::{Loop, Point2, Region};
use camcraft_motion::geometry::{Point3, Triangle};
use camcraft_motion::machining2d::{GenError, PocketStrategy};
use camcraft_motion::machining3d::{Entry3D, Pocket3DParams, pocket3d, pocket3d_offset};

fn boundary() -> Region {
    Region::new(vec![Loop::new(vec![
        Point2::new(-10.0, -10.0),
        Point2::new(10.0, -10.0),
        Point2::new(10.0, 10.0),
        Point2::new(-10.0, 10.0),
    ])
    .unwrap()])
    .unwrap()
}

fn box_facets(min: [f64; 3], max: [f64; 3]) -> Vec<Triangle> {
    let v = |i: usize| {
        Point3::new(
            [min[0], max[0]][i & 1],
            [min[1], max[1]][(i >> 1) & 1],
            [min[2], max[2]][(i >> 2) & 1],
        )
    };
    let quad = |a, b, c, d| [Triangle::new(v(a), v(b), v(c)), Triangle::new(v(a), v(c), v(d))];
    let mut facets = Vec::new();
    facets.extend(quad(4, 5, 7, 6));
    facets.extend(quad(0, 2, 3, 1));
    facets.extend(quad(0, 1, 5, 4));
    facets.extend(quad(2, 6, 7, 3));
    facets.extend(quad(1, 3, 7, 5));
    facets.extend(quad(0, 4, 6, 2));
    facets
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

fn assert_refusal(facets: &[Triangle], p: &Pocket3DParams, expected: &str) {
    let region = boundary();
    for result in [pocket3d(facets, &region, p), pocket3d_offset(facets, &region, p)] {
        match result {
            Err(GenError::Empty(message)) => assert_eq!(message, expected),
            other => panic!("expected an empty-pocket refusal, got {other:?}"),
        }
    }
}

#[test]
fn filled_pocket_names_the_model() {
    let facets = box_facets([-20.0, -20.0, -20.0], [20.0, 20.0, 5.0]);
    assert_refusal(&facets, &params(), "nothing to cut: the model fills the pocket at every level");
}

#[test]
fn narrow_pocket_names_the_tool() {
    // The model leaves a 6 mm strip within the boundary at every plane.
    let facets = box_facets([-20.0, -20.0, -20.0], [4.0, 20.0, 5.0]);
    assert_refusal(
        &facets,
        &params(),
        "nothing to cut: the open area is too narrow for the 8 mm tool and 0 mm stock to leave at every level",
    );
}

#[test]
fn stock_allowance_names_the_clearance_needed() {
    // A 10 mm strip fits the tool alone, but not its 2 mm allowance on each side.
    let facets = box_facets([-20.0, -20.0, -20.0], [0.0, 20.0, 5.0]);
    let mut p = params();
    p.stock_to_leave = 2.0;
    assert_refusal(
        &facets,
        &p,
        "nothing to cut: the open area is too narrow for the 8 mm tool and 2 mm stock to leave at every level",
    );
}
