//! Provenance: analytic circle sections and our own placed-stock fixtures (#2758).
//! Gate map (M4.4): covers app::stock_follow::sim_frame, app::stock_sim::stock_grid.

use camcraft_app::commands::{CreateSetup, History, SetStock};
use camcraft_app::geo2d::{Loop, Point2, Region};
use camcraft_app::geometry::{Point3, quat_from_euler};
use camcraft_app::schema::Location;
use camcraft_app::stock::StockShape;
use camcraft_app::stock_sim::{Body, measure, stock_grid};
use camcraft_app::world::new_world;

#[test]
fn laid_cylinders_cut_only_their_real_material_along_x_and_y() {
    let quarter = std::f64::consts::FRAC_PI_2;
    for (roll, pitch, along_x, sign) in [
        (0.0, quarter, true, 1.0),
        (0.0, -quarter, true, -1.0),
        (-quarter, 0.0, false, 1.0),
        (quarter, 0.0, false, -1.0),
    ] {
        let world = new_world();
        let mut h = History::default();
        h.execute(&world, CreateSetup::new(&world, None, None).expect("setup"));
        let at = Point3::new(13.0, -7.0, 11.0);
        h.execute(&world, SetStock::new(
            &world,
            StockShape::Cylinder { diameter: 10.0, height: 20.0 },
            Location { pos: at, rot: quat_from_euler(roll, pitch, 0.0) },
        ).expect("laid stock"));
        let mut dex = stock_grid(&world, Some("Setup1"), 1.0).expect("laid cylinder is simulated");
        let footprint = Region::new(vec![Loop::new(vec![
            Point2::new(-100.0, -100.0), Point2::new(100.0, -100.0),
            Point2::new(100.0, 100.0), Point2::new(-100.0, 100.0),
        ]).expect("loop")]).expect("region");
        let part = vec![f64::NEG_INFINITY; dex.cells()];
        assert_eq!(measure(&dex, &part, &footprint, 0.01).removed_volume, 0.0);
        let mut expected_half = 0.0;
        for iy in 0..dex.ny {
            for ix in 0..dex.nx {
                let q = dex.center(ix, iy);
                let (long, cross) = if along_x { ((q.x - at.x) * sign, q.y - at.y) }
                    else { ((q.y - at.y) * sign, q.x - at.x) };
                let top = dex.top_at(ix + iy * dex.nx);
                if (0.0..=20.0).contains(&long) && cross.abs() < 5.0 {
                    let half = (25.0 - cross * cross).sqrt();
                    assert!((top - (at.z + half)).abs() < 1e-9, "top {top} at {q:?}");
                    expected_half += half * dex.cell * dex.cell;
                } else {
                    assert!(!top.is_finite(), "no stock outside the cylinder at {q:?}");
                }
            }
        }
        let full = 2.0 * expected_half;
        assert!((full - std::f64::consts::PI * 25.0 * 20.0).abs() / full < 0.01);
        let body = Body::on_floor(dex.clone(), at.z - 5.0);
        assert!((body.volume() - full).abs() < 1e-7, "the carried body has a rounded underside");
        let a = Point3::new(at.x, at.y, at.z);
        dex.cut_chord(a, a, 100.0);
        let half = measure(&dex, &part, &footprint, 0.01).removed_volume;
        assert!((half - expected_half).abs() < 1e-7, "half removal {half}, expected {expected_half}");
        assert!((Body::on_floor(dex.clone(), at.z - 5.0).volume() - expected_half).abs() < 1e-7);
        let below = Point3::new(at.x, at.y, at.z - 100.0);
        dex.cut_chord(below, below, 100.0);
        let removed = measure(&dex, &part, &footprint, 0.01).removed_volume;
        assert!((removed - full).abs() < 1e-7, "removal stops at the rounded underside: {removed}, expected {full}");
        assert_eq!(Body::on_floor(dex, at.z - 5.0).volume(), 0.0);
    }
}
