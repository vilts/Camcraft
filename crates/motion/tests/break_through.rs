//! #1557: path/depth helpers belong to the generator domain and are usable
//! without a document, ECS world, or app crate.
use camcraft_motion::break_through::{BELOW_STOCK_NOTE, deepest, judge};
use camcraft_motion::cl::Toolpath;
use camcraft_motion::geo2d::Point2;
use camcraft_motion::geometry::Point3;
use camcraft_motion::units::Unit;

#[test]
fn the_domain_helpers_pick_and_report_the_deepest_through_tip() {
    let broke = deepest([(Point2::new(1.0, 2.0), -0.3), (Point2::new(3.0, 4.0), -0.8)], 0.3).expect("through tips");
    assert_eq!(broke.tip, Point3::new(3.0, 4.0, -0.8));
    assert_eq!(broke.run, 0.3);
    let path = judge(Toolpath::new(Vec::new()), Some(broke), Some(0.0), false, Unit::Millimeters);
    assert!(path.notes.iter().any(|n| n.contains("run 0.300 mm past the part bottom") && n.contains("Z-0.800")), "{:?}", path.notes);
    assert!(path.notes.iter().any(|n| n.starts_with(BELOW_STOCK_NOTE) && n.contains("0.800 mm under the stock bottom Z0.000")), "{:?}", path.notes);
}
