//! Provenance: analytic circle sections and our own cylinder fixtures (#2758).
//! Gate map (M4.4): covers camcraft_sim::stock_sim::laid_cylinder, camcraft_sim::stock_sim::kernel, camcraft_sim::stock_sim::carry, camcraft_sim::stock_sim::tiles.

use camcraft_sim::geo2d::{Loop, Point2, Region};
use camcraft_sim::geometry::Point3;
use camcraft_sim::stock_sim::{Body, Dexel, measure};

fn footprint() -> Region {
    Region::new(vec![Loop::new(vec![
        Point2::new(-200.0, -200.0), Point2::new(200.0, -200.0),
        Point2::new(200.0, 200.0), Point2::new(-200.0, 200.0),
    ]).expect("rectangle loop")]).expect("rectangle region")
}

#[test]
fn laid_cylinder_refuses_invalid_dimensions_by_name() {
    for bad in [0.0, -1.0, f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        for (diameter, height) in [(bad, 20.3), (10.3, bad)] {
            for along_x in [false, true] {
                let error = Dexel::laid_cylinder(diameter, height, along_x, 0.7)
                    .expect_err("invalid cylinder dimensions");
                assert!(error.to_string().contains(
                    "laid cylinder stock needs finite positive diameter, height"
                ), "{error}");
            }
        }
    }
}

#[test]
fn laid_cylinder_padding_and_translation_keep_exact_material_sections() {
    let (diameter, length) = (10.3_f64, 20.3_f64);
    let radius = diameter * 0.5;
    let origin = Point3::new(13.0, -7.0, 11.0);
    for along_x in [false, true] {
        // The second cell size pads the grid beyond both the end and rim.
        for cell in [0.7, 0.9] {
            let mut dex = Dexel::laid_cylinder(diameter, length, along_x, cell)
                .expect("horizontal cylinder");
            let original = dex.clone();
            dex.translate_origin(origin.x, origin.y, origin.z);
            let body = Body::on_floor(dex.clone(), -1000.0);
            let mut volume = 0.0;
            let mut air = 0;
            let mut material = 0;
            for iy in 0..dex.ny {
                for ix in 0..dex.nx {
                    let k = ix + iy * dex.nx;
                    let q = dex.center(ix, iy);
                    let (long, cross) = if along_x {
                        (q.x - origin.x, q.y - origin.y)
                    } else {
                        (q.y - origin.y, q.x - origin.x)
                    };
                    let top = dex.top_at(k);
                    if (0.0..=length).contains(&long) && cross.abs() < radius {
                        let half = (radius * radius - cross * cross).sqrt();
                        assert!((top - origin.z - half).abs() < 1e-9, "top at {q:?}");
                        assert!((body.floor_at(k) - origin.z + half).abs() < 1e-9, "floor at {q:?}");
                        assert!((top - original.top_at(k) - origin.z).abs() < 1e-9);
                        volume += 2.0 * half * cell * cell;
                        material += 1;
                    } else {
                        assert_eq!(top, f64::NEG_INFINITY, "padding at {q:?} is air");
                        assert_eq!(body.floor_at(k), f64::INFINITY);
                        air += 1;
                    }
                }
            }
            assert!(material > 0);
            if cell == 0.9 { assert!(air > 0, "the fixture exercises padded air columns"); }
            assert!((body.volume() - volume).abs() < 1e-7);
            assert!((Body::on_floor(original.clone(), -1000.0).volume() - volume).abs() < 1e-7);
            let part = vec![f64::NEG_INFINITY; dex.cells()];
            let region = footprint();
            assert!(measure(&dex, &part, &region, 0.01).removed_volume.abs() < 1e-9);
            let below = Point3::new(origin.x, origin.y, origin.z - 100.0);
            dex.cut_chord(below, below, 200.0);
            let removed = measure(&dex, &part, &region, 0.01).removed_volume;
            assert!((removed - volume).abs() < 1e-7, "removed {removed}, material {volume}");
            assert_eq!(Body::on_floor(dex.clone(), -1000.0).volume(), 0.0);
            // Keep the tool depth for gouge diagnostics after a through-cut.
            assert!(dex.tops().filter(|z| z.is_finite()).all(|z| z < origin.z - radius));
            assert!((Body::on_floor(original, -1000.0).volume() - volume).abs() < 1e-7,
                "cutting the translated clone does not change the original stock");
        }
    }
}
