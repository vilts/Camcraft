//! Unit tests of the open-edge band law (`open_band`, #1735).

use super::*;

fn rect(x0: f64, y0: f64, x1: f64, y1: f64) -> Region {
    let l = Loop::new(vec![Point2::new(x0, y0), Point2::new(x1, y0), Point2::new(x1, y1), Point2::new(x0, y1)]);
    Region::new(vec![l.expect("loop")]).expect("region")
}

fn seg(x0: f64, y0: f64, x1: f64, y1: f64) -> Vec<Point2> {
    vec![Point2::new(x0, y0), Point2::new(x1, y1)]
}

fn why(r: Result<Region, GenError>) -> String {
    r.expect_err("refused").to_string()
}

#[test]
fn the_band_is_the_strip_between_the_walls_closed_by_the_stock() {
    let stock = rect(-35.0, -35.0, 35.0, 35.0);
    // Walls picked in opposite directions, shorter than the stock.
    let (a, b) = (seg(-8.0, -20.0, -8.0, 20.0), seg(8.0, 20.0, 8.0, -20.0));
    let r = band(&a, &b, &stock, 0.0).expect("band");
    assert!((r.area() - 16.0 * 70.0).abs() < 1e-6, "{}", r.area());
    // The extension moves the open ends out, never the walls.
    let e = band(&a, &b, &stock, 5.0).expect("extended");
    assert!((e.area() - 16.0 * 80.0).abs() < 1e-6, "{}", e.area());
    let (lo, hi) = e.bounds().expect("bounds");
    assert!((lo.x + 8.0).abs() < 1e-9 && (hi.x - 8.0).abs() < 1e-9 && (hi.y - 40.0).abs() < 1e-9 && (lo.y + 40.0).abs() < 1e-9);
}

#[test]
fn unclear_walls_are_refused_by_name() {
    let stock = rect(-35.0, -35.0, 35.0, 35.0);
    let a = seg(-8.0, -20.0, -8.0, 20.0);
    // At a right angle.
    assert!(why(band(&a, &seg(0.0, 10.0, 20.0, 10.0), &stock, 0.0)).contains("do not face each other (they stand at 90 degrees"));
    // Parallel, but one after the other.
    assert!(why(band(&a, &seg(8.0, 22.0, 8.0, 30.0), &stock, 0.0)).contains("neither runs alongside the other"));
    // On one line.
    assert!(why(band(&a, &seg(-8.0, -10.0, -8.0, 30.0), &stock, 0.0)).contains("cross or touch once run on"));
    // Converging to a point inside the stock.
    assert!(why(band(&a, &seg(8.0, -20.0, -6.0, 20.0), &stock, 0.0)).contains("run together inside the stock"));
    // Stock elsewhere.
    assert!(why(band(&a, &seg(8.0, -20.0, 8.0, 20.0), &rect(50.0, 50.0, 60.0, 60.0), 0.0)).contains("the stock does not cover the space"));
    // A U-shaped stock closes the band twice.
    let u = rect(-35.0, -35.0, 35.0, 35.0).difference(&rect(-36.0, 10.0, 0.0, 20.0)).expect("u");
    let (a, b) = (seg(-20.0, -30.0, -20.0, 30.0), seg(-10.0, -30.0, -10.0, 30.0));
    assert!(why(band(&a, &b, &u, 0.0)).contains("more than one way to close it"));
}

#[test]
fn runs_that_share_an_end_weld_into_one_wall() {
    let w = weld(vec![seg(0.0, 0.0, 0.0, 5.0), seg(9.0, 0.0, 9.0, 9.0), seg(0.0, 9.0, 0.0, 5.0)]).expect("weld");
    assert_eq!(w.len(), 2);
    assert_eq!(w.iter().map(Vec::len).max(), Some(3));
    let fork = weld(vec![seg(0.0, 0.0, 0.0, 5.0), seg(0.0, 5.0, 0.0, 9.0), seg(0.0, 5.0, 4.0, 5.0)]);
    assert!(fork.expect_err("fork").to_string().contains("three picked open edges meet"));
}
