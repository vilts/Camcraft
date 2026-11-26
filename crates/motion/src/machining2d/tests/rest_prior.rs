//! The reading guards of `rest_prior` (#2033), each switched off alone:
//! its pin holds with every guard on and fails without that guard.

use super::*;
use crate::stock_sim::{Dexel, Stock};

const CELL: f64 = 0.25;
const TOP: f64 = 0.0;

fn rect(x0: f64, y0: f64, x1: f64, y1: f64) -> Region {
    Region::new(vec![
        Loop::new(vec![Point2::new(x0, y0), Point2::new(x1, y0), Point2::new(x1, y1), Point2::new(x0, y1)]).unwrap(),
    ])
    .unwrap()
}

/// A 40 x 30 block whose columns inside `x 8..32, y 8..22` are cut to
/// `floor`; `edit` then rewrites single tops.
fn stock(floor: f64, mut edit: impl FnMut(Point2, f64) -> f64) -> Stock {
    let mut s = Stock::Column(Dexel::block(0.0, 0.0, 40.0, 30.0, TOP, CELL).unwrap());
    s.set_tops(|_, c, cur| {
        let cut = if c.x > 8.0 && c.x < 32.0 && c.y > 8.0 && c.y < 22.0 { cur.min(floor) } else { cur };
        edit(c, cut)
    })
    .unwrap();
    s
}

fn pocket() -> Region {
    rect(5.0, 5.0, 35.0, 25.0)
}

/// The depth pin: an earlier cut to Z-2 proves nothing at Z-4, so the
/// whole pocket stands for a rest pass that cuts at both.
fn depth_pin(g: Guards) -> bool {
    let s = stock(-2.0, |_, z| z);
    let cols = s.columns().unwrap();
    let levels = [-2.0, -4.0];
    let st = stock_from(&cols, &pocket(), &levels, g).unwrap();
    let plan = plan_from(&pocket(), 4.0, 0.0, 0.0, &st, &levels, g).unwrap();
    !plan.previous_fits && (plan.rest.area().abs() - pocket().area().abs()).abs() < 1e-6
}

/// The unknown pin: a column with no readable top stands.
fn unknown_pin(g: Guards) -> bool {
    let hole = Point2::new(20.125, 15.125);
    let s = stock(-4.0, |c, z| if (c.x - hole.x).abs() < 1e-9 && (c.y - hole.y).abs() < 1e-9 { f64::NAN } else { z });
    let cols = s.columns().unwrap();
    let st = stock_from(&cols, &pocket(), &[-4.0], g).unwrap();
    st.standing_at(-4.0).unwrap().contains(hole)
}

/// The outside pin: the pocket outside the grid stands.
fn outside_pin(g: Guards) -> bool {
    let s = stock(-4.0, |_, z| z);
    let cols = s.columns().unwrap();
    let wide = rect(-6.0, 5.0, 35.0, 25.0);
    let st = stock_from(&cols, &wide, &[-4.0], g).unwrap();
    st.standing_at(-4.0).unwrap().contains(Point2::new(-3.0, 15.0))
}

/// The cell pin: a point in a cut column 0.1 mm from a standing column
/// is not proved clear.
fn cell_pin(g: Guards) -> bool {
    let s = stock(-4.0, |_, z| z);
    let cols = s.columns().unwrap();
    let st = stock_from(&cols, &pocket(), &[-4.0], g).unwrap();
    st.standing_at(-4.0).unwrap().contains(Point2::new(8.1, 15.0))
}

#[test]
fn each_reading_guard_is_pinned_and_its_removal_fails_the_pin() {
    let all = Guards::ALL;
    assert!(depth_pin(all), "every cut depth is read");
    assert!(!depth_pin(Guards { depth: false, ..all }), "without the depth guard the lower floor reads clear");
    assert!(unknown_pin(all), "an unreadable column stands");
    assert!(!unknown_pin(Guards { unknown: false, ..all }), "without the unknown guard a NaN column reads clear");
    assert!(outside_pin(all), "the pocket outside the grid stands");
    assert!(!outside_pin(Guards { outside: false, ..all }), "without the outside guard it reads clear");
    assert!(cell_pin(all), "the standing columns grow by a cell");
    assert!(!cell_pin(Guards { cell: false, ..all }), "without the cell guard the wall band reads clear");
}

#[test]
fn a_floor_cut_at_the_cut_depth_does_not_stand_and_one_above_it_does() {
    // A floor allowance of 0.2 mm is material at Z-4; float noise is not.
    for (floor, standing) in [(-4.0, false), (-4.0 + 1e-9, false), (-3.8, true)] {
        let s = stock(floor, |_, z| z);
        let cols = s.columns().unwrap();
        let st = rest_stock_from_columns(&cols, &pocket(), &[-4.0]).unwrap();
        assert_eq!(st.standing_at(-4.0).unwrap().contains(Point2::new(20.0, 15.0)), standing, "floor {floor}");
    }
}

#[test]
fn bad_input_is_refused_by_name() {
    let s = stock(-4.0, |_, z| z);
    let cols = s.columns().unwrap();
    let no_levels = rest_stock_from_columns(&cols, &pocket(), &[]).unwrap_err().to_string();
    assert!(no_levels.contains("finite cut depths"), "{no_levels}");
    let nan = rest_stock_from_columns(&cols, &pocket(), &[f64::NAN]).unwrap_err().to_string();
    assert!(nan.contains("finite cut depths"), "{nan}");
    assert!(rest_stock_from_columns(&cols, &Region::empty(), &[-4.0]).is_err());
    // A depth the stock was not read at is refused, never guessed.
    let st = rest_stock_from_columns(&cols, &pocket(), &[-2.0]).unwrap();
    let deep = rest_plan_stock(&pocket(), 4.0, 0.0, 0.0, &st, &[-2.0, -4.0]).unwrap_err().to_string();
    assert!(deep.contains("not proved at Z-4.000"), "{deep}");
}
