//! The chain boundary interface works without application or ECS types.
use camcraft_geometry::boundary_chains::{group_runs, sides_of_chains, touching_refusal};
use camcraft_geometry::geo2d::{Loop, Point2, Region};

fn p(x: f64, y: f64) -> Point2 {
    Point2::new(x, y)
}

#[test]
fn boundary_chains_are_a_geometry_crate_interface() {
    let chains = group_runs(vec![
        vec![p(-10.0, 10.0), p(0.0, 10.0)],
        vec![p(10.0, 10.0), p(0.0, 10.0)],
        vec![p(-10.0, -10.0), p(10.0, -10.0)],
    ]);
    assert_eq!(chains.len(), 2);
    assert_eq!(chains[0], vec![p(-10.0, 10.0), p(0.0, 10.0), p(10.0, 10.0)]);
    assert!(touching_refusal(&chains).is_none());
    let square = |h: f64| {
        Region::new(vec![Loop::new(vec![p(-h, -h), p(h, -h), p(h, h), p(-h, h)]).expect("loop")])
            .expect("region")
    };
    let (between, beyond, _) = sides_of_chains(&chains, &square(30.0), &square(10.0)).expect("strip");
    assert!(between.contains(p(0.0, 0.0)));
    assert!(!between.contains(p(0.0, 20.0)));
    assert!(beyond.contains(p(0.0, 20.0)));
}

#[test]
fn blind_slot_is_a_geometry_crate_interface() {
    let stock = Region::new(vec![
        Loop::new(vec![p(-35.0, -35.0), p(35.0, -35.0), p(35.0, 35.0), p(-35.0, 35.0)])
            .expect("stock loop"),
    ])
    .expect("stock region");
    let wall = [p(-8.0, -20.0), p(-8.0, 8.0), p(8.0, 8.0), p(8.0, -20.0)];
    let slot = camcraft_geometry::boundary_chains::open_slot::region(&wall, &stock, 0.0)
        .expect("blind slot");
    assert!(slot.contains(p(0.0, 0.0)));
    assert!(slot.contains(p(0.0, -30.0)));
    assert!(!slot.contains(p(0.0, 20.0)));
    assert!(!slot.contains(p(20.0, 0.0)));
}
