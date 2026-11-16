//! Issue #882: opening containment on narrow and nested geometry, with
//! polygon difference checking the whole area rather than boundary samples.
//! Gate map: covers motion::machining2d::adaptive (rings, simplify).
//! Provenance: original adversarial fixtures, no vendor material.

use super::{seg_dist, thin_loop, within};
use crate::geo2d::{Loop, Point2, Region};
use crate::machining2d::adaptive::rings::{SIMPLIFY, open};

fn polygon(points: &[(f64, f64)]) -> Loop {
    Loop::new(points.iter().map(|&(x, y)| Point2::new(x, y)).collect()).expect("fixture loop")
}

fn circle(cx: f64, r: f64) -> Loop {
    Loop::new((0..1440).map(|i| {
        let a = std::f64::consts::TAU * i as f64 / 1440.0;
        Point2::new(cx + r * a.cos(), r * a.sin())
    }).collect()).expect("fixture circle")
}

fn contained(name: &str, area: &Region, radii: &[f64]) {
    for &rho in radii {
        let opened = open(area, rho).expect("opening");
        let leaked = opened.difference(area).expect("whole-area difference");
        assert!(leaked.is_empty(), "{name}, R{rho}: opening adds {} mm2 outside its source", leaked.area());
        assert!(opened.area() <= area.area() + 1e-9, "{name}, R{rho}: area grew");
    }
}

#[test]
fn openings_never_add_material_across_acute_notches_nearby_loops_or_narrow_holes() {
    let acute = Region::new(vec![polygon(&[
        (0.0, 0.0), (20.0, 0.0), (20.0, 20.0), (10.0002, 20.0),
        (10.0, 1.0), (9.9998, 20.0), (0.0, 20.0),
    ])]).expect("acute notch");
    contained("acute notch", &acute, &[0.08, 0.6, 2.5]);
    let near_hole = Region::new(vec![circle(0.0, 10.0), circle(7.0, 2.9998)]).expect("narrow wall");
    contained("0.2 um wall by hole", &near_hole, &[0.08, 0.6, 2.5]);
    let neighbors = Region::new(vec![circle(-3.00005, 3.0), circle(3.00005, 3.0)]).expect("nearby components");
    contained("0.1 um gap", &neighbors, &[0.08, 0.6, 2.5]);
    let nested = Region::new(vec![circle(0.0, 10.0), circle(0.0, 4.0), circle(0.0, 1.0)]).expect("nested island");
    contained("hole and nested island", &nested, &[0.08, 0.6, 2.5]);
    let sliver = Region::new(vec![polygon(&[(0.0, 0.0), (5.0, 0.0), (5.0, 0.0001), (0.0, 0.0001)])]).expect("thin sliver");
    contained("0.1 um sliver", &sliver, &[0.00001, 0.08]);
}

#[test]
fn thinning_holds_its_bound_across_the_closing_seam_in_either_direction() {
    let base: Vec<_> = (0..720).map(|i| {
        let a = std::f64::consts::TAU * i as f64 / 720.0;
        let r = 5.0 + 0.1 * (5.0 * a).cos();
        Point2::new(r * a.cos(), r * a.sin())
    }).collect();
    for seam in [0, 1, 179, 359, 719] {
        for reverse in [false, true] {
            let mut points = base.clone();
            points.rotate_left(seam);
            if reverse { points.reverse(); }
            let light = thin_loop(&points, SIMPLIFY);
            assert!(light.len() >= 3 && light.len() < points.len(), "fixture actually thins");
            let off = points.iter().map(|&p| {
                (0..light.len()).map(|i| seg_dist(p, light[i], light[(i + 1) % light.len()])).fold(f64::INFINITY, f64::min)
            }).fold(0.0, f64::max);
            assert!(off <= SIMPLIFY + 1e-12, "seam {seam}, reverse {reverse}: {off} mm");
            let area = Region::new(vec![Loop::new(points).expect("seam loop")]).expect("seam area");
            contained("cyclic seam", &area, &[0.6]);
        }
    }
}

#[test]
fn empty_regions_and_invalid_thinning_tolerances_keep_the_original() {
    let empty = Region::default();
    assert!(within(&empty, SIMPLIFY).is_none());
    let square = Region::new(vec![polygon(&[(0.0, 0.0), (1.0, 0.0), (1.0, 1.0), (0.0, 1.0)])]).expect("square");
    for tol in [0.0, -1.0, f64::NAN, f64::INFINITY] {
        assert!(within(&square, tol).is_none(), "invalid tolerance {tol}");
    }
    contained("empty", &empty, &[0.08, 2.5]);
}
