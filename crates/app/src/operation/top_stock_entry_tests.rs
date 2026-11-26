//! #2129 review pin: an air entry in one region cannot hide stock in the next.
//! Provenance: synthetic stepped-stock fixture and the real 2D generators.

use super::*;
use crate::cl::MoveKind;
use crate::geometry::{Point3, cube};
use crate::links::Links;
use crate::machining2d::{
    ContourParams, Lead, PocketParams, PocketStrategy, Side, contour, pocket,
};
use crate::machining3d::Entry3D;

fn regions() -> Region {
    let ring = |x| {
        Loop::new(vec![
            Point2::new(x, 0.0),
            Point2::new(x + 40.0, 0.0),
            Point2::new(x + 40.0, 30.0),
            Point2::new(x, 30.0),
        ])
        .expect("rectangle")
    };
    Region::new(vec![ring(0.0), ring(60.0)]).expect("separate regions")
}

fn stepped_stock() -> Vec<Triangle> {
    let mut stock = cube(Point3::ZERO, Point3::new(40.0, 30.0, 5.0))
        .facets
        .to_vec();
    stock.extend(
        cube(Point3::new(60.0, 0.0, 0.0), Point3::new(100.0, 30.0, 10.0))
            .facets
            .iter()
            .copied(),
    );
    stock
}

fn pocket_path(region: &Region, material_top: Option<f64>) -> Toolpath {
    let mut links = Links::PLANAR;
    // #2004: the generation lays the setup's stock top (the tall block,
    // Z10) over every operation, so the level at Top Z stays a level.
    links.approach.stock_top = Some(10.0);
    links.approach.material_top = material_top;
    pocket(
        region,
        &PocketParams {
            tool_diameter: 6.0,
            stepover: 3.0,
            stock_to_leave: 0.0,
            clearance_z: 20.0,
            top_z: 6.0,
            bottom_z: 0.0,
            stepdown: 3.0,
            feed: 800.0,
            plunge_feed: 200.0,
            climb: true,
            strategy: PocketStrategy::Zigzag,
            retract_z: Some(15.0),
            entry: Entry3D::Plunge,
            helix_radius: 1.0,
            optimal_load: crate::load::DEFAULT_OPTIMAL_LOAD,
            blend_turns: false,
            blend_radius: None,
            links,
        },
    )
    .expect("pocket")
}

fn contour_path(region: &Region, material_top: Option<f64>) -> Toolpath {
    let mut links = Links::PLANAR;
    // #2004: the generation lays the setup's stock top (the tall block,
    // Z10) over every operation, so the level at Top Z stays a level.
    links.approach.stock_top = Some(10.0);
    links.approach.material_top = material_top;
    contour(
        region,
        &ContourParams {
            tool_diameter: 6.0,
            stock_to_leave: 0.0,
            clearance_z: 20.0,
            retract_z: 15.0,
            top_z: 6.0,
            bottom_z: 0.0,
            stepdown: 3.0,
            feed: 800.0,
            plunge_feed: 200.0,
            climb: true,
            side: Side::Inside,
            lead: Lead::None,
            tolerance: 0.01,
            tab_count: 0,
            tab_width: 0.0,
            tab_height: 0.0,
            entry: Entry3D::Plunge,
            helix_radius: 1.0,
            blend_turns: false, blend_radius: None,
            links,
            picks: Default::default(),
        },
    )
    .expect("contour")
}

fn check_later_region(generate: fn(&Region, Option<f64>) -> Toolpath) {
    let region = regions();
    let stock = stepped_stock();
    let m = carried(Some(6.0), &region, &stock, None).expect("stock above Top");
    assert_eq!(m, 10.0);
    let plain = generate(&region, None);
    let air_feed = plain
        .moves
        .iter()
        .position(|mv| {
            mv.kind != MoveKind::Rapid && mv.to.x < 40.0 && (mv.to.z - 6.0).abs() < Z_TOL
        })
        .expect("first region feeds to Top in air");
    let dive = plain
        .moves
        .iter()
        .position(|mv| {
            mv.kind == MoveKind::Rapid && mv.to.x > 60.0 && mv.to.z < 10.0 && mv.to.z < mv.from.z
        })
        .expect("plain later entry rapids inside tall stock");
    assert!(
        air_feed < dive,
        "the clear region must precede the unsafe entry"
    );
    assert!(
        dives_into(&plain, m, &stock, None, 3.0),
        "an earlier air feed must not acquit the later region"
    );
    let carried_path = generate(&region, Some(m));
    let entry = carried_path
        .moves
        .iter()
        .find(|mv| mv.kind == MoveKind::Rapid && mv.to.x > 60.0 && mv.to.z < mv.from.z)
        .expect("later region entry");
    assert!(
        entry.to.z >= 12.0 - Z_TOL,
        "later entry must start over the tall stock: {entry:?}"
    );

    // Prior cuts still count at each tool disc. Clearing only the first
    // region cannot acquit the second; clearing both removes the dive.
    let mut prior = Dexel::block(0.0, 0.0, 100.0, 30.0, 10.0, 0.5).expect("prior grid");
    for iy in 0..prior.ny {
        for ix in 0..prior.nx {
            if prior.center(ix, iy).x < 50.0 {
                prior.lower(ix + iy * prior.nx, 0.0);
            }
        }
    }
    assert!(dives_into(&plain, m, &stock, Some(&prior), 3.0));
    for k in 0..prior.nx * prior.ny {
        prior.lower(k, 0.0);
    }
    assert!(
        !dives_into(&plain, m, &stock, Some(&prior), 3.0),
        "the later entry through previously removed material stays clear"
    );
}

#[test]
fn pocket_checks_later_region_after_air_feed_and_keeps_prior_credit() {
    check_later_region(pocket_path);
}

#[test]
fn contour_checks_later_region_after_air_feed_and_keeps_prior_credit() {
    check_later_region(contour_path);
}
