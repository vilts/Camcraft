//! Ray picking: nearest model along the ray, misses and degenerate rays
//! return None - never a panic. Includes rigid (rotated) placements.
//!
//! Gate map (M4.4): covers app::commands, app::geometry, app::schema, geometry::geometry::stock.

use camcraft_app::commands::{Command, ImportModel};
use camcraft_app::geometry::{cube, Mesh, Point3, quat_from_yaw};
use camcraft_app::schema::Location;
use camcraft_app::{model, pick, world::new_world};

fn cube_at(origin: Point3) -> Mesh {
    cube(origin, Point3::new(origin.x + 1.0, origin.y + 1.0, origin.z + 1.0))
}

#[test]
fn ray_hits_the_named_cube() {
    let world = new_world();
    ImportModel::from_mesh("a", cube_at(Point3::ZERO)).apply(&world);
    let found = pick::first_hit(&world, [0.5, 0.5, 10.0], [0.0, 0.0, -1.0]);
    assert_eq!(found.as_deref(), Some("a"));
}

#[test]
fn nearest_hit_wins() {
    let world = new_world();
    ImportModel::from_mesh("near", cube_at(Point3::ZERO)).apply(&world);
    ImportModel::from_mesh("far", cube_at(Point3::new(0.0, 0.0, -20.0))).apply(&world);
    let found = pick::first_hit(&world, [0.5, 0.5, 10.0], [0.0, 0.0, -1.0]);
    assert_eq!(found.as_deref(), Some("near"));
}

#[test]
fn miss_is_none() {
    let world = new_world();
    ImportModel::from_mesh("a", cube_at(Point3::ZERO)).apply(&world);
    let found = pick::first_hit(&world, [50.0, 50.0, 10.0], [0.0, 0.0, -1.0]);
    assert_eq!(found, None);
}

#[test]
fn non_finite_ray_is_none_not_crash() {
    let world = new_world();
    ImportModel::from_mesh("a", cube_at(Point3::ZERO)).apply(&world);
    let nan = f64::NAN;
    assert_eq!(pick::first_hit(&world, [nan, 0.0, 0.0], [0.0, 0.0, -1.0]), None);
    assert_eq!(pick::first_hit(&world, [0.0, 0.0, 0.0], [f64::INFINITY, 0.0, 0.0]), None);
}

#[test]
fn rotated_cube_picks_where_it_actually_is() {
    // Unit cube at origin, yawed 90 deg about Z, translated to x=5: it then
    // spans x in [4,5], y in [0,1]. The pre-rotation footprint (x in [5,6])
    // must miss.
    let world = new_world();
    ImportModel::from_mesh("a", cube_at(Point3::ZERO)).apply(&world);
    let e = model::find_by_name(&world, "a").expect("entity");
    model::set_transform(
        &world,
        &e,
        Location {
            pos: Point3::new(5.0, 0.0, 0.0),
            rot: quat_from_yaw(std::f64::consts::FRAC_PI_2),
        },
    );
    let found = pick::first_hit(&world, [4.5, 0.5, 10.0], [0.0, 0.0, -1.0]);
    assert_eq!(found.as_deref(), Some("a"));
    let miss = pick::first_hit(&world, [5.5, 0.5, 10.0], [0.0, 0.0, -1.0]);
    assert_eq!(miss, None, "rotation must move the geometry, not the ray");
}

#[test]
fn empty_world_picks_nothing() {
    let world = new_world();
    assert_eq!(
        pick::first_hit(&world, [0.0, 0.0, 5.0], [0.0, 0.0, -1.0]),
        None
    );
    assert_eq!(model::named_entities(&world).len(), 0);
}

#[test]
fn first_hit_f_reports_facet_and_world_point() {
    let world = new_world();
    ImportModel::from_mesh("a", cube_at(Point3::ZERO)).apply(&world);
    let e = model::find_by_name(&world, "a").expect("entity");
    let n_facets = model::facets_of(&world, &e).map_or(0, |v| v.len());
    let hit = pick::first_hit_f(&world, [0.5, 0.5, 10.0], [0.0, 0.0, -1.0], |_| false);
    let Some((name, facet, xyz)) = hit else {
        panic!("the cube top is on the ray");
    };
    assert_eq!(name, "a");
    assert!(facet < n_facets, "the index must be into this entity's facets");
    let near = |v: f64, w: f64| (v - w).abs() < 1e-9;
    assert!(
        near(xyz[0], 0.5) && near(xyz[1], 0.5) && near(xyz[2], 1.0),
        "world hit point on the top face, got {xyz:?}"
    );
    // The wrapper agrees about who was hit.
    assert_eq!(pick::first_hit(&world, [0.5, 0.5, 10.0], [0.0, 0.0, -1.0]).as_deref(), Some("a"));
}

#[test]
fn first_hit_f_skip_lets_the_ray_through() {
    let world = new_world();
    ImportModel::from_mesh("near", cube_at(Point3::ZERO)).apply(&world);
    ImportModel::from_mesh("far", cube_at(Point3::new(0.0, 0.0, -20.0))).apply(&world);
    // Skipping the near cube: same ray, now the far cube's top (z = -19).
    let hit = pick::first_hit_f(&world, [0.5, 0.5, 10.0], [0.0, 0.0, -1.0], |n| n == "near");
    let Some((name, _, xyz)) = hit else {
        panic!("far cube is on the ray");
    };
    assert_eq!(name, "far");
    assert!((xyz[2] - -19.0).abs() < 1e-9);
    // Skipping everything is a miss, not a panic.
    assert_eq!(
        pick::first_hit_f(&world, [0.5, 0.5, 10.0], [0.0, 0.0, -1.0], |_| true),
        None
    );
}

#[test]
fn models_win_over_stock_and_stock_is_pickable() {
    use camcraft_app::commands::SetStock;
    use camcraft_app::stock::StockShape;

    let world = new_world();
    ImportModel::from_mesh("part", cube_at(Point3::ZERO)).apply(&world);
    SetStock::new(
        &world,
        StockShape::Box {
            dx: 10.0,
            dy: 10.0,
            dz: 10.0,
        },
        Location::default(),
    )
    .expect("sane box")
    .apply(&world);

    // Through the stock, at the model: the model wins at any depth.
    assert_eq!(
        pick::first_hit(&world, [0.5, 0.5, 20.0], [0.0, 0.0, -1.0]).as_deref(),
        Some("part")
    );
    // Stock with empty space behind it is still pickable.
    assert_eq!(
        pick::first_hit(&world, [4.0, 4.0, 20.0], [0.0, 0.0, -1.0]).as_deref(),
        Some("Stock")
    );
}
